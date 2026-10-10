#include "RavetPerformance.h"
#include "RavetNativeClient.hpp"
#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <new>
#include <type_traits>

static size_t allocations=0, frees=0;
void *operator new(size_t bytes) { ++allocations; if (void *p=std::malloc(bytes?bytes:1)) return p; throw std::bad_alloc(); }
void *operator new[](size_t bytes) { return ::operator new(bytes); }
void operator delete(void *p) noexcept { ++frees; std::free(p); }
void operator delete[](void *p) noexcept { ::operator delete(p); }
bool check(bool value,const char *message) { if (!value) std::fprintf(stderr,"native performance: %s\n",message); return value; }
RavetNativeEventV1 on(uint32_t id,uint32_t key=69,float velocity=.12345679f) {
    RavetNativeEventV1 e{}; e.kind=RAVET_NATIVE_NOTE_ON_WITH_ID; e.channel=3;e.key=key;e.note_id=id;e.value=double(velocity);return e;
}
RavetNativeEventV1 expression(uint32_t detail,double value) {
    RavetNativeEventV1 e{};e.kind=RAVET_NATIVE_NOTE_EXPRESSION;e.detail=detail;e.value=value;return e;
}
template<class S> bool precision(uint32_t bits) {
    const auto *core=ravet_performance_processor_v1(bits); const auto *state=ravet_performance_audio_state_v1(bits);
    ravet::native::NativeProcessorClient client(core,state);
    if (!check(client.valid() && client.sampleBits()==bits && client.hasAudioState() && client.parameterCount()==1 && !client.hasTelemetry(),"client profile")) return false;
    RavetNativeParameterInfoV1 info{};
    if (!check(client.parameterInfo(0,info)==0 && info.id==1 && info.minimum==0.f && info.maximum==1.f && info.default_value==.2f,"level metadata")) return false;
    RavetNativeProcessorConfigV1 config{}; config.version=1; config.maximum_frames=4096;config.maximum_events=1024;config.maximum_output_events=16;config.bus_count=1;config.sample_rate=48000.;config.buses[0]={0,2};
    if (!check(client.prepare(config)==0,"prepare")) return false;
    RavetNativeProcessorTimingV1 timing{};
    if (!check(client.timing(timing)==0 && timing.latency_samples==0 && timing.tail_kind==2,"timing")) return false;
    const auto *parameters=ravet_performance_parameters_v1(bits);
    if (!check(parameters && parameters->state_bytes==24 && parameters->state_version==1 && std::memcmp(parameters->state_product,"RAVETPS",7)==0 && parameters->state_product[7]==0,"canonical state profile")) return false;
    void *stopped=core->create();
    if (!check(stopped!=nullptr,"stopped codec owner")) return false;
    std::array<uint8_t,26> bytes{};bytes.fill(91);
    std::array<RavetNativeParameterValueV1,1> stoppedValue{};
    bool codecOK=parameters->save_state(stopped,bytes.data(),26)==0 && bytes[24]==91 && bytes[25]==91;
    const float replacement=.75f;uint32_t replacementBits;std::memcpy(&replacementBits,&replacement,4);
    for(unsigned i=0;i<4;++i)bytes[20+i]=uint8_t(replacementBits>>(8*i));
    codecOK=codecOK && parameters->load_state(stopped,bytes.data(),24)==0 && parameters->snapshot(stopped,stoppedValue.data(),1)==0 && stoppedValue[0].id==1 && stoppedValue[0].value==replacement;
    auto damaged=bytes;damaged[0]^=1;
    codecOK=codecOK && parameters->load_state(stopped,damaged.data(),24)==1 && parameters->snapshot(stopped,stoppedValue.data(),1)==0 && stoppedValue[0].value==replacement;
    core->destroy(stopped);
    if (!check(codecOK,"canonical codec/tails/atomic malformed rejection"))return false;
    std::array<S,4098> left{},right{};std::array<S*,2> channels{left.data(),right.data()};
    RavetNativeProcessorBusV1 bus{0,2,nullptr,channels.data()},zeroBus{0,2,nullptr,nullptr};
    std::array<RavetNativeEventV1,16> output{};RavetNativeProcessorResultV1 result{};
    const auto render=[&](uint32_t frames,const RavetNativeEventV1 *events,uint32_t count) {
        return client.process(frames?&bus:&zeroBus,1,frames,events,count,nullptr,output.data(),16,result);
    };
    const auto zeroStart=on(UINT32_MAX,69,-0.f);
    if (!check(render(0,&zeroStart,1)==0 && result.event_count==0,"zero-velocity/full-ID On")) return false;
    const auto conflict=on(UINT32_MAX,70);
    if (!check(render(0,&conflict,1)==1 && result.status==2 && result.event_count==0,"ID ownership/address conflict")) return false;
    RavetNativeEventV1 choke{};choke.kind=RAVET_NATIVE_NOTE_CHOKE;
    if (!check(render(0,&choke,1)==0 && result.event_count==1 && output[0].kind==RAVET_NATIVE_NOTE_END && output[0].scope==7 && output[0].note_id==UINT32_MAX && output[0].channel==3 && output[0].key==69,"original completion")) return false;
    std::array<RavetNativeParameterValueV1,2> snapshot{{{99,91.f},{99,91.f}}};
    const auto oldAlloc=allocations,oldFree=frees;
    for (uint32_t cycle=0;cycle<1000;++cycle) {
        if (!check(client.reset()==0,"reset")) return false;
        const RavetNativeParameterValueV1 level{1,.5f};
        if (!check(client.applyState(&level,1)==0,"audio state recall")) return false;
        std::array<RavetNativeEventV1,5> events{on(cycle?0x80000000u+cycle:UINT32_MAX),expression(0,2.),expression(1,0.),expression(2,12.),expression(4,.7)};
        left.fill(S(99));right.fill(S(88));
        if (!check(render(300,events.data(),uint32_t(events.size()))==0 && result.status==0 && result.event_count==0,"timed render")) return false;
        const double tolerance=std::is_same_v<S,float>?1e-7:1e-13;
        for (size_t i=0;i<240;++i) {
            const double expected=std::sin(6.2831853071795864769*880./48000.*double(i))*(double(i)+1.)/240.*double(.12345679f)*2.*.7/16.;
            if (!check(std::abs(double(left[i])-expected)<tolerance && right[i]==S(0),"analytic native precision/velocity/pan")) return false;
        }
        if (!check(left[300]==S(99) && right[300]==S(88) && client.snapshotState(snapshot.data(),2)==0 && snapshot[0].id==1 && snapshot[0].value==.5f && snapshot[1].id==99,"state/backing tails")) return false;
        const auto mute=expression(0,-0.);
        if (!check(render(128,&mute,1)==0,"zero volume")) return false;
        for (size_t i=0;i<128;++i) if (!check(left[i]==S(0) && right[i]==S(0),"exact mute")) return false;
        auto off=events[0];off.kind=RAVET_NATIVE_NOTE_OFF_WITH_ID;off.value=double(.7f);
        if (!check(render(2400,&off,1)==0 && result.event_count==1 && output[0].kind==RAVET_NATIVE_NOTE_END && output[0].note_id==events[0].note_id && output[0].offset==2399,"release completion")) return false;
    }
    if (!check(allocations==oldAlloc && frees==oldFree,"C++ heap-free prepared workload")) return false;
    RavetNativeParameterValueV1 invalid{1,std::numeric_limits<float>::quiet_NaN()};
    if (!check(client.applyState(&invalid,1)==1 && client.snapshotState(snapshot.data(),2)==0 && snapshot[0].value==.5f,"atomic bad state")) return false;
    if (!check(client.deactivate()==0 && client.snapshotState(snapshot.data(),2)==1 && client.prepare(config)==0 && client.snapshotState(snapshot.data(),2)==0 && snapshot[0].value==.5f,"lifecycle state retention")) return false;
    return true;
}
int main() {
    if (!check(!ravet_performance_processor_v1(16) && !ravet_performance_parameters_v1(16) && !ravet_performance_audio_state_v1(16),"unsupported precision") || !precision<float>(32) || !precision<double>(64)) return 1;
    std::puts("native identified-performance actual-library f32/f64 2000 state/audio/mute/release/completion cycles, analytic precision, full identities, rejection, tails, lifecycle and zero C++ heap contracts passed");
    return 0;
}
