#include "EditorAutomationHost.hpp"
#include "RavetAaxEditorControls.hpp"
#include "RavetAaxEffectParameters.hpp"
#include "RavetGain.h"
#include "AAX_VAutomationDelegate.h"
#include <iostream>
#include <stdexcept>
#include <thread>
#include <pthread.h>
#include <cmath>
namespace {
void require(bool value,const char* message){if(!value)throw std::runtime_error(message);}
bool mainThread(){return pthread_main_np()!=0;}
using BaseModel=ravet::aax::NativeAaxEffectParameters<2,32>;
struct Model:BaseModel{
 using BaseModel::BaseModel;
 mutable uint64_t badValueBits=0;mutable AAX_Result readStatus=0;bool badCount=false,badId=false;
 AAX_Result GetParameterNormalizedValue(AAX_CParamID id,double* value)const override{if(readStatus&&std::strcmp(id,"p2")==0)return readStatus;if(badValueBits){std::memcpy(value,&badValueBits,8);return 0;}return BaseModel::GetParameterNormalizedValue(id,value);}
 AAX_Result GetNumberOfParameters(int32_t* count)const override{if(badCount){*count=3;return 0;}return BaseModel::GetNumberOfParameters(count);}
 AAX_Result GetParameterIDFromIndex(int32_t index,AAX_IString* value)const override{if(badId){value->Set("p9");return 0;}return BaseModel::GetParameterIDFromIndex(index,value);}
};
using Controls=ravet::aax::AaxEditorControls<2>;using Error=Controls::Error;using Gesture=Controls::Gesture;
struct Instance{
 EditorAutomationHost host;Model model{{"org.ravet.gain.mono",1,0x52617674,0x5276476e,0x52764731,0x52765331},ravet_native_processor_v1(32),ravet_native_audio_state_v1(32),ravet_native_audio_state_v1(32)->cached_parameters_entry(),ravet_native_state_v1(32)};
 ravet::aax::NativeAaxParameters<2> schema{ravet_native_audio_state_v1(32)->cached_parameters_entry()};AAX_VAutomationDelegate automation{host.unknown()};Controls controls;
 static Model* initialized(Instance& instance){require(instance.model.Initialize(instance.host.unknown())==0,"actual SDK initialized owned model");return &instance.model;}
 Instance():controls(schema,initialized(*this),&automation,mainThread){require(controls.valid(),"exact model/schema editor control adoption");}
 ~Instance(){model.Uninitialize();}
 double level(){double value=-1;require(model.GetParameterNormalizedValue("p1",&value)==0,"actual normalized model read");return value;}
 void render(float expected){BaseModel::Owner* owner=nullptr;require(model.ResetFieldData(0,&owner,sizeof(owner))==0,"actual context owner");std::array<float,1025> in{},out{};in.fill(1);out.fill(99);float* inputs[]{in.data()};float* outputs[]{out.data()};int32_t frames=1024;BaseModel::Context context{&owner,&frames,inputs,outputs};BaseModel::Context* contexts[]{&context};for(int i=0;i<4;++i)BaseModel::process(contexts,contexts+1);require(std::abs(out[1023]-expected)<1e-6f&&out[1024]==99,"independent actual Rust Gain and backing tail");}
};
void contracts(){
 {Instance f;const auto before=f.level();f.render(1);require(f.controls.edit("p1",.125)==Error::Ok&&f.host.touches==1&&f.host.requests==1&&f.host.releases==1,"actual public versioned delegate balanced checked edit");require(f.level()==before,"accepted GUI host request alone does not change model");f.render(1);require(f.model.UpdateParameterNormalizedValue(f.host.requestedId.c_str(),f.host.requestedValue,AAX_eUpdateSource_Unspecified)==0,"separate host forwarding into model SPSC");f.render(.5f);auto d=f.controls.parameterDiagnosticsMain("p1");require(d.begin.returned&&d.value.returned&&d.end.returned&&d.begin.status==0&&d.value.status==0&&d.end.status==0&&d.gesture==Gesture::Idle,"original independent successful statuses");}
 {Instance f;for(const auto id:{"p9","01",static_cast<const char*>(nullptr)})require(f.controls.edit(id,.5)==Error::Invalid,"unknown ID rejects before host");double nan=0;uint64_t bits=UINT64_C(0x7ff8000000000001);std::memcpy(&nan,&bits,8);require(f.controls.edit("p1",nan)==Error::Invalid&&f.controls.edit("p1",-1)==Error::Invalid&&f.controls.edit("p1",2)==Error::Invalid&&f.host.requests==0&&f.host.touches==0,"whole late scalar preflight before touch/value/end");}
 {Instance f;f.host.setStatus=-4312;require(f.controls.edit("p1",.25)==Error::Sdk&&f.host.releases==1&&f.level()==.25,"refused host value still balances accepted begin and keeps old model");auto d=f.controls.parameterDiagnosticsMain("p1");require(d.value.returned&&d.value.status==-4312&&d.end.returned&&d.end.status==0&&d.gesture==Gesture::Idle,"successful end never overwrites value refusal");}
 {Instance f;f.host.releaseStatus=-4313;require(f.controls.edit("p1",.5)==Error::Sdk&&f.controls.parameterDiagnosticsMain("p1").gesture==Gesture::Unresolved,"refused end retains unresolved host gesture");const auto releases=f.host.releases;require(f.controls.retireMain()==Error::Unresolved&&f.host.releases==releases,"retirement does not replay uncertain end");require(f.controls.parameterDiagnosticsMain("p1").end.status==-4313,"original refused end remains observable");}
 {Instance f;f.host.touchStatus=-4311;require(f.controls.edit("p1",.5)==Error::Sdk&&f.host.requests==0&&f.host.releases==0&&f.controls.diagnosticsMain().unresolved==1,"begin refusal never invents successful touch");}
 {Instance f;f.host.throwSet=true;require(f.controls.edit("p1",.5)==Error::Exception&&f.host.releases==1,"unreturned value still balances accepted begin");auto d=f.controls.parameterDiagnosticsMain("p1");require(d.value.attempted&&!d.value.returned&&d.end.returned,"exception never fabricates original value status");}
 {Instance f;bool entered=false;Controls::Snapshot reentrySnapshot{};f.host.onSet=[&]{entered=true;require(f.controls.begin("p2")==Error::Busy&&f.controls.snapshot(reentrySnapshot)==Error::Busy,"reentrant controls do not borrow SDK again");};require(f.controls.edit("p1",.5)==Error::Ok&&entered&&f.host.requests==1,"actual public delegate callback reentry");f.host.onSet={};Error worker=Error::Ok;std::thread thread([&]{worker=f.controls.begin("p1");});thread.join();require(worker==Error::WrongThread&&f.host.touches==1,"wrong thread before host request");}
 {Instance f;Controls::Snapshot snapshot{};snapshot.count=77;snapshot.values.fill(81);f.model.readStatus=-5412;require(f.controls.snapshot(snapshot)==Error::Sdk&&snapshot.count==77&&snapshot.values[0]==81&&snapshot.values[1]==81&&f.controls.diagnosticsMain().read.status==-5412,"late getter failure preserves whole snapshot and original result");f.model.readStatus=0;for(auto bits:{UINT64_C(0x7ff0000000000000),UINT64_C(0x7ff8000000000001),UINT64_C(0xfff0000000000000)}){f.model.badValueBits=bits;require(f.controls.snapshot(snapshot)==Error::Invalid&&snapshot.count==77&&snapshot.values[1]==81,"foreign getter NaN/Inf cannot pass fast finite assumptions");}f.model.badValueBits=0;require(f.controls.snapshot(snapshot)==Error::Ok&&snapshot.count==2&&snapshot.values[0]==.25&&snapshot.values[1]==0,"complete owned normalized model snapshot");}
 {Instance f;require(f.controls.begin("p1")==Error::Ok&&f.controls.begin("p2")==Error::Ok,"independent active control gestures");f.controls.closeMain();require(f.controls.set("p1",.5)==Error::Closed&&f.controls.retireMain()==Error::Ok&&f.host.releases==2&&f.controls.diagnosticsMain().active==0,"bounded retirement ends accepted active begins");require(f.controls.retireMain()==Error::Ok&&f.host.releases==2,"repeated retirement does not replay ends");}
 {Instance f;f.host.onTouch=[&]{f.controls.closeMain();};require(f.controls.edit("p1",.5)==Error::Closed&&f.host.requests==0&&f.host.releases==0,"callback owner closes after touch before value");f.host.onTouch={};require(f.controls.retireMain()==Error::Ok&&f.host.releases==1,"retained callback context can retire accepted begin after return");}
 {Instance f;f.model.badCount=true;Controls wrong(f.schema,&f.model,&f.automation,mainThread);require(!wrong.valid()&&wrong.edit("p1",.5)==Error::NotReady,"complete exact catalogue count preflight");f.model.badCount=false;f.model.badId=true;Controls duplicate(f.schema,&f.model,&f.automation,mainThread);require(!duplicate.valid(),"exact stable ID order requires model/cache adoption");}
}
}
int main(){try{contracts();std::cout<<"Checked AAX editor control/main/model/DSP bridge PASS\n";return 0;}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
