#include "RavetAaxPrecision.hpp"
#include <cfenv>
#include <iostream>
#include <memory>
#include <stdexcept>
namespace {
void require(bool value,const char* message){if(!value)throw std::runtime_error(message);}
uint64_t doubleBits(double value){uint64_t bits=0;std::memcpy(&bits,&value,8);return bits;}
uint32_t floatBits(float value){uint32_t bits=0;std::memcpy(&bits,&value,4);return bits;}
void scalar(){
 using C=ravet::aax::AaxIeeePrecision;
 struct Pair{uint64_t source;uint32_t expected;};
 const Pair golden[]{
 {0,0},{UINT64_C(0x8000000000000000),0x80000000u},{UINT64_C(0x3ff0000000000000),0x3f800000u},
 {UINT64_C(0x3ff0000010000000),0x3f800000u},{UINT64_C(0x3ff0000010000001),0x3f800001u},
 {UINT64_C(0x3ff0000030000000),0x3f800002u},{UINT64_C(0x3810000000000000),0x00800000u},
 {UINT64_C(0x36a0000000000000),1u},{UINT64_C(0x3690000000000000),0u},{UINT64_C(0x3690000000000001),1u},
 {UINT64_C(0xb690000000000000),0x80000000u},{UINT64_C(0x47efffffe0000000),0x7f7fffffu},
 {UINT64_C(0x47efffffefffffff),0x7f7fffffu},{UINT64_C(0x7ff0000000000000),0x7f800000u},
 {UINT64_C(0xfff0000000000000),0xff800000u},{UINT64_C(0x7ff0000000000001),0x7fc00000u},
 {UINT64_C(0xfff8000020000000),0xffc00001u}};
 for(int mode:{FE_TONEAREST,FE_DOWNWARD,FE_UPWARD,FE_TOWARDZERO}){
 require(std::fesetround(mode)==0,"fixture rounding environment available");
 for(auto pair:golden){uint32_t output=99;require(C::narrow(pair.source,output)&&output==pair.expected,"independent integer golden rounding under every FE mode");}
 for(uint64_t overflow:{UINT64_C(0x47effffff0000000),UINT64_C(0x47f0000000000000),UINT64_C(0xc7effffff0000000),UINT64_C(0x7fefffffffffffff)}){uint32_t output=99;require(!C::narrow(overflow,output)&&output==99,"rounded finite overflow output last");}
 }
 require(std::fesetround(FE_TONEAREST)==0,"restore fixture rounding mode");
 uint32_t seed=12345;for(uint32_t i=0;i<100000;++i){seed=seed*1664525u+1013904223u;const uint32_t bits=seed;if(((bits>>23)&255)==255)continue;float original;std::memcpy(&original,&bits,4);volatile float observed=original;const double expected=static_cast<double>(observed);require(C::widen(bits)==doubleBits(expected),"independent hardware finite widening oracle");uint32_t back=0;require(C::narrow(C::widen(bits),back)&&back==bits,"arbitrary exact finite binary32 roundtrip");}
 uint64_t wide=UINT64_C(0x123456789abcdef0);for(uint32_t i=0;i<100000;++i){wide=wide*UINT64_C(6364136223846793005)+1;const uint64_t bits=(wide&UINT64_C(0x800fffffffffffff))|(uint64_t(897u+uint32_t(wide>>52)%254u)<<52);double original;std::memcpy(&original,&bits,8);volatile double observed=original;const uint32_t expected=floatBits(static_cast<float>(observed));uint32_t output=99;const bool success=C::narrow(bits,output);require(success==((expected&0x7f800000u)!=0x7f800000u)&&(!success||output==expected),"independent hardware finite normal narrowing oracle");}
 require(C::widen(0x7f800000u)==UINT64_C(0x7ff0000000000000)&&C::widen(0xff800000u)==UINT64_C(0xfff0000000000000),"signed infinity widens without sanitization");require(C::widen(0x7f800001u)==UINT64_C(0x7ff8000020000000),"signaling NaN quiet payload policy");
}
void scratch(){
 using namespace ravet::aax;auto bridge=std::make_unique<AaxNativeDoubleAudio>();AaxNativeAudioMap mapper;
 auto layout=AaxNativeAudioLayout::main(2,2,4);require(mapper.prepare(layout)&&bridge->prepare(layout),"prepared bounded bridge");
 float first[]{1,2,3,4,81},second[]{5,6,7,8,82};const float* inputs[]{first,second};float* outputs[]{second,first};AaxNativeAudioBlock block;const auto start=reinterpret_cast<uintptr_t>(bridge.get());AaxAudioProtectedRange protection{start,start+sizeof(*bridge)};
 require(mapper.map(inputs,outputs,4,-1,block,&protection,1)&&bridge->capture(block),"cross-channel in-place source capture");const auto* native=bridge->buses();auto in=static_cast<const double*const*>(native[0].inputs);auto out=static_cast<double*const*>(native[0].outputs);
 require(in[0][3]==4&&in[1][3]==8,"all aliased source channels widened before host writes");for(uint32_t ch=0;ch<2;++ch)for(uint32_t frame=0;frame<4;++frame)out[ch][frame]=in[ch][frame]*.5;
 AaxNativeDoubleAudio::Failure failure;require(bridge->check(failure)&&bridge->copy(block)&&second[0]==.5f&&first[3]==4&&first[4]==81&&second[4]==82,"whole successful narrowing preserves backing tails");
 first[0]=91;second[0]=92;require(bridge->capture(block),"new capture invalidates prior narrowing readiness");out=static_cast<double*const*>(bridge->buses()[0].outputs);for(uint32_t ch=0;ch<2;++ch)for(uint32_t frame=0;frame<4;++frame)out[ch][frame]=double(ch+frame);out[1][3]=std::numeric_limits<double>::max();require(!bridge->check(failure)&&failure.bus==0&&failure.channel==1&&failure.frame==3&&!bridge->copy(block)&&first[0]==91&&second[0]==92,"late last-output overflow publishes no earlier host prefix");
 AaxNativeAudioBlock zero;require(mapper.map(nullptr,nullptr,0,-1,zero)&&bridge->capture(zero)&&bridge->buses()[0].inputs==nullptr&&bridge->buses()[0].outputs==nullptr&&bridge->check(failure)&&bridge->copy(zero),"zero frame exact native bus shape, no sample arrays");
 auto invalid=layout;invalid.maximumFrames=1025;require(!bridge->prepare(invalid)&&bridge->capture(zero),"late rejected prepared profile retains prior configuration");
}
}
int main(){try{scalar();scratch();std::cout<<"AAX IEEE precision golden/100000 finite/exact aliases/late overflow/zero PASS\n";return 0;}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
