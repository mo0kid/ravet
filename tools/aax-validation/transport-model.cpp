#include "TransportControllerHost.hpp"
#include "RavetAaxEffectParameters.hpp"
#include "RavetGain.h"
#include "FaultTables.hpp"
#include <cmath>
#include <iostream>
#include <stdexcept>
using Model=ravet::aax::NativeAaxEffectParameters<2,4,false,1024,true>;
void require(bool value,const char *message){if(!value)throw std::runtime_error(message);}
namespace {
RavetNativeTransportV1 observed{};bool observedKnown=false;uint32_t captureCalls=0;
int32_t capture(void *owner,const RavetNativeProcessorBusV1 *buses,uint32_t busCount,uint32_t frames,const RavetNativeEventV1 *events,uint32_t count,const RavetNativeTransportV1 *transport,RavetNativeEventV1 *output,uint32_t capacity,RavetNativeProcessorResultV1 *result){++captureCalls;observedKnown=transport!=nullptr;if(transport)observed=*transport;return ravet_aax_test::faultyProcess(owner,buses,busCount,frames,events,count,transport,output,capacity,result);}
}
int main(){try{
 using namespace ravet_aax_test;initializeFaultTables();faultCore.process=capture;
 for(uint32_t channels:{1u,2u}){
  TransportControllerHost host,referenceHost;host.input=host.output=referenceHost.input=referenceHost.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;host.effect=referenceHost.effect=channels==1?"org.ravet.gain.mono":"org.ravet.gain.stereo";
  const Model::Identity identity{host.effect.c_str(),channels,0x52617674,0x5276476e,channels==1?0x52764731u:0x52764732u,0x52765331};
  Model model(identity,&faultCore,&faultAudio,&faultCache,&faultState),reference(identity,&faultCore,&faultAudio,&faultCache,&faultState);
  require(model.Initialize(host.unknown())==AAX_SUCCESS&&reference.Initialize(referenceHost.unknown())==AAX_SUCCESS&&host.references==4,"real SDK Initialize creates retained versioned V1 transport proxy");
  Model::Owner *owner=nullptr,*oracle=nullptr;require(model.ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS&&reference.ResetFieldData(0,&oracle,sizeof(oracle))==AAX_SUCCESS,"checked ready capture owner publication");
  std::array<std::array<float,1026>,2> input{},a{},b{};for(auto &ch:input)ch.fill(1.f);for(auto &ch:a)ch.fill(77.f);for(auto &ch:b)ch.fill(77.f);float *in[]={input[0].data(),input[1].data()},*out[]={a[0].data(),a[1].data()},*expected[]={b[0].data(),b[1].data()};int32_t frames=17;Model::Context context{&owner,&frames,in,out},referenceContext{&oracle,&frames,in,expected};Model::Context *contexts[]={&context},*referenceContexts[]={&referenceContext};
  Model::process(contexts,contexts+1);require(model.lastProcessStatus()==0&&observedKnown&&observed.sample_position==123&&observed.beat_position==1.&&observed.tempo==120.&&model.transportSnapshotStopped().available()==127,"actual initialized SDK proxy fields forwarded through exact-origin wrapper to authoritative Rust native process");for(auto count:host.transportCalls)require(count==1,"actual SDK proxy each required getter once");
  Model::process(referenceContexts,referenceContexts+1);
  require(model.UpdateParameterNormalizedValue("p1",.03125,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&reference.UpdateParameterNormalizedValue("p1",.03125,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"pending Gain before malformed last transport tuple");
  std::array<uint8_t,32> state{},oracleState{};require(owner->save(state.data(),32)&&oracle->save(oracleState.data(),32),"save complete queued intent before transport rejection");Model::Owner::Reservation restore{},oracleRestore{};require(owner->stageRestore(state.data(),32,restore)&&owner->commit(restore)&&oracle->stageRestore(oracleState.data(),32,oracleRestore)&&oracle->commit(oracleRestore),"queue complete native restore after scalar intent");
  host.looping=true;host.loopStart=INT64_MAX-1;host.loopEnd=INT64_MAX;const auto processBefore=captureCalls,applyBefore=applyCalls;Model::process(contexts,contexts+1);require(model.lastProcessStatus()==1&&owner->ready()&&captureCalls==processBefore&&applyCalls==applyBefore&&a[0][0]==0.f&&a[0][17]==77.f,"collapsed late transport projection preflights before native process/state/queue, supported prefix silence/tail");
  const auto fault=model.transportResultStopped();require(fault.fault==ravet::aax::AaxTransportFault::InvalidProjection&&fault.getter==ravet::aax::AaxTransportGetter::Loop&&fault.returned==127&&fault.statuses[5]==AAX_SUCCESS,"original successful SDK statuses distinct from local malformed projection status");
  host.looping=false;Model::process(contexts,contexts+1);Model::process(referenceContexts,referenceContexts+1);for(uint32_t ch=0;ch<channels;++ch)require(!std::memcmp(a[ch].data(),b[ch].data(),17*sizeof(float)),"independent successful native smoothing oracle proves transport failure retained pending Gain/history");
  host.transportStatuses[1]=AAX_ERROR_UNIMPLEMENTED;Model::process(contexts,contexts+1);require(model.lastProcessStatus()==0&&!(observed.present&RAVET_TRANSPORT_TEMPO)&&model.transportSnapshotStopped().status(ravet::aax::AaxTransportGetter::Tempo)==AAX_ERROR_UNIMPLEMENTED,"actual versioned proxy independent SDK getter failure preserved with other native fields");
  require(model.Uninitialize()==AAX_SUCCESS&&reference.Uninitialize()==AAX_SUCCESS&&host.references==1&&host.registered.empty(),"SDK transport borrower detached before proxies retire");
 }
 TransportControllerHost absent;absent.exposeTransport=false;Model model({"org.ravet.gain.mono",1,0x52617674,0x5276476e,0x52764731,0x52765331},&faultCore,&faultAudio,&faultCache,&faultState);require(model.Initialize(absent.unknown())==AAX_SUCCESS&&absent.references==3,"missing host interface keeps valid initialized wrapper without new retained host ref");Model::Owner *owner=nullptr;require(model.ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS,"missing transport still ready processing owner");float input=1.f,output=77.f;float *in[]={&input},*out[]={&output};int32_t frames=1;Model::Context context{&owner,&frames,in,out};Model::Context *contexts[]={&context};Model::process(contexts,contexts+1);require(model.lastProcessStatus()==0&&!observedKnown&&model.transportSnapshotStopped().available()==0,"absent underlying versioned interface forwards nullptr, no invented context");require(model.Uninitialize()==AAX_SUCCESS&&absent.references==1,"missing interface ordinary retirement");
 std::cout<<"PASS real initialized versioned AAX transport proxy lifetime, exact-origin native forwarding, original query statuses, collapsed-loop pre-state/DSP rejection and independent Gain history oracle\n";return 0;
 }catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}}
