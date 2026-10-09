// Actual exported reference bundle; no copy of its processor/controller or Rust ABI.
#include "base/source/fobject.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/vst/ivstunits.h"
#include "public.sdk/source/vst/hosting/module.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/parameterchanges.h"
#include "public.sdk/source/common/memorystream.h"
#include <algorithm>
#include <array>
#include <cassert>
#include <cmath>
#include <cstring>
#include <iostream>
#include <limits>
using namespace Steinberg;
using namespace Steinberg::Vst;
class Handler final : public FObject, public IComponentHandler {
public:
  int calls=0; bool reject=false;
  IEditController* controller=nullptr;bool reenter=false;
  tresult PLUGIN_API beginEdit(ParamID) override {return kResultOk;}
  tresult PLUGIN_API performEdit(ParamID,ParamValue) override {return kResultOk;}
  tresult PLUGIN_API endEdit(ParamID) override {return kResultOk;}
  tresult PLUGIN_API restartComponent(int32 flags) override {
    assert(flags==kParamValuesChanged);++calls;
    if(reenter) {
      assert(controller && controller->getParamNormalized(1)==0.);
      assert(controller->setParamNormalized(1,.9)==kResultFalse);
      assert(controller->setParamNormalized(3,1.)==kResultFalse);
    }
    return reject?kResultFalse:kResultOk;
  }
  OBJ_METHODS(Handler,FObject)
  DEFINE_INTERFACES
    DEF_INTERFACE(IComponentHandler)
  END_DEFINE_INTERFACES(FObject)
  REFCOUNT_METHODS(FObject)
};
void snapshot(IComponent* component,float gain,uint32 bypass) {
  MemoryStream stream;assert(component->getState(&stream)==kResultOk);
  assert(stream.getSize()==32);
  auto* data=reinterpret_cast<const uint8*>(stream.getData());
  assert(std::memcmp(data,"RAVETGN\0",8)==0);
  uint32 version,count,id1,id2;float actualGain,actualBypass;
  std::memcpy(&version,data+8,4);std::memcpy(&count,data+12,4);
  std::memcpy(&id1,data+16,4);std::memcpy(&actualGain,data+20,4);
  std::memcpy(&id2,data+24,4);std::memcpy(&actualBypass,data+28,4);
  assert(version==2 && count==2 && id1==1 && id2==2);
  assert(actualGain==gain && actualBypass==float(bypass));
}
void unity(IComponent* component) {
  // Public complete state fixture, little-endian macOS validation target.
  std::array<uint8,32> data{};std::memcpy(data.data(),"RAVETGN\0",8);
  uint32 version=2,count=2,id1=1,id2=2;float gain=1.f;
  std::memcpy(data.data()+8,&version,4);std::memcpy(data.data()+12,&count,4);
  std::memcpy(data.data()+16,&id1,4);std::memcpy(data.data()+20,&gain,4);
  std::memcpy(data.data()+24,&id2,4);
  MemoryStream stream(data.data(),data.size());assert(component->setState(&stream)==kResultOk);
}
int main(int argc,char** argv) {
  assert(argc==2);std::string error;
  auto module=VST3::Hosting::Module::create(argv[1],error);
  if(!module){std::cerr<<error<<"\n";return 1;}
  HostApplication host;auto& factory=module->getFactory();factory.setHostContext(&host);
  IPtr<IComponent> component;IPtr<IEditController> controller;
  for(const auto& info:factory.classInfos()) {
    if(info.category()==kVstAudioEffectClass) component=factory.createInstance<IComponent>(info.ID());
    if(info.category()==kVstComponentControllerClass) controller=factory.createInstance<IEditController>(info.ID());
  }
  assert(component && controller && component->initialize(&host)==kResultOk && controller->initialize(&host)==kResultOk);
  FUnknownPtr<IAudioProcessor> audio(component);FUnknownPtr<IUnitInfo> units(controller);
  assert(audio && units && controller->getParameterCount()==
#if defined(RAVET_NATIVE_WEB_EDITOR)
  15
#else
  3
#endif
);
  assert(units->getUnitCount()==1 && units->getProgramListCount()==1);
  UnitInfo unit{};ProgramListInfo list{};
  assert(units->getUnitInfo(0,unit)==kResultOk && unit.id==kRootUnitId && unit.parentUnitId==kNoParentUnitId && unit.programListId==3);
  assert(units->getProgramListInfo(0,list)==kResultOk && list.id==3 && list.programCount==3);
  const char16* names[]={u"Unity",u"Silence",u"Bypass"};
  for(int32 index=0;index<3;++index) {
    String128 name{};assert(units->getProgramName(3,index,name)==kResultOk);
    assert(std::char_traits<char16>::compare(name,names[index],std::char_traits<char16>::length(names[index])+1)==0);
  }
  String128 invalidName{};assert(units->getProgramName(3,3,invalidName)!=kResultOk);
  ParameterInfo parameter{};assert(controller->getParameterInfo(2,parameter)==kResultOk);
  assert(parameter.id==3 && parameter.unitId==kRootUnitId && parameter.stepCount==2);
  assert((parameter.flags&ParameterInfo::kIsProgramChange) && !(parameter.flags&ParameterInfo::kCanAutomate));
  auto handler=owned(new Handler);assert(controller->setComponentHandler(handler)==kResultOk);
  const auto previousProgram=controller->getParamNormalized(3),previousGain=controller->getParamNormalized(1),previousBypass=controller->getParamNormalized(2);
  for(double bad:{-1.,2.,std::numeric_limits<double>::quiet_NaN()}) assert(controller->setParamNormalized(3,bad)==kResultFalse);
  assert(handler->calls==0);
  handler->controller=controller;handler->reenter=true;
  handler->reject=true;assert(controller->setParamNormalized(3,.5)==kResultFalse);
  assert(controller->getParamNormalized(3)==previousProgram && controller->getParamNormalized(1)==previousGain && controller->getParamNormalized(2)==previousBypass);
  handler->reject=false;handler->reenter=false;
  SpeakerArrangement layout=SpeakerArr::kMono;assert(audio->setBusArrangements(&layout,1,&layout,1)==kResultOk);
  unsigned blocks=0;
  for(int cycle=0;cycle<3;++cycle) for(int32 precision:{kSample32,kSample64}) {
    ProcessSetup setup{kRealtime,precision,1024,48000.};assert(audio->setupProcessing(setup)==kResultOk);
    assert(component->setActive(true)==kResultOk && audio->setProcessing(true)==kResultOk);
    std::array<float,1024> input32{},output32{};std::array<double,1024> input64{},output64{};
    input32.fill(1.f);input64.fill(1.);
    float* i32[]={input32.data()};float* o32[]={output32.data()};
    double* i64[]={input64.data()};double* o64[]={output64.data()};
    AudioBusBuffers input{},output{};input.numChannels=output.numChannels=1;
    if(precision==kSample32){input.channelBuffers32=i32;output.channelBuffers32=o32;}
    else {input.channelBuffers64=i64;output.channelBuffers64=o64;}
    ProcessData data{};data.symbolicSampleSize=precision;data.numSamples=512;data.numInputs=data.numOutputs=1;data.inputs=&input;data.outputs=&output;
    auto sample=[&](size_t index){return precision==kSample32?double(output32[index]):output64[index];};
    auto render=[&](IParameterChanges* changes,tresult expected=kResultOk){data.inputParameterChanges=changes;assert(audio->process(data)==expected);++blocks;};
    auto controls=[&](ParameterChanges& changes){int32 index;for(ParamID id:{1u,2u}) changes.addParameterData(id,index)->addPoint(0,controller->getParamNormalized(id),index);};
    unity(component);ParameterChanges changes(2);
    assert(controller->setParamNormalized(3,.5)==kResultOk);
    assert(controller->getParamNormalized(1)==0. && controller->getParamNormalized(2)==0.);
    controls(changes);render(&changes);
    for(size_t i=240;i<512;++i) assert(sample(i)==0.);
    snapshot(component,0.f,0);
    changes.clearQueue();assert(controller->setParamNormalized(3,1.)==kResultOk);controls(changes);render(&changes);
    for(size_t i=0;i<512;++i) assert(sample(i)==1.);
    snapshot(component,1.f,1);
    unity(component);changes.clearQueue();int32 index;
    auto* queue=changes.addParameterData(3,index);queue->addPoint(3,.5,index);queue->addPoint(300,1.,index);render(&changes);
    for(size_t i=0;i<3;++i) assert(sample(i)==1.);
    for(size_t i=250;i<300;++i) assert(sample(i)==0.);
    for(size_t i=300;i<512;++i) assert(sample(i)==1.);
    snapshot(component,1.f,1);
    unity(component);data.numSamples=600;changes.clearQueue();queue=changes.addParameterData(3,index);
    for(int32 point=0;point<513;++point) queue->addPoint(point,1.,index);
    render(&changes,kResultFalse);for(size_t i=0;i<600;++i) assert(sample(i)==0.);snapshot(component,1.f,0);
    changes.clearQueue();queue=changes.addParameterData(3,index);
    for(int32 point=0;point<512;++point) queue->addPoint(point,1.,index);
    render(&changes);snapshot(component,1.f,1);
    changes.clearQueue();queue=changes.addParameterData(3,index);queue->addPoint(0,.5,index);
    data.numSamples=0;data.numInputs=data.numOutputs=0;data.inputs=data.outputs=nullptr;
    render(&changes);snapshot(component,0.f,0);
    assert(audio->setProcessing(false)==kResultOk && component->setActive(false)==kResultOk);
  }
  assert(controller->setComponentHandler(nullptr)==kResultOk);
  assert(component->terminate()==kResultOk && controller->terminate()==kResultOk);
  controller=nullptr;audio=nullptr;units=nullptr;component=nullptr;factory.setHostContext(nullptr);
  std::cout<<"PASS actual VST3 programs: unit/list/selector, rejected notification rollback, complete host controls, timed f32/f64 state/audio and expanded event bounds; "<<blocks<<" blocks/six activations\n";
}
