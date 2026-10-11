#include "ControllerHost.hpp"
#include "RavetAaxEffectParameters.hpp"
#include "RavetGain.h"
#include <iostream>
#include <stdexcept>
using Model=ravet::aax::NativeAaxEffectParameters<2,4>;
void require(bool value,const char *message){if(!value)throw std::runtime_error(message);}
int main(){try{
 for(uint32_t channels:{1u,2u}){
  ControllerHost host;host.input=host.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;host.effect=channels==1?"org.ravet.gain.mono":"org.ravet.gain.stereo";
  Model model({host.effect.c_str(),channels,0x52617674,0x5276476e,channels==1?0x52764731u:0x52764732u,0x52765331},ravet_native_processor_v1(32),ravet_native_audio_state_v1(32),ravet_native_audio_state_v1(32)->cached_parameters_entry(),ravet_native_state_v1(32));
  require(model.Initialize(host.unknown())==AAX_SUCCESS&&model.nativeStatus()==0,"real SDK native model Initialize prepares ordinary main realtime profile");
  Model::Owner *owner=nullptr;require(model.ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS&&owner,"owned public context publish");
  std::array<std::array<float,1026>,2> inputs{},outputs{};float *in[]={inputs[0].data(),inputs[1].data()},*out[]={outputs[0].data(),outputs[1].data()};for(uint32_t ch=0;ch<channels;++ch){inputs[ch].fill(float(ch+1));outputs[ch].fill(77.f);}
  int32_t frames=1024;Model::Context context{&owner,&frames,in,out};Model::Context *contexts[]={&context};Model::process(contexts,contexts+1);for(uint32_t ch=0;ch<channels;++ch)require(outputs[ch][1023]==float(ch+1)&&outputs[ch][1024]==77.f,"native model mono/stereo first-block realtime and tails");
  bool reentered=false;
  host.onCurrent=[&]{reentered=true;require(model.Uninitialize()==AAX_ERROR_INVALID_ARGUMENT&&model.UpdateParameterNormalizedValue("p2",1.,AAX_eUpdateSource_Unspecified)==AAX_ERROR_INVALID_ARGUMENT,"foreign notification reentry cannot retire borrowed SDK objects or steal queue reservation");};
  require(model.UpdateParameterNormalizedValue("p1",.125,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"acknowledged SDK/native queue control");
  host.onCurrent={};require(reentered&&host.references==3&&host.registered.size()==2,"outer SDK/native control remains valid after guarded reentry");
  for(int i=0;i<4;++i)Model::process(contexts,contexts+1);for(uint32_t ch=0;ch<channels;++ch)require(std::abs(outputs[ch][1023]-.5f*float(ch+1))<1e-6f,"native model actual Gain target");
  require(model.resetStopped()==AAX_SUCCESS&&model.reprepareStopped()==AAX_SUCCESS,"authoritative checked native stopped reset/reprepare");
  require(model.Uninitialize()==AAX_SUCCESS&&host.references==1&&host.registered.empty(),"native model checked retirement releases SDK borrower objects");
 }
 for(int fault=0;fault<8;++fault){
  ControllerHost host;host.effect="org.ravet.gain.mono";
  if(fault==0)host.rateStatus=AAX_ERROR_UNIMPLEMENTED;
  if(fault==1)host.inputStatus=AAX_ERROR_UNIMPLEMENTED;
  if(fault==2)host.outputStatus=AAX_ERROR_UNIMPLEMENTED;
  if(fault==3)host.rate=0.f;
  if(fault==4){uint32_t bits=0x7fc00001;std::memcpy(&host.rate,&bits,4);}
  if(fault==5)host.output=AAX_eStemFormat_Stereo;
  if(fault==6)host.effect="org.ravet.gain.stereo";
  if(fault==7)host.exposeController=false;
  Model model({"org.ravet.gain.mono",1,0x52617674,0x5276476e,0x52764731,0x52765331},ravet_native_processor_v1(32),ravet_native_audio_state_v1(32),ravet_native_audio_state_v1(32)->cached_parameters_entry(),ravet_native_state_v1(32));
  const auto status=model.Initialize(host.unknown());require(status==(fault<3?AAX_ERROR_UNIMPLEMENTED:(fault==7?AAX_ERROR_NULL_OBJECT:AAX_ERROR_INVALID_ARGUMENT))&&host.registered.empty(),"original host errors/exact rate/stem/effect preflight before native/manager publication");
  Model::Owner *owner=nullptr;require(model.ResetFieldData(0,&owner,sizeof(owner))==AAX_ERROR_NULL_OBJECT&&!owner,"failed Initialize never publishes processing-ready owner");
  require(model.Uninitialize()==AAX_SUCCESS&&host.references==1,"failed Initialize releases actual SDK proxies");
 }
 std::cout<<"PASS real SDK native AAX model initial mono/stereo1024 realtime, acknowledged controls, checked reset/reprepare/retirement\n";return 0;
}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
