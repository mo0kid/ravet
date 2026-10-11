#include "RavetNativeClient.hpp"
#include <array>
#include <cstdio>
#include <cstdlib>
#include <new>
namespace { bool measuring=false;size_t news=0,deletes=0; }
void *operator new(size_t bytes){if(measuring)++news;if(auto *p=std::malloc(bytes?bytes:1))return p;throw std::bad_alloc();}
void operator delete(void *p)noexcept{if(p&&measuring)++deletes;std::free(p);}
void operator delete(void *p,size_t)noexcept{::operator delete(p);}
using ravet::native::NativeProcessorClient;
static bool check(bool okay,const char *message){if(!okay)std::fprintf(stderr,"native client: %s\n",message);return okay;}
bool precision(uint32_t bits){
    NativeProcessorClient client(ravet_native_processor_v1(bits),ravet_native_audio_state_v1(bits),ravet_native_telemetry_v1(bits));
    if(!check(client.valid()&&client.sampleBits()==bits&&client.hasAudioState()&&client.hasTelemetry()&&client.parameterCount()==2&&client.telemetryCount()==12,"complete profile"))return false;
    RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=8;config.maximum_events=2;config.bus_count=1;config.sample_rate=48000.;config.buses[0]={1,1};
    if(!check(client.prepare(config)==0,"prepare"))return false;
    std::array<float,10> in32{},out32{};std::array<double,10> in64{},out64{};in32.fill(.25f);in64.fill(1e200);out32.fill(91.f);out64.fill(91.);
    const std::array<const float*,1> inputs32{in32.data()};const std::array<float*,1> outputs32{out32.data()};const std::array<const double*,1> inputs64{in64.data()};const std::array<double*,1> outputs64{out64.data()};
    RavetNativeProcessorBusV1 bus{1,1,bits==32?static_cast<const void*>(inputs32.data()):static_cast<const void*>(inputs64.data()),bits==32?static_cast<const void*>(outputs32.data()):static_cast<const void*>(outputs64.data())};
    RavetNativeProcessorResultV1 report{};std::array<RavetNativeParameterValueV1,3> state{{{77,91.f},{77,91.f},{77,91.f}}};std::array<RavetNativeTelemetryValueV1,13> meters{};meters[12]={77,99,91.,91};
    RavetNativeParameterInfoV1 parameter{};RavetNativeTelemetryInfoV1 telemetry{};RavetNativeProcessorTimingV1 timing{};
    measuring=true;bool okay=true;
    for(uint32_t cycle=0;cycle<1000&&okay;++cycle){const float gain=float(cycle%5);const std::array<RavetNativeParameterValueV1,2> recall{{{2,0.f},{1,gain}}};
        okay=client.applyState(recall.data(),2)==0&&client.process(&bus,1,8,nullptr,0,nullptr,nullptr,0,report)==0&&client.snapshotState(state.data(),3)==0&&client.snapshotTelemetry(meters.data(),13)==0&&client.parameterInfo(0,parameter)==0&&client.telemetryInfo(0,telemetry)==0&&client.timing(timing)==0&&client.reset()==0;
        okay=okay&&state[0].id==1&&state[0].value==gain&&state[2].id==77&&state[2].value==91.f&&meters[0].id==10&&meters[0].samples==8&&meters[6].value==(bits==32?.25:1e200)*gain&&meters[12].id==77&&meters[12].samples==91&&parameter.id==1&&telemetry.id==10&&timing.latency_samples==0;
        okay=okay&&(bits==32?out32[0]==.25f*gain&&out32[8]==91.f:out64[0]==1e200*gain&&out64[8]==91.);
    }
    measuring=false;if(!check(okay&&news==0&&deletes==0,"actual complete forwarding/wide audio/tails/zero C++ new-delete"))return false;
    if(!check(client.deactivate()==0&&client.snapshotState(state.data(),3)==1&&client.prepare(config)==0,"lifecycle"))return false;
    NativeProcessorClient coreOnly(ravet_native_processor_v1(bits));
    if(!check(coreOnly.valid()&&!coreOnly.hasAudioState()&&!coreOnly.hasTelemetry()&&coreOnly.applyState(state.data(),2)==1&&coreOnly.snapshotTelemetry(meters.data(),13)==1&&coreOnly.prepare(config)==0,"explicit optional capabilities"))return false;
    return true;
}
const RavetNativeCachedParameterEntryV1 *cacheProfile=nullptr;uint32_t created=0,retired=0;
const RavetNativeCachedParameterEntryV1 *profile(){return cacheProfile;}
void *rejectCache(){++created;return nullptr;}
void dropCache(void *cache){++retired;ravet_native_cached_parameters_v1(32)->destroy(cache);}
void *rejectTelemetry(){return nullptr;}
bool failures(){
    const auto *core=ravet_native_processor_v1(32);const auto *state=ravet_native_audio_state_v1(32);const auto *telemetry=ravet_native_telemetry_v1(32);
    NativeProcessorClient absent(nullptr);RavetNativeProcessorTimingV1 timing{77,99,91};RavetNativeParameterValueV1 output{77,91.f};
    if(!check(!absent.valid()&&absent.sampleBits()==0&&absent.timing(timing)==1&&timing.latency_samples==77&&absent.snapshotState(&output,1)==1&&output.id==77,"invalid instance output preservation"))return false;
    for(uint32_t field=0;field<5;++field){auto bad=*core;if(field==0)bad.version=2;if(field==1)bad.byte_size=16;if(field==2)bad.sample_bits=16;if(field==3)bad.reserved=1;if(field==4)bad.process=nullptr;NativeProcessorClient rejected(&bad);if(!check(!rejected.valid(),"core prefix/callback preflight"))return false;}
    NativeProcessorClient wrongPrecision(core,ravet_native_audio_state_v1(64));NativeProcessorClient wrongTelemetry(core,nullptr,ravet_native_telemetry_v1(64));
    if(!check(!wrongPrecision.valid()&&!wrongTelemetry.valid(),"precision pairing"))return false;
    auto copiedCore=*core;NativeProcessorClient wrongOrigin(&copiedCore,state,telemetry);if(!check(!wrongOrigin.valid(),"byte-identical copied core is not exact companion origin"))return false;
    auto badCore=*core; badCore.reset=[](void*)->int32_t{return 0;}; NativeProcessorClient wrongCallbacks(&badCore,state,telemetry); if(!check(!wrongCallbacks.valid(),"different originating callback profile"))return false;
    for(uint32_t field=0;field<4;++field){auto bad=*state;if(field==0)bad.version=2;if(field==1)bad.byte_size=16;if(field==2)bad.parameter_count=1025;if(field==3)bad.snapshot=nullptr;NativeProcessorClient rejected(core,&bad);if(!check(!rejected.valid(),"state profile"))return false;}
    for(uint32_t field=0;field<4;++field){auto bad=*telemetry;if(field==0)bad.version=2;if(field==1)bad.byte_size=16;if(field==2)bad.telemetry_count=1025;if(field==3)bad.snapshot=nullptr;NativeProcessorClient rejected(core,nullptr,&bad);if(!check(!rejected.valid(),"telemetry profile"))return false;}
    auto cached=*ravet_native_cached_parameters_v1(32);cached.create=rejectCache;cacheProfile=&cached;auto patched=*state;patched.cached_parameters_entry=profile;
    {NativeProcessorClient rejected(core,&patched);if(!check(!rejected.valid()&&created==1&&retired==0,"cache factory failure"))return false;}
    cached=*ravet_native_cached_parameters_v1(32);cached.destroy=dropCache;auto failedTelemetry=*telemetry;failedTelemetry.create_cache=rejectTelemetry;
    {NativeProcessorClient rejected(core,&patched,&failedTelemetry);if(!check(!rejected.valid()&&retired==0,"partial cache retained until stopped owner retirement"))return false;}
    return check(retired==1,"partial construction cleanup");
}
int main(){if(!failures()||!precision(32)||!precision(64))return 1;std::puts("native C++ client exact optional pairing, invalid profiles/factory cleanup, 2000 complete callback cycles/wide audio/tails/lifecycle and zero C++ new-delete passed");return 0;}
