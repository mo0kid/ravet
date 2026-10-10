#include "RavetNativeTelemetry.h"
#include <array>
#include <cmath>
#include <cstdio>
#include <limits>
static bool check(bool value,const char *message){if(!value)std::fprintf(stderr,"native telemetry: %s\n",message);return value;}
bool precision(uint32_t bits){
    const auto *table=ravet_native_telemetry_v1(bits);
    if(!check(table && table->version==1 && table->byte_size==sizeof(*table) && table->sample_bits==bits && table->telemetry_count==12,"table"))return false;
    const auto *core=table->processor_entry(); if(!check(core->sample_bits==bits && core->create==ravet_native_processor_v1(bits)->create && core->destroy==ravet_native_processor_v1(bits)->destroy,"pairing"))return false;
    void *cache=table->create_cache();void *handle=core->create();if(!check(cache&&handle,"create"))return false;
    const auto cleanup=[&]{table->destroy_cache(cache);core->destroy(handle);};
    const RavetNativeTelemetryValueV1 sentinel{77,99,91.,91};std::array<RavetNativeTelemetryValueV1,14> values{};values.fill(sentinel);
    RavetNativeTelemetryInfoV1 info{};
    for(uint32_t index=0;index<12;++index){if(!check(table->info(cache,index,&info)==0 && info.id==10+index && info.display_minimum==0. && info.display_maximum==1. && info.name_length>0 && info.name[info.name_length]==0 && info.reserved==0,"metadata")){cleanup();return false;}}
    info.id=77;if(!check(table->info(cache,12,&info)==1 && info.id==77 && table->snapshot(handle,cache,values.data(),14)==1 && values[0].id==77,"failed cached index/stopped")){cleanup();return false;}
    RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=8;config.maximum_events=0;config.bus_count=1;config.sample_rate=48000.;config.buses[0]={2,2};
    if(!check(core->prepare(handle,&config)==0,"prepare")){cleanup();return false;}
    std::array<float,10> input32{},output32{},right32{};std::array<double,10> input64{},output64{},right64{};
    input32.fill(.5f);input32[2]=std::numeric_limits<float>::quiet_NaN();input64.fill(1e200);input64[2]=std::numeric_limits<double>::infinity();right32.fill(.25f);right64.fill(1e-200);
    std::array<const float*,2> in32{input32.data(),right32.data()};std::array<float*,2> out32{output32.data(),right32.data()};
    std::array<const double*,2> in64{input64.data(),right64.data()};std::array<double*,2> out64{output64.data(),right64.data()};
    RavetNativeProcessorBusV1 bus{2,2,bits==32?static_cast<const void*>(in32.data()):static_cast<const void*>(in64.data()),bits==32?static_cast<const void*>(out32.data()):static_cast<const void*>(out64.data())};RavetNativeProcessorResultV1 result{};
    const double left=bits==32?.5:1e200,right=bits==32?.25:1e-200;
    for(uint32_t cycle=0;cycle<1000;++cycle){
        if(!check(core->process(handle,&bus,1,8,nullptr,0,nullptr,nullptr,0,&result)==0 && table->snapshot(handle,cache,values.data(),14)==0,"process/snapshot")){cleanup();return false;}
        for(uint32_t reading=0;reading<4;++reading){const bool invalid=reading%2==0;const double peak=invalid?left:right;
            const auto &p=values[3*reading],&r=values[3*reading+1],&flag=values[3*reading+2];
            if(!check(p.id==10+3*reading && p.value==peak && p.samples==8 && p.flags==uint32_t(invalid) && r.samples==8 && r.flags==p.flags && std::abs(r.value/peak-(invalid?std::sqrt(7./8.):1.))<(bits==32?1e-6:1e-14) && flag.value==double(invalid) && flag.samples==8 && flag.flags==p.flags,"raw wide/tiny/RMS/flags/exact window")){cleanup();return false;}
        }
        if(!check(values[12].id==77 && values[12].value==91. && values[13].samples==91,"tails")){cleanup();return false;}
    }
    const auto previous=values;
    if(!check(core->process(handle,&bus,1,0,nullptr,0,nullptr,nullptr,0,&result)==0 && table->snapshot(handle,cache,values.data(),14)==0,"zero-frame")){cleanup();return false;}
    for(uint32_t i=0;i<12;++i)if(!check(values[i].value==previous[i].value && values[i].samples==previous[i].samples && values[i].flags==previous[i].flags,"zero-frame retention")){cleanup();return false;}
    if(!check(core->process(handle,&bus,1,9,nullptr,0,nullptr,nullptr,0,&result)==1 && table->snapshot(handle,cache,values.data(),14)==0 && values[0].value==left && values[0].samples==8,"rejected-block retention")){cleanup();return false;}
    values.fill(sentinel);if(!check(table->snapshot(handle,cache,values.data(),11)==1 && values[0].id==77 && values[13].id==77,"failed prefix preservation")){cleanup();return false;}
    if(!check(core->reset(handle)==0 && table->snapshot(handle,cache,values.data(),14)==0,"reset")){cleanup();return false;}
    for(uint32_t i=0;i<12;++i)if(!check(values[i].value==0. && values[i].samples==0 && values[i].flags==0,"cleared consumer meters")){cleanup();return false;}
    config.buses[0]={1,1};bus.input_channels=1;bus.output_channels=1;
    if(!check(core->prepare(handle,&config)==0 && core->process(handle,&bus,1,8,nullptr,0,nullptr,nullptr,0,&result)==0 && table->snapshot(handle,cache,values.data(),14)==0,"mono reprepare")){cleanup();return false;}
    for(const auto i:std::array<uint32_t,6>{3,4,5,9,10,11})if(!check(values[i].value==0. && values[i].samples==0 && values[i].flags==0,"unused channels")){cleanup();return false;}
    if(!check(core->deactivate(handle)==0 && table->snapshot(handle,cache,values.data(),14)==1,"deactivate")){cleanup();return false;}
    cleanup();return true;
}
int main(){if(!check(ravet_native_telemetry_v1(16)==nullptr,"precision")||!precision(32)||!precision(64))return 1;std::puts("native telemetry actual-library f32/f64 2000 stereo render/snapshot cycles, raw extreme/tiny levels, exact counts/nonfinite flags, mono/reset/zero/rejected retention and failed/tail outputs passed");return 0;}
