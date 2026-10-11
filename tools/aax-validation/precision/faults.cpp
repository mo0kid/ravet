#include "TimingRegistrationFixture.hpp"
#include "ControllerHost.hpp"
#include "RavetAaxEffectParameters.hpp"
#include "RavetGain.h"
#include <iostream>
#include <memory>
#include <stdexcept>
namespace {
void require(bool value,const char* message){if(!value)throw std::runtime_error(message);}
using Model=ravet::aax::NativeAaxEffectParameters<2,32,false,1024,false,false,0,false,64>;
using Error=Model::Owner::PrecisionError;
struct Forward{
 static inline RavetNativeProcessorEntryV1 core{};static inline RavetNativeParameterEntryV1 parameters{};static inline RavetNativeCachedParameterEntryV1 cache{};static inline RavetNativeAudioStateEntryV1 audio{};static inline RavetNativeStateEntryV1 codec{};
 static inline int mode=0;static inline uint32_t nativeCalls=0,actualCalls=0,applies=0;
 static const RavetNativeProcessorEntryV1* coreOrigin(){return &core;}static const RavetNativeParameterEntryV1* parametersOrigin(){return &parameters;}static const RavetNativeCachedParameterEntryV1* cacheOrigin(){return &cache;}
 static int32_t apply(void* handle,const void* cached,const RavetNativeParameterValueV1* values,uint32_t count){++applies;return ravet_native_audio_state_v1(64)->apply(handle,cached,values,count);}
 static int32_t process(void* handle,const RavetNativeProcessorBusV1* buses,uint32_t count,uint32_t frames,const RavetNativeEventV1* events,uint32_t eventCount,const RavetNativeTransportV1* transport,RavetNativeEventV1* outputs,uint32_t capacity,RavetNativeProcessorResultV1* report){
  ++nativeCalls;if(mode==2)return -4177;if(mode==3)throw 17;++actualCalls;const auto status=ravet_native_processor_v1(64)->process(handle,buses,count,frames,events,eventCount,transport,outputs,capacity,report);if(status)return status;if(mode==4)throw 18;
  if(mode==1&&frames){auto samples=static_cast<double*const*>(buses[count-1].outputs);const uint64_t overflow=UINT64_C(0x7fefffffffffffff);std::memcpy(samples[buses[count-1].output_channels-1]+frames-1,&overflow,8);}return status;
 }
 static void initialize(){core=*ravet_native_processor_v1(64);core.process=process;audio=*ravet_native_audio_state_v1(64);audio.apply=apply;cache=*audio.cached_parameters_entry();parameters=*cache.parameters_entry();parameters.processor_entry=coreOrigin;cache.parameters_entry=parametersOrigin;audio.cached_parameters_entry=cacheOrigin;codec=*ravet_native_state_v1(64);codec.parameters_entry=parametersOrigin;}
};
AAX_IEffectParameters* AAX_CALLBACK create(){return new Model({"org.ravet.validation.precision",2,0x52617674,0x5276476e,0x52764732,0x52765331},&Forward::core,&Forward::audio,&Forward::cache,&Forward::codec);}
void contracts(){
 Forward::initialize();Collection registration;auto* descriptor=registration.NewDescriptor();require(descriptor&&descriptor->AddProcPtr(reinterpret_cast<void*>(create),kAAX_ProcPtrID_Create_EffectParameters)==0,"public registered owned native64 factory");auto* component=descriptor->NewComponentDescriptor();using Context=Model::Context;require(component->AddPrivateData(AAX_FIELD_INDEX(Context,owner),sizeof(Model::Owner*))==0&&component->AddAudioBufferLength(AAX_FIELD_INDEX(Context,frames))==0&&component->AddAudioIn(AAX_FIELD_INDEX(Context,inputs))==0&&component->AddAudioOut(AAX_FIELD_INDEX(Context,outputs))==0&&component->AddProcessProc_Native(reinterpret_cast<AAX_CProcessProc>(Model::process),component->NewPropertyMap())==0&&descriptor->AddComponent(component)==0&&registration.AddEffect("org.ravet.validation.precision",descriptor)==0,"real SDK callback registration unchanged hostfloat fields");
 ControllerHost host;host.effect="org.ravet.validation.precision";host.input=host.output=AAX_eStemFormat_Stereo;using Factory=AAX_IEffectParameters*(AAX_CALLBACK*)();std::unique_ptr<AAX_IEffectParameters> parameters(reinterpret_cast<Factory>(registration.descriptors[0].create)());require(parameters&&parameters->Initialize(host.unknown())==0,"actual registered factory SDK Initialize");auto* model=static_cast<Model*>(parameters.get());Model::Owner* owner=nullptr;require(parameters->ResetFieldData(0,&owner,sizeof(owner))==0&&owner,"registered exact64 native owner");
 std::array<float,1025> left{},right{},outLeft{},outRight{};left.fill(1);right.fill(2);outLeft.fill(91);outRight.fill(92);float* in[]{left.data(),right.data()};float* out[]{outLeft.data(),outRight.data()};int32_t frames=1024;Context context{&owner,&frames,in,out};void* contexts[]{&context};auto invoke=[&]{registration.descriptors[0].component.process(contexts,contexts+1);};
 Forward::mode=0;invoke();require(owner->lastProcessStatus()==0&&owner->lastNativeProcessStatus()==0&&outRight[1023]==2,"registered actual native64 successful reference");
 for(int mode:{1,2,3,4}){
  Forward::mode=0;require(model->reprepareStopped()==0,"fresh stopped authoritative prepare between faults");invoke();const auto previous=owner->lastNativeProcessStatus();require(parameters->UpdateParameterNormalizedValue("p1",.125,AAX_eUpdateSource_Unspecified)==0,"accepted global intent before native/conversion fault");outLeft.fill(91);outRight.fill(92);const auto actualBefore=Forward::actualCalls;Forward::mode=mode;invoke();const auto diagnostic=owner->precisionDiagnosticsStopped();require(!owner->ready()&&owner->lastProcessStatus()!=0&&diagnostic.nativeAttempted&&!diagnostic.samplesTransferred&&outLeft[0]==0&&outLeft[1023]==0&&outRight[0]==0&&outRight[1023]==0&&outLeft[1024]==91&&outRight[1024]==92,"fault before whole host publication, supported silence/tails and readiness poison");
  if(mode==1)require(diagnostic.nativeReturned&&diagnostic.nativeStatus==0&&owner->lastNativeProcessStatus()==0&&diagnostic.error==Error::NarrowOverflow&&diagnostic.overflow.bus==0&&diagnostic.overflow.channel==1&&diagnostic.overflow.frame==1023&&diagnostic.commandsConsumed&&Forward::actualCalls==actualBefore+1,"late last-sample overflow after actual native success, no history rollback or native-code substitution");
  if(mode==2)require(diagnostic.nativeReturned&&diagnostic.nativeStatus==-4177&&owner->lastNativeProcessStatus()==-4177&&diagnostic.error==Error::None&&!diagnostic.commandsConsumed&&Forward::actualCalls==actualBefore,"exact original refusal independent from local narrowing error");
  if(mode>=3)require(!diagnostic.nativeReturned&&diagnostic.error==Error::Exception&&owner->lastNativeProcessStatus()==previous&&Forward::actualCalls==actualBefore+uint32_t(mode==4),"unreturned throw retains prior returned native code, before/after actual DSP distinction");
  const auto calls=Forward::nativeCalls;invoke();require(Forward::nativeCalls==calls&&owner->lastNativeProcessStatus()==(mode==2?-4177:previous),"poison rejects follow-up before backend and preserves prior returned native status");Forward::mode=0;require(model->reprepareStopped()==0,"fresh stopped precision fault recovery retains intent");invoke();require(owner->lastProcessStatus()==0&&std::abs(outLeft[1023]-.5f)<1e-6f&&outLeft[1024]==91,"accepted intent source survives fault and fresh prepare, transient history not replayed");
 }
 // Raw signals propagate through actual Rust f64 Gain; the adapter has no finite
 // audio gate. Parameter/state/transport scalar rules remain independent.
 require(parameters->UpdateParameterNormalizedValue("p1",.25,AAX_eUpdateSource_Unspecified)==0&&model->reprepareStopped()==0,"unity intent restored");const uint32_t patterns[]{0,0x80000000u,1,0x007fffffu,0x7f800000u,0xff800000u,0x7f800001u,0xffc00021u};frames=8;for(uint32_t i=0;i<8;++i)std::memcpy(left.data()+i,patterns+i,4);invoke();require(owner->lastProcessStatus()==0,"actual native64 accepts all binary32 signal patterns");for(uint32_t i=0;i<8;++i){uint32_t value=0;std::memcpy(&value,outLeft.data()+i,4);const uint32_t expected=((patterns[i]&0x7f800000u)==0x7f800000u&&(patterns[i]&0x7fffffu))?patterns[i]|0x400000u:patterns[i];require(value==expected,"actual unity nonfinite/subnormal/signedzero bit policy");}
 require(parameters->Uninitialize()==0&&host.references==1,"actual public SDK borrower retirement after faults");
}
}
int main(){try{contracts();std::cout<<"Registered native64 whole overflow/native refusal/throw/poison/intent/nonfinite signal PASS\n";return 0;}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
