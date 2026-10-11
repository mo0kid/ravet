#include "HostedAudioScratch.hpp"
#include <cassert>
#include <iostream>
#include <limits>
using namespace ravet::vst3::hosted_audio;
static RavetVst3HostedAudioProfile profile(){
    RavetVst3HostedAudioProfile p{};p.struct_size=sizeof(p);p.maximum_frames=8;p.sample_bits=32;p.sample_rate=48000;
    p.input_buses=3;p.output_buses=1;p.inputs[0]={3,1,0};p.inputs[1]={UINT64_MAX,0,0};p.inputs[2]={1,1,0};p.outputs[0]={3,1,0};return p;
}
static void structuralWholeAdmission(){auto p=profile();assert(validProfile(p));
    for(uint32_t bits:{0u,16u,31u,33u,63u,65u}){auto bad=p;bad.sample_bits=bits;assert(!validProfile(bad));}
    for(double rate:{0.,-1.,768000.1}){auto bad=p;bad.sample_rate=rate;assert(!validProfile(bad));}
    for(uint64_t bits:{UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8000000000001),UINT64_C(0xfff0000000000001)}){auto bad=p;std::memcpy(&bad.sample_rate,&bits,sizeof(bits));assert(!validProfile(bad));}
    auto max=p;max.sample_rate=768000;max.maximum_frames=65536;max.sample_bits=64;assert(validProfile(max));
    for(uint32_t count:{17u,UINT32_MAX}){auto bad=p;bad.input_buses=count;assert(!validProfile(bad));}
    for(uint32_t frames:{0u,65537u,UINT32_MAX}){auto bad=p;bad.maximum_frames=frames;assert(!validProfile(bad));}
    auto bad=p;bad.reserved=1;assert(!validProfile(bad));bad=p;bad.inputs[15].arrangement=1;assert(!validProfile(bad));bad=p;bad.inputs[15].active=1;assert(!validProfile(bad));bad=p;bad.inputs[1].active=2;assert(!validProfile(bad));bad=p;bad.outputs[0].reserved=1;assert(!validProfile(bad));
}
static void inactiveIndexAndOutputLast(){auto p=profile();std::array<uint32_t,16> in{},out{};in[0]=2;in[1]=64;in[2]=1;out[0]=2;PackedLayout map{};assert(packedLayout(p,in,out,map));assert(map.inputChannels==3&&map.outputChannels==2);assert(map.inputs[0].offset==0&&map.inputs[1].offset==2&&map.inputs[2].offset==2);assert(map.inputs[1].channels==64&&!map.inputs[1].active);const auto saved=map;
    auto bad=p;bad.inputs[1].active=1;assert(!packedLayout(bad,in,out,map));assert(map==saved);
    in[2]=2;assert(!packedLayout(p,in,out,map));assert(map==saved);in[2]=1;in[15]=1;assert(!packedLayout(p,in,out,map));assert(map==saved);
    bad=p;for(auto& b:bad.inputs)b.active=0;for(auto& b:bad.outputs)b.active=0;in[15]=0;assert(!packedLayout(bad,in,out,map));assert(map==saved);
}
static void exact64ChannelsAndEmptyDirections(){RavetVst3HostedAudioProfile p{};p.struct_size=sizeof(p);p.maximum_frames=65536;p.sample_bits=64;p.sample_rate=1;p.output_buses=1;p.outputs[0]={UINT64_MAX,1,0};std::array<uint32_t,16> in{},out{};out[0]=64;PackedLayout map{};assert(packedLayout(p,in,out,map));assert(map.inputChannels==0&&map.outputChannels==64);
    p.output_buses=2;p.outputs[1]={1,1,0};out[1]=1;const auto saved=map;assert(!packedLayout(p,in,out,map));assert(map==saved);
    p.outputs[1]={0,0,0};out[1]=0;assert(packedLayout(p,in,out,map));assert(map.outputs[1].channels==0&&!map.outputs[1].active);
    p.outputs[1].active=1;assert(!packedLayout(p,in,out,map));
}
static void typedScratchExactExtentsAndMove(){ScratchBudget budget;Scratch a;a.prepare(budget,32,8,3,2);assert(a.bits()==32&&a.frames()==8&&a.bytes()==160&&budget.bytes()==160);assert(a.input<float>(0)&&a.input<float>(2)&&!a.input<float>(3)&&!a.input<double>(0));assert(a.input<float>(1)-a.input<float>(0)==8);assert(a.output<float>(1)-a.output<float>(0)==8);
    size_t ranges=0,bytes=0;assert(a.appendRanges([&](const void*,size_t n,size_t){++ranges;bytes+=n;return true;}));assert(ranges==2&&bytes==160);
    a.input<float>(0)[0]=17.25f;assert(!a.external(a.input<float>(0),4,alignof(float)));assert(!a.external(a.input<float>(2)+7,4,alignof(float)));int external=19;assert(a.external(&external,sizeof(external),alignof(int)));
    Scratch b=std::move(a);assert(a.bytes()==0&&a.bits()==0&&budget.bytes()==160&&b.input<float>(0)[0]==17.25f);b.prepare(budget,64,8,0,2);assert(budget.bytes()==128&&b.bytes()==128&&b.bits()==64&&!b.input<double>(0)&&b.output<double>(1));assert(!b.output<float>(0));assert(b.output<double>(1)[7]==0.);b.clear();assert(budget.bytes()==0);
}
static void wholePeakBudgetAndLegacyAdmission(){ScratchBudget budget;Scratch a;a.prepare(budget,64,65536,64,64);assert(budget.bytes()==ScratchBudget::MaximumBytes);auto* old=a.input<double>(0);old[0]=0.10000000000000001;
    try{a.prepare(budget,32,1,1,1);assert(false);}catch(const std::length_error&){}assert(a.input<double>(0)==old&&old[0]==0.10000000000000001&&budget.bytes()==ScratchBudget::MaximumBytes);
    Scratch b;try{b.prepare(budget,64,1,1,0);assert(false);}catch(const std::length_error&){}assert(b.bytes()==0&&budget.bytes()==ScratchBudget::MaximumBytes);
    // Existing legacy shapes keep admission; new opt-in shape counts them all.
    b.prepare(budget,32,1,1,0,false);assert(budget.bytes()==ScratchBudget::MaximumBytes+4);Scratch c;try{c.prepare(budget,32,1,1,0);assert(false);}catch(const std::length_error&){}assert(!c.bytes());b.clear();a.clear();assert(!budget.bytes());
    c.prepare(budget,32,1,1,0);assert(c.bytes()==4);try{c.prepare(budget,64,0,1,0);assert(false);}catch(const std::invalid_argument&){}assert(c.bits()==32&&c.bytes()==4&&budget.bytes()==4);
}
int main(){structuralWholeAdmission();inactiveIndexAndOutputLast();exact64ChannelsAndEmptyDirections();typedScratchExactExtentsAndMove();wholePeakBudgetAndLegacyAdmission();std::cout<<"5 hosted profile/scratch groups passed\n";}
