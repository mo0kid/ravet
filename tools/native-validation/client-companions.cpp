#include "RavetNativeClient.hpp"
#include <array>
#include <cstring>
#include <iostream>
#include <stdexcept>
extern "C" {
const RavetNativeProcessorEntryV1 *ravet_native_processor_v1(uint32_t);
const RavetNativeParameterEntryV1 *ravet_native_parameters_v1(uint32_t);
const RavetNativeCachedParameterEntryV1 *ravet_native_cached_parameters_v1(uint32_t);
const RavetNativeAudioStateEntryV1 *ravet_native_audio_state_v1(uint32_t);
const RavetNativeTelemetryEntryV1 *ravet_native_telemetry_v1(uint32_t);
}
namespace {
void require(bool value,const char *message){if(!value)throw std::runtime_error(message);}
const RavetNativeProcessorEntryV1 *originalCore=nullptr;
const RavetNativeCachedParameterEntryV1 *originalCache=nullptr;
const RavetNativeTelemetryEntryV1 *originalTelemetry=nullptr;
RavetNativeProcessorEntryV1 core{};RavetNativeParameterEntryV1 parameters{};
RavetNativeCachedParameterEntryV1 cache{};RavetNativeAudioStateEntryV1 audio{};
RavetNativeTelemetryEntryV1 telemetry{};
uint32_t acquired[3]{},retired[3]{},calls[3]{},queries[4]{};int failure=-1,queryFailure=-1;bool throws=false;
void query(uint32_t stage){++queries[stage];if(queryFailure==int(stage))throw std::runtime_error("violating foreign metadata query");}
const RavetNativeProcessorEntryV1 *coreEntry(){query(2);return &core;}
const RavetNativeProcessorEntryV1 *telemetryCoreEntry(){query(3);return &core;}
const RavetNativeParameterEntryV1 *parameterEntry(){query(1);return &parameters;}
const RavetNativeCachedParameterEntryV1 *cacheEntry(){query(0);return &cache;}
bool refuse(uint32_t stage){++calls[stage];if(failure!=int(stage))return false;if(throws)throw std::runtime_error("violating foreign CONTROL acquisition");return true;}
void *createCache(){if(refuse(0))return nullptr;auto *p=originalCache->create();if(p)++acquired[0];return p;}
void destroyCache(void *p){if(p)++retired[0];originalCache->destroy(p);}
void *createTelemetry(){if(refuse(1))return nullptr;auto *p=originalTelemetry->create_cache();if(p)++acquired[1];return p;}
void destroyTelemetry(void *p){if(p)++retired[1];originalTelemetry->destroy_cache(p);}
void *createCore(){if(refuse(2))return nullptr;auto *p=originalCore->create();if(p)++acquired[2];return p;}
void destroyCore(void *p){if(p)++retired[2];originalCore->destroy(p);}
void clear(){for(auto &n:acquired)n=0;for(auto &n:retired)n=0;for(auto &n:calls)n=0;for(auto &n:queries)n=0;}
void prepareSynthetic(uint32_t bits){
 originalCore=ravet_native_processor_v1(bits);originalCache=ravet_native_cached_parameters_v1(bits);originalTelemetry=ravet_native_telemetry_v1(bits);
 core=*originalCore;core.create=createCore;core.destroy=destroyCore;
 parameters=*ravet_native_parameters_v1(bits);parameters.processor_entry=coreEntry;
 cache=*originalCache;cache.parameters_entry=parameterEntry;cache.create=createCache;cache.destroy=destroyCache;
 audio=*ravet_native_audio_state_v1(bits);audio.cached_parameters_entry=cacheEntry;
 telemetry=*originalTelemetry;telemetry.processor_entry=telemetryCoreEntry;telemetry.create_cache=createTelemetry;telemetry.destroy_cache=destroyTelemetry;
}
template<class S>void genuine(uint32_t bits){
 const auto *exported=ravet_native_processor_v1(bits);const auto *state=ravet_native_audio_state_v1(bits);const auto *meter=ravet_native_telemetry_v1(bits);
 auto copied=*exported;require(!std::memcmp(&copied,exported,sizeof(copied)),"negative core is byte-identical, not a changed callback");
 {ravet::native::NativeProcessorClient rejected(&copied,state,meter);require(!rejected.valid(),"copied equivalent core refuses exact state origin");}
 {ravet::native::NativeProcessorClient rejected(&copied,nullptr,meter);require(!rejected.valid(),"telemetry-only copied equivalent core refuses exact origin");}
 ravet::native::NativeProcessorClient owner(exported,state,meter);require(owner.valid()&&owner.sampleBits()==bits&&owner.hasAudioState()&&owner.hasTelemetry(),"genuine exported selected32/64 exact companions admitted");
 RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=8;config.maximum_events=4;config.bus_count=1;config.sample_rate=48000.;config.buses[0]={1,1};require(!owner.prepare(config),"actual native client prepare");
 std::array<S,9> input{},output{};for(auto &v:input)v=S(.25);output.fill(S(77));const void *inputs[]={input.data()};void *outputs[]={output.data()};RavetNativeProcessorBusV1 bus{1,1,inputs,outputs};RavetNativeProcessorResultV1 result{};
 require(!owner.process(&bus,1,8,nullptr,0,nullptr,nullptr,0,result)&&output[0]!=S(77)&&output[8]==S(77),"actual paired typed processing preserves backing tail");
 std::array<RavetNativeParameterValueV1,2> values{};std::array<RavetNativeTelemetryValueV1,12> snapshot{};
 require(!owner.snapshotState(values.data(),2)&&!owner.applyState(values.data(),2)&&!owner.snapshotTelemetry(snapshot.data(),12),"actual native state and telemetry caches remain paired");
 require(!owner.reset()&&!owner.deactivate(),"actual native client stopped lifecycle");
}
}
int main(){try{
 genuine<float>(32);genuine<double>(64);
 for(uint32_t bits:{32u,64u}){prepareSynthetic(bits);
  auto copy=core;require(!std::memcmp(&copy,&core,sizeof(copy)),"synthetic negative retains identical creator callbacks/counters");
  for(bool statePresent:{true,false}){clear();{
   ravet::native::NativeProcessorClient owner(&copy,statePresent?&audio:nullptr,&telemetry);require(!owner.valid(),"copied synthetic origin refuses state or telemetry-only admission");
  }for(uint32_t i=0;i<3;++i)require(calls[i]==0&&acquired[i]==0&&retired[i]==0,"exact copied-origin refusal invokes no foreign resource creator or retirement");}
  for(int stage=0;stage<4;++stage){clear();queryFailure=stage;{
   ravet::native::NativeProcessorClient owner(&core,&audio,&telemetry);require(!owner.valid()&&queries[stage]==1,"violating state/cache/schema/telemetry metadata query contained");
  }for(uint32_t i=0;i<3;++i)require(calls[i]==0&&acquired[i]==0&&retired[i]==0,"throwing metadata preflight has no resource ownership or creator side effects");}
  queryFailure=-1;
  for(int stage=0;stage<3;++stage)for(bool throwing:{false,true}){
   failure=stage;throws=throwing;clear();{
    ravet::native::NativeProcessorClient owner(&core,&audio,&telemetry);require(!owner.valid(),"null/throw acquisition remains invalid with no fallback");
   }for(uint32_t i=0;i<3;++i)require(acquired[i]==retired[i],"every earlier acquired CONTROL resource balanced");
   require(calls[stage]==1&&(stage==2||calls[stage+1]==0),"original null/throw stops later foreign acquisition");
  }
  failure=-1;throws=false;clear();telemetry.snapshot=nullptr;{
   ravet::native::NativeProcessorClient owner(&core,&audio,&telemetry);require(!owner.valid()&&calls[0]==0&&calls[1]==0&&calls[2]==0,"late malformed companion preflights before all resource creation");
  }
  prepareSynthetic(bits);failure=-1;clear();{
   ravet::native::NativeProcessorClient owner(&core,&audio,&telemetry);require(owner.valid(),"coherent exact public synthetic origin accepts");
  }for(uint32_t i=0;i<3;++i)require(acquired[i]==1&&retired[i]==1,"successful CONTROL acquisitions retire exactly once");
 }
 std::cout<<"PASS exact native client origin and balanced CONTROL acquisition32/64\n";return 0;
 }catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}}
