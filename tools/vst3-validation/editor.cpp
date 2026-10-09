// Actual exported VST3 bundle/editor validation in an owned synthetic Cocoa host.
#include "base/source/fobject.h"
#include "pluginterfaces/gui/iplugview.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "public.sdk/source/vst/hosting/parameterchanges.h"
#include <array>
#include <limits>
#include "public.sdk/source/vst/hosting/module.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include <cassert>
#include <iostream>
#include <vector>
using namespace Steinberg;
using namespace Steinberg::Vst;
extern "C" void* editor_probe_create();
extern "C" void* editor_probe_view(void*);
extern "C" void editor_probe_resize(void*,double,double);
extern "C" size_t editor_probe_children(void*);
extern "C" bool editor_probe_eval(void*,const char*);
extern "C" bool editor_probe_snapshot(void*,const char*);
extern "C" void editor_probe_pump(double);
extern "C" void editor_probe_destroy(void*);
struct Edit {int kind;ParamID id;double value;};
class Handler final : public FObject,public IComponentHandler {
public:
  std::vector<Edit> edits;
  bool rejectValue=false,rejectEnd=false;
  tresult PLUGIN_API beginEdit(ParamID id) override {edits.push_back({1,id,0});return kResultOk;}
  tresult PLUGIN_API performEdit(ParamID id,ParamValue value) override {edits.push_back({2,id,value});return rejectValue?kResultFalse:kResultOk;}
  tresult PLUGIN_API endEdit(ParamID id) override {edits.push_back({3,id,0});return rejectEnd?kResultFalse:kResultOk;}
  tresult PLUGIN_API restartComponent(int32) override {return kResultOk;}
  OBJ_METHODS(Handler,FObject)
  DEFINE_INTERFACES
    DEF_INTERFACE(IComponentHandler)
  END_DEFINE_INTERFACES(FObject)
  REFCOUNT_METHODS(FObject)
};
void ready(void* parent,const char* expression) {
  bool success=false;
  for(int i=0;i<60&&!success;++i) {editor_probe_pump(.05);success=editor_probe_eval(parent,expression);}
  assert(success);
}
int main(int argc,char** argv) {
  assert(argc>=2);
  void* parent=editor_probe_create();
  std::string error;
  auto module=VST3::Hosting::Module::create(argv[1],error);
  if(!module) {std::cerr<<error<<"\n";return 1;}
  HostApplication host;
  auto& factory=module->getFactory();factory.setHostContext(&host);
  IPtr<IEditController> controller;
  IPtr<IComponent> component;
  for(const auto& info:factory.classInfos()) {
    if(info.category()==kVstComponentControllerClass) controller=factory.createInstance<IEditController>(info.ID());
    if(info.category()==kVstAudioEffectClass) component=factory.createInstance<IComponent>(info.ID());
  }
  assert(controller && controller->initialize(&host)==kResultOk);
  assert(component && component->initialize(&host)==kResultOk);
  FUnknownPtr<IAudioProcessor> audio(component);
  ProcessSetup setup{kRealtime,kSample32,4,48000.};
  assert(audio && audio->setupProcessing(setup)==kResultOk && component->setActive(true)==kResultOk && audio->setProcessing(true)==kResultOk);
  auto handler=owned(new Handler);
  assert(controller->setComponentHandler(handler)==kResultOk);
  assert(!controller->createView("unsupported"));
  for(int cycle=0;cycle<3;++cycle) {
    auto view=owned(controller->createView(ViewType::kEditor));assert(view);
    assert(view->isPlatformTypeSupported(kPlatformTypeNSView)==kResultTrue);
    assert(view->isPlatformTypeSupported("unsupported")==kResultFalse);
    assert(view->attached(nullptr,kPlatformTypeNSView)==kResultFalse);
    ViewRect initial;assert(view->getSize(&initial)==kResultOk);
    editor_probe_resize(parent,initial.getWidth(),initial.getHeight());
    assert(view->attached(editor_probe_view(parent),kPlatformTypeNSView)==kResultOk);
    assert(editor_probe_children(parent)==1);
    ready(parent,"document.getElementById('gain-number')?.value==='1'");
    // Actual exported processor -> host output queues -> actual controller ->
    // Rust JSON -> native WKWebView, with diagnostic audio in disposable buffers.
    std::array<float,4> left{{std::numeric_limits<float>::quiet_NaN(),.25f,.25f,.25f}},right{{-.5f,-.5f,-.5f,-.5f}},outL{},outR{};
    float* inputs[]{left.data(),right.data()};float* outputs[]{outL.data(),outR.data()};
    AudioBusBuffers input{},output{};input.numChannels=output.numChannels=2;input.channelBuffers32=inputs;output.channelBuffers32=outputs;
    ParameterChanges changes(12);ProcessData data{};data.symbolicSampleSize=kSample32;data.numSamples=4;data.numInputs=data.numOutputs=1;data.inputs=&input;data.outputs=&output;data.outputParameterChanges=&changes;
    assert(audio->process(data)==kResultOk && changes.getParameterCount()==12);
    for(int32 i=0;i<12;++i) {
      auto* queue=changes.getParameterData(i);int32 offset=-1;double value=-1;
      assert(queue && queue->getPointCount()==1 && queue->getPoint(0,offset,value)==kResultOk && offset==3);
      assert(controller->setParamNormalized(queue->getParameterId(),value)==kResultOk);
    }
    ready(parent,"document.getElementById('meters').hidden===false && Math.abs(document.getElementById('meter-0').value-Math.sqrt(0.75)*0.25)<1e-6 && document.getElementById('meter-3').value===0.5 && document.getElementById('reading-0').textContent.includes('invalid audio') && document.getElementById('meter-3').getAttribute('aria-label')==='Output R audio level'");
    handler->edits.clear();
    assert(editor_probe_eval(parent,"(()=>{const n=document.getElementById('gain-number');n.value='3';n.dispatchEvent(new Event('change'));return true;})()"));
    editor_probe_pump(.15);
    assert(controller->getParamNormalized(1)==.75);
    assert(handler->edits.size()==3 && handler->edits[0].kind==1 && handler->edits[1].kind==2 && handler->edits[1].id==1 && handler->edits[1].value==.75 && handler->edits[2].kind==3);
    handler->edits.clear();
    assert(editor_probe_eval(parent,"(()=>{const r=document.getElementById('gain');r.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight'}));r.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp'}));r.value='2';r.dispatchEvent(new Event('input'));r.dispatchEvent(new KeyboardEvent('keyup',{key:'Shift'}));r.dispatchEvent(new KeyboardEvent('keyup',{key:'ArrowRight'}));return true;})()"));
    editor_probe_pump(.1);
    assert(handler->edits.size()==2 && handler->edits[0].kind==1 && handler->edits[1].kind==2);
    assert(editor_probe_eval(parent,"document.getElementById('gain').dispatchEvent(new KeyboardEvent('keyup',{key:'ArrowUp'}));true"));
    editor_probe_pump(.1);
    assert(handler->edits.size()==3 && handler->edits.back().kind==3);
    handler->edits.clear();
    assert(editor_probe_eval(parent,"document.getElementById('bypass').click();true"));
    editor_probe_pump(.1);
    assert(controller->getParamNormalized(2)==1.);
    assert(handler->edits.size()==3 && handler->edits[1].id==2);
    handler->edits.clear();
    assert(controller->setParamNormalized(1,.125)==kResultOk);
    assert(controller->setParamNormalized(2,0)==kResultOk);
    ready(parent,"document.getElementById('gain-number').value==='0.5'&&!document.getElementById('bypass').checked");
    assert(handler->edits.empty());
    handler->rejectValue=true;
    assert(editor_probe_eval(parent,"(()=>{const n=document.getElementById('gain-number');n.value='4';n.dispatchEvent(new Event('change'));return true;})()"));
    ready(parent,"document.getElementById('status').textContent==='Edit rejected'");
    assert(controller->getParamNormalized(1)==.125);
    handler->rejectValue=false;
    handler->rejectEnd=true;
    assert(editor_probe_eval(parent,"(()=>{const r=document.getElementById('gain');r.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight'}));r.value='1';r.dispatchEvent(new Event('input'));r.dispatchEvent(new KeyboardEvent('keyup',{key:'ArrowRight'}));return true;})()"));
    ready(parent,"document.getElementById('status').textContent==='Gesture end rejected'");
    assert(view->removed()==kResultFalse && editor_probe_children(parent)==1);
    handler->rejectEnd=false;
    ready(parent,"document.getElementById('status').textContent==='Ready'");
    assert(controller->setParamNormalized(1,.125)==kResultOk);
    ready(parent,"document.getElementById('gain-number').value==='0.5'");
    ViewRect size(0,0,640,480);
    assert(view->canResize()==kResultTrue && view->onSize(&size)==kResultOk);
    ViewRect actual;assert(view->getSize(&actual)==kResultOk && actual.getWidth()==640);
    ViewRect invalid(0,0,1,1);assert(view->onSize(&invalid)==kResultFalse);
    assert(view->checkSizeConstraint(&invalid)==kResultTrue && invalid.getWidth()==360 && invalid.getHeight()==300);
    if(cycle==0 && argc>=3)assert(editor_probe_snapshot(parent,argv[2]));
    assert(view->removed()==kResultOk && editor_probe_children(parent)==0);
    // Reattach the same IPlugView without retaining a native child.
    assert(view->attached(editor_probe_view(parent),kPlatformTypeNSView)==kResultOk);
    ready(parent,"document.getElementById('gain-number')?.value==='0.5'");
    assert(view->removed()==kResultOk && editor_probe_children(parent)==0);
    view=nullptr;
    editor_probe_pump(.1);
    controller->setParamNormalized(1,.25);
  }
  assert(audio->setProcessing(false)==kResultOk && component->setActive(false)==kResultOk);
  component->terminate();audio=nullptr;component=nullptr;
  controller->setComponentHandler(nullptr);
  controller->terminate();controller=nullptr;handler=nullptr;
  factory.setHostContext(nullptr);module=nullptr;
  editor_probe_destroy(parent);
  std::cout<<"PASS actual VST3 bundle Cocoa editor: three lifecycle cycles, reattachment, gain/bypass gestures, host meter rendering, held keys, host refresh, rejected edit rollback and resizing\n";
}
