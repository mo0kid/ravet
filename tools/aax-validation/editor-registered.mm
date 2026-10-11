#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include "EditorAutomationHost.hpp"
#include "EditorRegistrationFixture.hpp"
#include "AAX_IEffectParameters.h"
#include "AAX_IEffectGUI.h"
#include "AAX_IACFViewContainer.h"
#include <dlfcn.h>
#include <iostream>
#include <cmath>
#include <thread>
#include <chrono>
namespace {
void require(bool yes,const char* message){if(!yes)throw std::runtime_error(message);}
struct View final:AAX_IACFViewContainer{
 NSView* parent;acfUInt32 refs=1;
 explicit View(NSView* p):parent(p){}
 ACFRESULT ACFMETHODCALLTYPE QueryInterface(const acfIID& iid,void** out)override{if(!out)return ACF_E_INVALIDARG;*out=nullptr;if(std::memcmp(&iid,&IID_IAAXViewContainerV1,sizeof(iid)))return ACF_E_NOINTERFACE;*out=static_cast<AAX_IACFViewContainer*>(this);AddRef();return 0;}
 acfUInt32 ACFMETHODCALLTYPE AddRef()override{return ++refs;}acfUInt32 ACFMETHODCALLTYPE Release()override{return --refs;}
 int32_t GetType()override{return AAX_eViewContainer_Type_NSView;}void* GetPtr()override{return (__bridge void*)parent;}
 AAX_Result GetModifiers(uint32_t* v)override{if(v)*v=0;return v?0:AAX_ERROR_NULL_ARGUMENT;}
 AAX_Result SetViewSize(AAX_Point& v)override{parent.frame=NSMakeRect(0,0,v.horz,v.vert);return 0;}
 AAX_Result HandleParameterMouseDown(AAX_CParamID,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result HandleParameterMouseDrag(AAX_CParamID,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_Result HandleParameterMouseUp(AAX_CParamID,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
};
WKWebView* web(NSView* parent){for(NSView* child in parent.subviews){if([child isKindOfClass:WKWebView.class])return static_cast<WKWebView*>(child);if(auto* v=web(child))return v;}return nil;}
void pump(){[[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:.02]];}
template<class F>void until(F&& test,const char* message){const auto deadline=std::chrono::steady_clock::now()+std::chrono::seconds(10);while(!test()&&std::chrono::steady_clock::now()<deadline)pump();require(test(),message);}
void send(NSView* parent,NSString* script){auto* child=web(parent);require(child,"actual packaged native child");__block bool done=false,ok=false;[child evaluateJavaScript:script completionHandler:^(id,NSError* error){done=true;ok=!error;}];const auto deadline=std::chrono::steady_clock::now()+std::chrono::seconds(10);while(!done&&std::chrono::steady_clock::now()<deadline)pump();require(done,"packaged native callback JavaScript completes");require(ok,"packaged trusted HTML loaded");}
void contracts(const char* path,bool headless){
 [NSApplication sharedApplication];[NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
 auto* module=dlopen(path,RTLD_NOW|RTLD_LOCAL);if(!module)throw std::runtime_error(dlerror());
 using Describe=AAX_Result(*)(AAX_ICollection*);auto describe=reinterpret_cast<Describe>(dlsym(module,"_Z21GetEffectDescriptionsP15AAX_ICollection"));require(describe,"actual module public descriptor export");
 using namespace ravet_aax_editor_test;
 ravet_aax_editor_test::Collection refused;refused.descriptors[0].guiStatus=-6121;if(!headless)require(describe(&refused)==-6121&&refused.effects.empty()&&refused.used==1,"GUI callback first refusal preserves exact status and registration source order");
 ravet_aax_editor_test::Collection registered;require(describe(&registered)==0&&registered.effects.size()==2,"actual optional GUI exact mono/stereo effects registered");
 if(headless){for(const auto& descriptor:registered.descriptors)require(!descriptor.gui,"default headless descriptor must not advertise GUI callback from unused editor exports");require(dlclose(module)==0,"default headless module closed");return;}
 for(uint32_t channels:{1u,2u}){
  auto& descriptor=registered.descriptors[channels-1];require(descriptor.gui&&descriptor.create&&descriptor.component.process,"actual descriptor has distinct GUI/model/algorithm callbacks");
  EditorAutomationHost host;host.effect=channels==1?"org.ravet.gain.mono":"org.ravet.gain.stereo";host.input=host.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;
  require(registered.effects[channels-1].first==host.effect,"explicit public identity selects mono/stereo profile");
  using Factory=AAX_IEffectParameters*(AAX_CALLBACK*)();auto* model=reinterpret_cast<Factory>(descriptor.create)();require(model,"actual registered owned model");model->AddRef();host.modelInterface=model;const auto initialized=model->Initialize(host.unknown());require(initialized==0,"real SDK initialized actual packaged model");
  using GuiFactory=AAX_IEffectGUI*(AAX_CALLBACK*)();auto* gui=reinterpret_cast<GuiFactory>(descriptor.gui)();require(gui,"actual registered editor factory");gui->AddRef();require(gui->Initialize(host.unknown())==0,"actual public GUI Initialize model query ownership");
  NSView* parent=[[NSView alloc] initWithFrame:NSMakeRect(0,0,540,340)];View view(parent);require(gui->SetViewContainer(&view)==0&&parent.subviews.count==1&&view.refs==2,"actual packaged NSView balanced owned container attach");
  until([&]{return web(parent).loading==NO;},"actual packaged trusted HTML load");send(parent,@"window.webkit.messageHandlers.ravet.postMessage({type:'edit',id:1,value:0.5});true");until([&]{return host.requests==1;},"actual packaged UI edit reaches checked host delegate");require(host.touches==1&&host.releases==1&&host.requestedValue==.125,"actual packaged UI gestures and physical normalization");
  double prior=-1;require(model->GetParameterNormalizedValue("p1",&prior)==0&&prior==.25,"post acceptance alone retains model intent");
  void* owner=nullptr;require(model->ResetFieldData(0,&owner,sizeof(owner))==0&&owner,"actual initialized model algorithm owner");std::array<std::array<float,1025>,2> inputs{},outputs{};for(uint32_t ch=0;ch<channels;++ch){inputs[ch].fill(float(ch+1));outputs[ch].fill(87);}
  float* in[]{inputs[0].data(),inputs[1].data()};float* out[]{outputs[0].data(),outputs[1].data()};int32_t frames=1024;void* context[]{&owner,&frames,in,out};void* contexts[]{context};descriptor.component.process(contexts,contexts+1);require(outputs[0][1023]==1,"actual algorithm before accepted host forwarding");
  require(model->UpdateParameterNormalizedValue(host.requestedId.c_str(),host.requestedValue,AAX_eUpdateSource_Unspecified)==0,"actual host forwarding after native GUI returns");for(int i=0;i<4;++i)descriptor.component.process(contexts,contexts+1);for(uint32_t ch=0;ch<channels;++ch)require(std::abs(outputs[ch][1023]-.5f*float(ch+1))<1e-6f&&outputs[ch][1024]==87,"independent packaged mono/stereo UI/model/native PCM and backing tail");
  host.setStatus=-6122;send(parent,@"window.webkit.messageHandlers.ravet.postMessage({type:'edit',id:1,value:3});true");until([&]{return host.requests==2;},"actual packaged host refusal attempted");require(host.touches==2&&host.releases==2,"actual refused value still ends accepted begin");double retained=-1;require(model->GetParameterNormalizedValue("p1",&retained)==0&&retained==.125,"host refusal preserves accepted model target");
  AAX_Result wrong=0;std::thread worker([&]{wrong=gui->Uninitialize();});worker.join();require(wrong!=0&&view.refs==2&&parent.subviews.count==1,"packaged wrong-thread teardown retains owned refs");
  require(gui->Uninitialize()==0&&view.refs==1&&parent.subviews.count==0,"actual packaged native child detach and balanced public container retirement");gui->Release();host.modelInterface=nullptr;require(model->Uninitialize()==0&&host.references==1,"actual packaged GUI/model public borrower retirement");model->Release();
 }
 require(dlclose(module)==0,"actual module unload after views/callbacks/models");
}
}
int main(int argc,char** argv){@autoreleasepool{try{require(argc==2||(argc==3&&std::strcmp(argv[2],"--headless")==0),"actual optional editor module required");contracts(argv[1],argc==3);std::cout<<(argc==3?"Default headless AAX descriptors do not advertise GUI callbacks PASS\n":"Actual registered AAX GUI factories NSView checked gestures model/native mono/stereo PCM PASS\n");return 0;}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}}
