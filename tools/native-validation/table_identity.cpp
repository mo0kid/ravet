#include "RavetNativeAudioState.h"
#include "RavetNativeState.h"
#include "RavetNativeTelemetry.h"
#include "RavetPerformance.h"
#include "RavetBusRouting.h"
#include "RavetNoteRouting.h"
#include "RavetDelayRouting.h"
#include <array>
#include <iostream>
#include <stdexcept>
#include <cstring>
extern "C" {
const RavetNativeProcessorEntryV1 *ravet_native_processor_v1(uint32_t);
const RavetNativeParameterEntryV1 *ravet_native_parameters_v1(uint32_t);
const RavetNativeCachedParameterEntryV1 *ravet_native_cached_parameters_v1(uint32_t);
const RavetNativeAudioStateEntryV1 *ravet_native_audio_state_v1(uint32_t);
const RavetNativeStateEntryV1 *ravet_native_state_v1(uint32_t);
const RavetNativeTelemetryEntryV1 *ravet_native_telemetry_v1(uint32_t);
}
void require(bool okay,const char *message){if(!okay)throw std::runtime_error(message);}
struct Family{
 const char *name;const RavetNativeProcessorEntryV1 *(*core)(uint32_t);const RavetNativeParameterEntryV1 *(*parameters)(uint32_t);
 const RavetNativeCachedParameterEntryV1 *(*cache)(uint32_t);const RavetNativeAudioStateEntryV1 *(*audio)(uint32_t);const RavetNativeStateEntryV1 *(*state)(uint32_t);const RavetNativeTelemetryEntryV1 *(*telemetry)(uint32_t);
 uint32_t inputs,buses,outputEvents;
};
#define FAMILY(prefix,in,bus,events) {#prefix,prefix##_processor_v1,prefix##_parameters_v1,prefix##_cached_parameters_v1,prefix##_audio_state_v1,prefix##_state_v1,prefix##_telemetry_v1,in,bus,events}
template<class S>void exercise(const Family &family,uint32_t bits){
 const auto *core=family.core(bits);const auto *parameters=family.parameters(bits);const auto *cache=family.cache(bits);const auto *audio=family.audio(bits);const auto *state=family.state(bits);const auto *telemetry=family.telemetry?family.telemetry(bits):nullptr;
 require(core&&parameters&&cache&&audio&&state&&core->sample_bits==bits&&parameters->sample_bits==bits&&cache->sample_bits==bits&&audio->sample_bits==bits&&state->sample_bits==bits,"complete exact precision family");
 for(uint32_t i=0;i<20;++i){
  require(core==family.core(bits)&&parameters==family.parameters(bits)&&cache==family.cache(bits)&&audio==family.audio(bits)&&state==family.state(bits),"runtime exported acquisition canonical addresses");
  require(parameters->processor_entry()==core&&cache->parameters_entry()==parameters&&audio->cached_parameters_entry()==cache&&state->parameters_entry()==parameters,"export and transitive companion exact cross-codegen addresses");
  if(family.telemetry)require(telemetry&&family.telemetry(bits)==telemetry&&telemetry->sample_bits==bits&&telemetry->processor_entry()==core,"telemetry exact exported/transitive core address");
 }
 auto *owner=core->create();auto *metadata=cache->create();auto *publication=state->create(nullptr,0);auto *observations=telemetry?telemetry->create_cache():nullptr;
 require(owner&&metadata&&publication&&(!telemetry||observations),"actual native family handle/cache/publication ownership");
 RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=32;config.maximum_events=32;config.maximum_output_events=family.outputEvents;config.bus_count=family.buses;config.sample_rate=48000.;config.buses[0]={family.inputs,1};
 if(family.buses==3){config.buses[1]={1,0};config.buses[2]={0,1};}
 require(core->prepare(owner,&config)==0,"actual native default32/64 consumer preparation");
 std::array<S,34> input{},detector{},main{},aux{};input.fill(S(1));detector.fill(S(.5));main.fill(S(77));aux.fill(S(77));const S *in[]={input.data()},*detect[]={detector.data()};S *out[]={main.data()},*monitor[]={aux.data()};
 std::array<RavetNativeProcessorBusV1,3> buses{};buses[0]={family.inputs,1,family.inputs?in:nullptr,out};buses[1]={1,0,detect,nullptr};buses[2]={0,1,nullptr,monitor};RavetNativeProcessorResultV1 result{};std::array<RavetNativeEventV1,2> events{};
 require(!core->process(owner,buses.data(),family.buses,32,nullptr,0,nullptr,events.data(),family.outputEvents,&result)&&result.status==0&&main[32]==S(77)&&main[33]==S(77),"actual selected precision processing/default preserved backing tails");
 if(family.buses==3)require(main[0]==S(1)&&aux[0]==S(.5)&&aux[32]==S(77),"actual generic multibus default routing");
 std::array<RavetNativeParameterValueV1,2> values{};require(audio->parameter_count<=values.size()&&!audio->snapshot(owner,metadata,values.data(),audio->parameter_count),"exact native audio snapshot through companion cache");
 require(!audio->apply(owner,metadata,values.data(),audio->parameter_count),"exact native audio state apply");
 if(telemetry){std::array<RavetNativeTelemetryValueV1,1024> seen{};require(telemetry->telemetry_count<=seen.size()&&!telemetry->snapshot(owner,observations,seen.data(),telemetry->telemetry_count),"exact native telemetry companion handles");}
 core->destroy(owner);cache->destroy(metadata);state->destroy(publication);if(telemetry)telemetry->destroy_cache(observations);
}
int main(){try{
 const std::array<Family,6> families{{FAMILY(ravet_native,1,1,0),
  {"Performance",ravet_performance_processor_v1,ravet_performance_parameters_v1,ravet_performance_cached_parameters_v1,ravet_performance_audio_state_v1,ravet_performance_state_v1,nullptr,0,1,0},
  FAMILY(ravet_bus_routing,1,3,0),FAMILY(ravet_note_routing,0,1,2),{"Delay8",ravet_delay8_processor_v1,ravet_delay8_parameters_v1,ravet_delay8_cached_parameters_v1,ravet_delay8_audio_state_v1,ravet_delay8_state_v1,nullptr,1,1,0},
  {"Delay16",ravet_delay16_processor_v1,ravet_delay16_parameters_v1,ravet_delay16_cached_parameters_v1,ravet_delay16_audio_state_v1,ravet_delay16_state_v1,nullptr,1,1,0}}};
 for(const auto &family:families){exercise<float>(family,32);exercise<double>(family,64);std::cout<<"PASS "<<family.name<<" exact exported/transitive tables and actual32/64 native processing\n";}
 return 0;
 }catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}}
