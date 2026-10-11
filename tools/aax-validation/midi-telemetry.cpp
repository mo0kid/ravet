#include "RavetAaxProcessor.hpp"
#include "RavetMidiSine.h"
#include "RavetPerformance.h"
#include <iostream>
#include <stdexcept>
#include <array>
#include <atomic>
#include <thread>
#include <cstring>
namespace {
const RavetNativeCachedParameterEntryV1 *original=nullptr;RavetNativeCachedParameterEntryV1 cache{};RavetNativeAudioStateEntryV1 audio{};RavetNativeTelemetryEntryV1 telemetry{};
// Whole owner preflight needs coherent authored schema/cache/codec origins.
RavetNativeParameterEntryV1 proxyParameters{};RavetNativeStateEntryV1 proxyState{};
const RavetNativeParameterEntryV1* parametersOrigin(){return &proxyParameters;}
RavetNativeProcessorEntryV1 copiedCore{};const RavetNativeProcessorEntryV1 *copiedOrigin(){return &copiedCore;}
uint32_t acquired=0,retired=0,telemetryCreates=0;bool throwing=false;
void *createParameters(){auto *p=original->create();if(p)++acquired;return p;}
void destroyParameters(void *p){if(p)++retired;original->destroy(p);}
const RavetNativeCachedParameterEntryV1 *cacheEntry(){return &cache;}
void *createTelemetry(){++telemetryCreates;if(throwing)throw std::runtime_error("synthetic control acquisition throw");return nullptr;}
void require(bool value,const char *message){if(!value)throw std::runtime_error(message);}
}
namespace raw_status_fixture {
RavetNativeProcessorEntryV1 core{};RavetNativeParameterEntryV1 parameters{};RavetNativeCachedParameterEntryV1 cache{};RavetNativeAudioStateEntryV1 audio{};RavetNativeStateEntryV1 state{};RavetNativeTelemetryEntryV1 telemetry{};const RavetNativeProcessorEntryV1* actual=nullptr;int32_t status=0;bool throws=false;uint32_t calls=0;std::atomic<bool> entered{false},released{false};void (*hook)()=nullptr;
const RavetNativeProcessorEntryV1* coreOrigin(){return &core;}const RavetNativeParameterEntryV1* parametersOrigin(){return &parameters;}const RavetNativeCachedParameterEntryV1* cacheOrigin(){return &cache;}
int32_t process(void* owner,const RavetNativeProcessorBusV1* buses,uint32_t count,uint32_t frames,const RavetNativeEventV1* events,uint32_t eventCount,const RavetNativeTransportV1* transport,RavetNativeEventV1* output,uint32_t capacity,RavetNativeProcessorResultV1* report){++calls;if(hook)hook();if(throws)throw 97;if(status)return status;return actual->process(owner,buses,count,frames,events,eventCount,transport,output,capacity,report);}
template<uint32_t Bits>void contracts(){actual=ravet_midi_sine_processor_v1(Bits);core=*actual;core.process=process;parameters=*ravet_midi_sine_parameters_v1(Bits);parameters.processor_entry=coreOrigin;cache=*ravet_midi_sine_cached_parameters_v1(Bits);cache.parameters_entry=parametersOrigin;audio=*ravet_midi_sine_audio_state_v1(Bits);audio.cached_parameters_entry=cacheOrigin;state=*ravet_midi_sine_state_v1(Bits);state.parameters_entry=parametersOrigin;telemetry=*ravet_midi_sine_telemetry_v1(Bits);telemetry.processor_entry=coreOrigin;
using Owner=ravet::aax::NativeAaxProcessor<1,32,true,1024,false,0,false,Bits,true>;auto enqueue=[](Owner& target,double value){typename Owner::Reservation reservation{};return target.stageParameter("p1",value,reservation)&&target.commit(reservation);};auto restoreState=[](Owner& target,const uint8_t* bytes,uint32_t length){typename Owner::Reservation reservation{};return target.stageRestore(bytes,length,reservation)&&target.commit(reservation);};auto owner=std::make_unique<Owner>(&core,&audio,&cache,&state,&telemetry);require(owner->valid()&&owner->prepare(1,48000.)==0,"coherent native raw status fixture acquired actual canonical Rust processor");std::array<float,10> samples{};samples.fill(61.f);float* outputs[]={samples.data()};AAX_CMidiStream stream{};status=79;throws=false;calls=0;
require(owner->processNotes(outputs,8,&stream)==79&&owner->lastNativeProcessStatus()==79&&!owner->ready()&&samples[0]==0.f&&samples[8]==61.f,"raw native32/64 retains original79 refusal and safe prefix/tail");auto d=owner->rawDiagnosticsStopped();require(d.nativeAttempted&&d.nativeReturned&&d.nativeStatus==79&&!d.commandsConsumed&&!d.samplesTransferred,"raw returned failure has exact per-block provenance");
status=0;throws=true;require(owner->prepare(1,48000.)==0&&owner->processNotes(outputs,8,&stream)==1&&owner->lastNativeProcessStatus()==79&&!owner->ready(),"raw violating throw never substitutes prior actually returned native status");d=owner->rawDiagnosticsStopped();require(d.error==Owner::PrecisionError::Exception&&d.nativeAttempted&&!d.nativeReturned&&!d.commandsConsumed&&!d.samplesTransferred&&calls==2,"raw exception diagnostic distinguishes unreturned native attempt");throws=false;
// Separate healthy owners prove controls published after the DSP queue snapshot
// stay pending and are consumed in source order only by the following block.
auto pending=std::make_unique<Owner>(&core,&audio,&cache,&state,&telemetry);auto reference=std::make_unique<Owner>(&core,&audio,&cache,&state,&telemetry);require(pending->valid()&&reference->valid()&&!pending->prepare(1,48000.)&&!reference->prepare(1,48000.),"raw32/64 mid-DSP healthy owners");std::array<float,10> expected{};expected.fill(61.f);float* refOutputs[]={expected.data()};AAX_CMidiPacket note{0,3,{0x90,69,127,0},0};AAX_CMidiStream notes{1,&note};require(!pending->processNotes(outputs,8,&notes)&&!reference->processNotes(refOutputs,8,&notes),"actual audible raw voice before concurrent queue publication");
std::array<uint8_t,24> restore{};std::memcpy(restore.data(),"RAVETMS\0",8);const uint32_t version=1,id=1;const float level=.3f;std::memcpy(restore.data()+8,&version,4);std::memcpy(restore.data()+12,&id,4);std::memcpy(restore.data()+16,&id,4);std::memcpy(restore.data()+20,&level,4);
entered=false;released=false;hook=[](){entered.store(true,std::memory_order_release);while(!released.load(std::memory_order_acquire))std::this_thread::yield();};std::thread worker([&]{require(!pending->processNotes(outputs,8,&stream),"admitted block inside actual native process");});while(!entered.load(std::memory_order_acquire))std::this_thread::yield();const bool accepted=enqueue(*pending,.75)&&restoreState(*pending,restore.data(),restore.size());released.store(true,std::memory_order_release);worker.join();hook=nullptr;require(accepted&&!reference->processNotes(refOutputs,8,&stream)&&samples==expected,"mid-DSP scalar and distinct complete restore accepted without changing captured current voice audio");std::array<RavetNativeTelemetryValueV1,23> observed{};require(!pending->snapshotTelemetryStopped(observed.data(),observed.size())&&observed[6].value==double(.2f),"current admitted DSP retains prior acknowledged Level before queue snapshot publication");require(enqueue(*reference,.75)&&restoreState(*reference,restore.data(),restore.size())&&!pending->processNotes(outputs,8,&stream)&&!reference->processNotes(refOutputs,8,&stream)&&samples==expected,"next block applies complete restore after earlier scalar with independent exact voice-phase audio");require(!pending->snapshotTelemetryStopped(observed.data(),observed.size())&&observed[6].value==double(level),"distinct complete restore consumed rather than discarded and native Level telemetry coherent");
// Stress one CONTROL producer against AUDIO consuming only immutable admitted
// queue slots. No CONTROL observation touches AUDIO-owned diagnostic storage.
std::atomic<bool> done{false};std::atomic<uint32_t> acceptedCount{0};std::thread producer([&]{for(uint32_t i=0;i<1000;++i){const double value=(i%8)/8.;while(!enqueue(*pending,value))std::this_thread::yield();acceptedCount.fetch_add(1,std::memory_order_relaxed);}while(!restoreState(*pending,restore.data(),restore.size()))std::this_thread::yield();done.store(true,std::memory_order_release);});while(!done.load(std::memory_order_acquire))require(!pending->processNotes(outputs,1,&stream),"SPSC AUDIO immutable queue stress");producer.join();for(uint32_t i=0;i<32;++i)require(!pending->processNotes(outputs,1,&stream),"SPSC trailing queue drain");require(acceptedCount==1000&&!pending->snapshotTelemetryStopped(observed.data(),observed.size())&&observed[6].value==double(level),"concurrent thousand scalar publications plus distinct last complete restore converge to exact native Level");
}
}
int main(){try{
 for(uint32_t bits:{32u,64u}){
  const auto *core=ravet_midi_sine_processor_v1(bits);original=ravet_midi_sine_cached_parameters_v1(bits);cache=*original;cache.create=createParameters;cache.destroy=destroyParameters;proxyParameters=*original->parameters_entry();cache.parameters_entry=parametersOrigin;proxyState=*ravet_midi_sine_state_v1(bits);proxyState.parameters_entry=parametersOrigin;audio=*ravet_midi_sine_audio_state_v1(bits);audio.cached_parameters_entry=cacheEntry;telemetry=*ravet_midi_sine_telemetry_v1(bits);telemetry.create_cache=createTelemetry;
  for(bool fault:{false,true}){throwing=fault;acquired=retired=telemetryCreates=0;{
    ravet::native::NativeProcessorClient owner(core,&audio,&telemetry);require(!owner.valid()&&acquired==1&&telemetryCreates==1,"failed/throwing telemetry cache retains no valid processor");
   }require(acquired==retired,"every acquired parameter cache retired on failed/throwing telemetry creation");}
  acquired=retired=telemetryCreates=0;telemetry.version=99;{
    ravet::native::NativeProcessorClient owner(core,&audio,&telemetry);require(!owner.valid()&&acquired==0&&telemetryCreates==0,"late invalid companion metadata preflights before resource acquisition");
   }require(retired==0,"invalid table has no ownership to retire");
  const auto validTelemetry=*ravet_midi_sine_telemetry_v1(bits);
  for(uint32_t fault=0;fault<8;++fault){telemetry=validTelemetry;acquired=retired=telemetryCreates=0;
   switch(fault){case 0:telemetry.byte_size=0;break;case 1:telemetry.sample_bits=bits==32?64:32;break;case 2:telemetry.telemetry_count=1025;break;case 3:telemetry.processor_entry=nullptr;break;case 4:telemetry.create_cache=nullptr;break;case 5:telemetry.destroy_cache=nullptr;break;case 6:telemetry.info=nullptr;break;default:telemetry.snapshot=nullptr;break;}
   {ravet::native::NativeProcessorClient owner(core,&audio,&telemetry);require(!owner.valid()&&acquired==0&&retired==0,"every companion shape/function fault rejects before CONTROL resource acquisition");}
  }
  telemetry=validTelemetry;telemetry.version=99;
  using Owner32=ravet::aax::NativeAaxProcessor<1,32,true,1024,true,0,false,32,true>;using Owner64=ravet::aax::NativeAaxProcessor<1,32,true,1024,true,0,false,64,true>;
  for(bool fault:{false,true}){telemetry=validTelemetry;telemetry.create_cache=createTelemetry;throwing=fault;acquired=retired=telemetryCreates=0;
   if(bits==32){Owner32 owner(core,&audio,&cache,&proxyState,&telemetry);require(!owner.valid(),"owned raw32 failed/throwing telemetry remains invalid");}
   else{auto owner=std::make_unique<Owner64>(core,&audio,&cache,&proxyState,&telemetry);require(!owner->valid(),"owned raw64 failed/throwing telemetry remains invalid");}
   require(acquired==2&&retired==acquired&&telemetryCreates==1,"owned schema and client parameter caches balanced after telemetry refusal/throw");
  }
  copiedCore=*core;telemetry=validTelemetry;telemetry.processor_entry=copiedOrigin;acquired=retired=telemetryCreates=0;
  if(bits==32){Owner32 owner(core,&audio,&cache,&proxyState,&telemetry);require(!owner.valid()&&acquired==0,"raw32 equivalent copied telemetry origin rejects before schema state acquisition");}
  else{auto owner=std::make_unique<Owner64>(core,&audio,&cache,&proxyState,&telemetry);require(!owner->valid()&&acquired==0,"raw64 equivalent copied telemetry origin rejects before schema state acquisition");}
  telemetry=validTelemetry;telemetry.version=99;acquired=retired=telemetryCreates=0;
  if(bits==32){Owner32 owner(core,&audio,&cache,&proxyState,&telemetry);require(!owner.valid()&&acquired==0&&telemetryCreates==0,"optional malformed telemetry fails whole owner before schema/cache state mutation");}
  else{auto owner=std::make_unique<Owner64>(core,&audio,&cache,&proxyState,&telemetry);require(!owner->valid()&&acquired==0&&telemetryCreates==0,"selected64 malformed telemetry preflight before schema/cache acquisition");}
 }
 using Legacy32=ravet::aax::NativeAaxProcessor<1,32,true,1024,true,0,false,32>;
 using Legacy64=ravet::aax::NativeAaxProcessor<1,32,true,1024,true,0,false,64>;
 auto legacy=[&](auto &owner){require(owner.valid()&&!owner.prepare(1,48000.),"unchanged default note profile prepares with null telemetry");
  std::array<float,66> output{};output.fill(77.f);float *outputs[]={output.data()};AAX_CMidiPacket note{0,3,{0x90,69,127,0},0};AAX_CMidiStream stream{1,&note};
  require(owner.processNotes(outputs,0,&stream)==1&&owner.ready()&&output[0]==77.f&&output[65]==77.f,"default note zero-frame event remains refused without output/history mutation");
  stream.mBufferSize=0;require(owner.processNotes(outputs,0,&stream)==0,"default empty-only zero-frame remains accepted");
  stream.mBufferSize=1;require(owner.processNotes(outputs,64,&stream)==0&&output[20]!=0.f&&output[64]==77.f,"unchanged default note native32/64 remains audible with backing tail");};
 {Legacy32 owner(ravet_performance_processor_v1(32),ravet_performance_audio_state_v1(32),ravet_performance_cached_parameters_v1(32),ravet_performance_state_v1(32));legacy(owner);}
 {auto owner=std::make_unique<Legacy64>(ravet_performance_processor_v1(64),ravet_performance_audio_state_v1(64),ravet_performance_cached_parameters_v1(64),ravet_performance_state_v1(64));legacy(*owner);}
 raw_status_fixture::contracts<32>();raw_status_fixture::contracts<64>();
 std::cout<<"PASS optional telemetry whole preflight and null/throw ownership32/64\n";return 0;
 }catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}}
