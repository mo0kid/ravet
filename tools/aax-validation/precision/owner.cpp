#include "RavetAaxProcessor.hpp"
#include "RavetAaxEffectParameters.hpp"
#include "RavetGain.h"
#include <iostream>
#include <memory>
#include <stdexcept>
using Owner=ravet::aax::NativeAaxProcessor<2,32,false,1024,false,0,false,64>;
using Model=ravet::aax::NativeAaxEffectParameters<2,32,false,1024,false,false,0,false,64>;
static_assert(sizeof(Owner)>1024*1024&&sizeof(ravet::aax::NativeAaxProcessor<2,32>)<1024*1024);
void require(bool value,const char* message){if(!value)throw std::runtime_error(message);}
int main(){try{
 const auto* audio=ravet_native_audio_state_v1(64);auto owner=std::make_unique<Owner>(ravet_native_processor_v1(64),audio,audio->cached_parameters_entry(),ravet_native_state_v1(64));require(owner->valid()&&owner->prepare(2,48000)==0,"exact authoritative native64 Gain setup");std::array<float,1025> first{},second{};first.fill(1);second.fill(2);const float* inputs[]{first.data(),second.data()};float* outputs[]{second.data(),first.data()};require(owner->process(inputs,outputs,1024)==0&&second[1023]==1&&first[1023]==2&&first[1024]==1&&second[1024]==2,"actual f64 native one DSP cross-channel aliases/backing tails");Owner::Reservation reservation;require(owner->stageParameter("p1",.125,reservation)&&owner->commit(reservation),"schema64 actual f32 Level event");first.fill(1);second.fill(2);for(int i=0;i<4;++i){require(owner->process(inputs,outputs,1024)==0,"native64 block-start controls");first.fill(1);second.fill(2);}float* independent[]{second.data()};const float* mono[]{first.data()};require(owner->prepare(1,48000)==0&&owner->process(mono,independent,1024)==0&&second[1023]==.5f,"complete intent recalls through native64 reprepare");auto diag=owner->precisionDiagnosticsStopped();require(diag.nativeAttempted&&diag.nativeReturned&&diag.nativeStatus==0&&diag.samplesTransferred&&owner->lastNativeProcessStatus()==0,"native return retained separately from conversion status");require(owner->process(nullptr,nullptr,0)==0,"zero frame native64 authoritative acceptance");auto mismatched=std::make_unique<Owner>(ravet_native_processor_v1(32),ravet_native_audio_state_v1(32),ravet_native_audio_state_v1(32)->cached_parameters_entry(),ravet_native_state_v1(32));require(!mismatched->valid(),"explicit64 origin rejects native32, no export inference");std::cout<<"Actual native64 Gain state/control aliases/tails/zero/provenance PASS\n";return 0;
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
