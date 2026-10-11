#include "RavetAaxProcessor.hpp"
#include "RavetGain.h"
#include "FaultTables.hpp"
#include <cmath>
#include <iostream>
#include <stdexcept>
#include <cstdlib>
#include <new>
namespace {bool measuring=false;uint64_t allocations=0,frees=0;}
void *operator new(size_t bytes){if(measuring)++allocations;if(auto *p=std::malloc(bytes?bytes:1))return p;throw std::bad_alloc();}
void *operator new[](size_t bytes){return ::operator new(bytes);}
void operator delete(void *p)noexcept{if(measuring&&p)++frees;std::free(p);}
void operator delete[](void *p)noexcept{::operator delete(p);}
void operator delete(void *p,size_t)noexcept{::operator delete(p);}
void operator delete[](void *p,size_t)noexcept{::operator delete(p);}
namespace {
void require(bool yes,const char *message){if(!yes)throw std::runtime_error(message);}
using Owner=ravet::aax::NativeAaxProcessor<2,4>;
Owner make() {return Owner(ravet_native_processor_v1(32),ravet_native_audio_state_v1(32),ravet_native_audio_state_v1(32)->cached_parameters_entry(),ravet_native_state_v1(32));}
using namespace ravet_aax_test;
void faultContracts(){
 initializeFaultTables();
 Owner owner(&faultCore,&faultAudio,&faultCache,&faultState);require(owner.valid(),"explicit matching synthetic companion provenance accepted around actual Gain callbacks");
 std::array<float,1026> input{},output{};input.fill(1.f);output.fill(77.f);const float *in[]={input.data()};float *out[]={output.data()};
 prepareFault=1;require(owner.prepare(1,48000)==1&&!owner.ready()&&prepareCalls==1&&applyCalls==0,"exact native prepare rejection remains unready without state apply");require(owner.process(in,out,1024)==1&&output[0]==0.f&&output[1024]==77.f&&!processCalls,"failed initial setup supported silence without foreign process");
 prepareFault=0;applyFault=2;require(owner.prepare(1,48000)==2&&!owner.ready(),"exact native initial state poison never publishes readiness");applyFault=0;require(owner.prepare(1,48000)==0,"fresh authoritative preparation recovers synthetic faults");
 Owner::Reservation reservation{};require(owner.stageParameter("p1",.125,reservation)&&owner.commit(reservation),"accepted intent before backend fault");
 const auto before=processCalls;processFault=1;require(owner.process(in,out,1024)==1&&!owner.ready()&&processCalls==before+1&&output[1023]==0.f,"invoked backend rejection fail-closes readiness and silences");require(owner.lastProcessStatus()==1,"native original process rejection retained atomically");processFault=0;require(owner.prepare(1,48000)==0&&owner.process(in,out,1024)==0&&output[1023]==.5f,"fresh prepare restores accepted canonical intent after backend rejection");
 processFault=2;require(owner.process(in,out,1024)==2&&!owner.ready()&&owner.lastProcessStatus()==2,"original native AUDIO poison retained exactly for control diagnostics");processFault=0;require(owner.prepare(1,48000)==0,"stopped recovery after AUDIO poison");
 resetFault=2;require(owner.reset()==2&&!owner.ready(),"exact reset poison retained fail-closed");resetFault=0;require(owner.prepare(1,48000)==0,"fresh prepare after reset fault");
 deactivateFault=1;require(owner.deactivate()==1&&!owner.ready(),"exact retirement error never restores readiness");deactivateFault=0;
 auto badState=faultState;badState.parameters_entry=[]()->const RavetNativeParameterEntryV1*{return ravet_native_parameters_v1(32);};Owner mismatched(&faultCore,&faultAudio,&faultCache,&badState);require(!mismatched.valid()&&mismatched.prepare(1,48000)==1,"late codec-origin mismatch rejects construction and setup");
}
void phaseAndHeap(){
 auto owner=make(),oracle=make();require(owner.prepare(2,48000)==0&&oracle.prepare(2,48000)==0,"phase oracle prepared");
 std::array<std::array<float,1026>,2> input{},a{},b{};for(auto &ch:input)ch.fill(1.f);for(auto &ch:a)ch.fill(77.f);for(auto &ch:b)ch.fill(77.f);const float *in[]={input[0].data(),input[1].data()};float *out[]={a[0].data(),a[1].data()},*expected[]={b[0].data(),b[1].data()};
 Owner::Reservation pending{},other{};require(owner.stageParameter("p1",.03125,pending)&&owner.commit(pending)&&oracle.stageParameter("p1",.03125,other)&&oracle.commit(other),"queued phase-oracle target");
 const auto *badInputs=reinterpret_cast<const float*const*>(&owner);const auto badOutputs=reinterpret_cast<float*const*>(&owner);require(owner.process(badInputs,out,17)==1&&owner.process(in,badOutputs,17)==1&&owner.ready(),"owner-overlapping pointer-array metadata rejects before any index/capture or DSP/queue mutation");
 require(owner.process(in,out,0)==1,"pending block-start commands require positive frames before codec or DSP transfer");
 float *metadata[]={reinterpret_cast<float*>(in),a[1].data()};const auto first=in[0],second=in[1];require(owner.process(in,metadata,17)==1&&in[0]==first&&in[1]==second,"late metadata alias rejection preserves input pointer storage");
 float *overlap[]={a[0].data(),a[0].data()+1};require(owner.process(in,overlap,17)==1,"late overlap rejects before DSP/queue consumption");require(owner.process(in,out,17)==0&&oracle.process(in,expected,17)==0,"phase-oracle matched successful callback");for(uint32_t ch=0;ch<2;++ch)require(!std::memcmp(a[ch].data(),b[ch].data(),17*sizeof(float)),"independent successful native history proves rejected block did not advance phase");
 measuring=true;for(uint32_t i=0;i<10000;++i){Owner::Reservation intent{};if(!owner.stageParameter("p1",i%2?.125:.75,intent)||!owner.commit(intent)||owner.process(in,out,1024)||owner.process(in,overlap,1024)!=1){measuring=false;throw std::runtime_error("repeated prepared valid/late-fault mapping");}}measuring=false;require(!allocations&&!frees,"prepared AUDIO cycles allocate/deallocate no C++ heap");
}
void contracts(){
 for(uint32_t channels:{1u,2u}){
  auto owner=make();require(owner.valid()&&!owner.ready()&&owner.prepare(channels,48000)==0&&owner.ready(),"authoritative native mono/stereo initial prepare/state success");
  std::array<std::array<float,1026>,2> inputs{},outputs{};const float *in[]={inputs[0].data(),inputs[1].data()};float *out[]={outputs[0].data(),outputs[1].data()};for(uint32_t ch=0;ch<channels;++ch){inputs[ch].fill(float(ch+1));outputs[ch].fill(77.f);}
  require(owner.process(in,out,1024)==0,"ordinary initial native realtime block");for(uint32_t ch=0;ch<channels;++ch)require(outputs[ch][0]==float(ch+1)&&outputs[ch][1023]==float(ch+1)&&outputs[ch][1024]==77.f,"initial main mono/stereo output and tails");
  Owner::Reservation pending{};require(owner.stageParameter("p1",.125,pending)&&pending.normalized==.125,"canonical parameter reservation");
  Owner::Reservation nested{};require(!owner.stageParameter("p1",.75,nested)&&owner.commit(pending)&&!owner.commit(pending),"nested/stale reservation guard");
  std::array<uint8_t,8208> saved{};require(owner.save(saved.data(),owner.stateBytes()),"canonical CONTROL intent saves before queued audio consumption");
  float *alias[]={outputs[0].data(),outputs[0].data()+1};if(channels==2){require(owner.process(in,alias,1024)==1,"late output partial overlap rejection before queued intent/DSP");require(outputs[0][1025]==77.f,"rejected backing tail preserved");}
  for(uint32_t cycle=0;cycle<4;++cycle)require(owner.process(in,out,1024)==0,"queued native control processing");for(uint32_t ch=0;ch<channels;++ch)require(std::abs(outputs[ch][1023]-.5f*float(ch+1))<1e-6f,"actual native smoothed Gain target");
  require(owner.stageParameter("p1",.75,pending)&&owner.commit(pending),"old parameter command before restore");require(owner.stageRestore(saved.data(),owner.stateBytes(),pending)&&owner.commit(pending),"complete decode restore command");require(owner.stageParameter("p2",1.,pending)&&owner.commit(pending),"source order trailing bypass intent");require(!owner.stageParameter("p1",.5,nested),"whole fixed capacity overflow rejects before overwrite");
  require(owner.process(in,out,1024)==0,"last complete restore supersedes preceding intent and preserves trailing controls");for(uint32_t ch=0;ch<channels;++ch)require(outputs[ch][1023]==float(ch+1),"restored snapshot then trailing bypass order");
  const auto original=saved; saved[0]^=1;require(!owner.stageRestore(saved.data(),owner.stateBytes(),pending),"foreign wrong-product state rejects before queue");saved=original;
  require(owner.reset()==0&&owner.ready()&&owner.process(in,out,1024)==0,"checked reset preserves latest accepted global intent");
  for(uint32_t ch=0;ch<channels;++ch)outputs[ch][1024]=77.f;
  require(owner.process(in,out,1025)==1&&outputs[0][1024]==77.f,"excess frames supported prefix silence and untouched tail");
  require(owner.deactivate()==0&&!owner.ready()&&!owner.stageParameter("p1",.5,pending),"authoritative retirement disables controls");require(owner.prepare(channels,48000)==0&&owner.process(in,out,1024)==0,"fresh stopped preparation restores accepted global intent");
 }
 auto invalid=make();require(invalid.prepare(3,48000)==1&&!invalid.ready(),"invalid main profile rejects before backend setup");
}
}
int main(){try{contracts();faultContracts();phaseAndHeap();std::cout<<"PASS prepared native AAX mono/stereo initial lifecycle, SPSC intent ordering/capacity, codec, independent phase oracle, checked backend faults and10000allocation-counted AUDIO cycles\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
