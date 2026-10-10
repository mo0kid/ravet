#include "RavetNativeAudioState.h"
#include <array>
#include <cstdio>
#include <limits>
static bool check(bool value, const char *message) { if (!value) std::fprintf(stderr,"native audio state: %s\n",message); return value; }
bool precision(uint32_t bits) {
    const auto *table = ravet_native_audio_state_v1(bits);
    if (!check(table && table->version == 1 && table->byte_size == sizeof(*table) && table->sample_bits == bits && table->parameter_count == 2,"table")) return false;
    const auto *cached = table->cached_parameters_entry(); const auto *parameters = cached->parameters_entry(); const auto *core = parameters->processor_entry();
    if (!check(cached->sample_bits == bits && core->create == ravet_native_processor_v1(bits)->create && core->destroy == ravet_native_processor_v1(bits)->destroy && cached->create == ravet_native_cached_parameters_v1(bits)->create && cached->destroy == ravet_native_cached_parameters_v1(bits)->destroy,"pairing")) return false;
    void *cache = cached->create(); void *handle = core->create();
    if (!check(cache && handle,"create")) return false;
    const auto cleanup = [&] { cached->destroy(cache); core->destroy(handle); };
    const RavetNativeParameterValueV1 sentinel{77,91.f}; std::array<RavetNativeParameterValueV1,3> values{{sentinel,sentinel,sentinel}};
    std::array<RavetNativeParameterValueV1,2> state{{{2,0.f},{1,1.f}}};
    if (!check(table->apply(handle,cache,state.data(),2) == 1 && table->snapshot(handle,cache,values.data(),3) == 1 && values[0].id == 77,"stopped guard")) {cleanup(); return false;}
    RavetNativeProcessorConfigV1 config{}; config.version=1; config.maximum_frames=8; config.maximum_events=2; config.bus_count=1; config.sample_rate=48000.; config.buses[0]={1,1};
    if (!check(core->prepare(handle,&config)==0,"prepare")) {cleanup(); return false;}
    std::array<float,10> input32{},output32{}; std::array<double,10> input64{},output64{};
    input32.fill(.25f); output32.fill(91.f); input64.fill(1e200); output64.fill(91.);
    std::array<const float*,1> in32{input32.data()}; std::array<float*,1> out32{output32.data()};
    std::array<const double*,1> in64{input64.data()}; std::array<double*,1> out64{output64.data()};
    RavetNativeProcessorBusV1 bus{1,1,bits==32?static_cast<const void*>(in32.data()):static_cast<const void*>(in64.data()),bits==32?static_cast<const void*>(out32.data()):static_cast<const void*>(out64.data())};
    RavetNativeProcessorResultV1 result{};
    for (uint32_t cycle=0;cycle<1000;++cycle) {
        const float gain=float(cycle%5), bypass=float(cycle%2); state={{{2,bypass},{1,gain}}};
        if (!check(table->apply(handle,cache,state.data(),2)==0 && core->process(handle,&bus,1,8,nullptr,0,nullptr,nullptr,0,&result)==0 && table->snapshot(handle,cache,values.data(),3)==0,"apply/process/snapshot")) {cleanup(); return false;}
        const double multiplier=bypass?1.:gain;
        if (!check((bits==32?output32[0]==.25f*float(multiplier):output64[0]==1e200*multiplier) && (bits==32?output32[8]==91.f:output64[8]==91.) && values[0].id==1 && values[0].value==gain && values[1].id==2 && values[1].value==bypass && values[2].id==77 && values[2].value==91.f,"wide audio/state/tails")) {cleanup(); return false;}
    }
    state={{{2,0.f},{1,.5f}}};
    if (!check(table->apply(handle,cache,state.data(),2)==0,"ramp start")) {cleanup(); return false;}
    RavetNativeEventV1 event{}; event.kind=RAVET_NATIVE_PARAMETER_VALUE; event.parameter_id=1; event.value=2.;
    if (!check(core->process(handle,&bus,1,0,&event,1,nullptr,nullptr,0,&result)==0 && table->snapshot(handle,cache,values.data(),3)==0 && values[0].value==2.f && core->process(handle,&bus,1,8,nullptr,0,nullptr,nullptr,0,&result)==0 && (bits==32?output32[0]>.125f && output32[0]<.5f:output64[0]>.5e200 && output64[0]<2e200),"zero-frame automation snapshot retains smoothing")) {cleanup(); return false;}
    for (const auto bad:std::array<RavetNativeParameterValueV1,5>{{{2,.5f},{99,0.f},{1,std::numeric_limits<float>::infinity()},{1,std::numeric_limits<float>::quiet_NaN()},{1,5.f}}}) {
        state={{{2,0.f},bad}};
        if (!check(table->apply(handle,cache,state.data(),2)==1 && table->snapshot(handle,cache,values.data(),3)==0 && values[0].value==2.f,"atomic malformed restore")) {cleanup(); return false;}
    }
    values.fill(sentinel);
    if (!check(table->snapshot(handle,cache,values.data(),1)==1 && values[0].id==77 && table->apply(handle,cache,state.data(),1)==1 && core->reset(handle)==0 && table->snapshot(handle,cache,values.data(),3)==0 && values[0].value==2.f && core->deactivate(handle)==0 && table->snapshot(handle,cache,values.data(),3)==1 && core->prepare(handle,&config)==0 && table->snapshot(handle,cache,values.data(),3)==0,"capacity/reset/deactivate/reprepare")) {cleanup(); return false;}
    cleanup(); return true;
}
int main() { if (!check(ravet_native_audio_state_v1(16)==nullptr,"precision") || !precision(32) || !precision(64)) return 1; std::puts("native direct audio-state actual-library f32/f64 2000 restore/render/snapshot cycles, whole-state rejection, zero-frame automation, retained smoothing and lifecycle passed"); return 0; }
