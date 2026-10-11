#pragma once
#include "RavetAaxAudio.hpp"
#include <cstring>
#include <limits>

namespace ravet::aax {
// IEEE storage conversion for the host-f32/native-f64 bridge. No floating
// comparison, cast, fenv change, allocation or signal sanitization. NaNs quiet;
// finite narrowing uses nearest/even with gradual underflow. A rounded finite
// overflow rejects without publishing an output word.
struct AaxIeeePrecision {
    static_assert(sizeof(float)==4&&sizeof(double)==8&&std::numeric_limits<float>::is_iec559&&std::numeric_limits<double>::is_iec559
        &&std::numeric_limits<float>::radix==2&&std::numeric_limits<double>::radix==2
        &&std::numeric_limits<float>::digits==24&&std::numeric_limits<double>::digits==53
        &&std::numeric_limits<float>::min_exponent==-125&&std::numeric_limits<float>::max_exponent==128
        &&std::numeric_limits<double>::min_exponent==-1021&&std::numeric_limits<double>::max_exponent==1024);
    static uint64_t widen(uint32_t bits)noexcept{
        const uint64_t sign=uint64_t(bits>>31)<<63;
        const uint32_t exponent=(bits>>23)&255u,fraction=bits&0x7fffffu;
        if(exponent==255)return sign|UINT64_C(0x7ff0000000000000)|(fraction?(uint64_t(fraction)<<29)|UINT64_C(0x8000000000000):0);
        if(exponent)return sign|(uint64_t(exponent+896u)<<52)|(uint64_t(fraction)<<29);
        if(!fraction)return sign;
        uint32_t top=0;for(uint32_t value=fraction;value>>=1;)++top;
        return sign|(uint64_t(top+874u)<<52)|(uint64_t(fraction^(1u<<top))<<(52u-top));
    }
private:
    static uint64_t rounded(uint64_t significand,uint32_t shift)noexcept{
        const uint64_t result=significand>>shift,remainder=significand&((UINT64_C(1)<<shift)-1u),half=UINT64_C(1)<<(shift-1u);
        return result+uint64_t(remainder>half||(remainder==half&&(result&1u)));
    }
public:
    static bool narrow(uint64_t bits,uint32_t& output)noexcept{
        const uint32_t sign=uint32_t(bits>>32)&0x80000000u,exponent=uint32_t((bits>>52)&2047u);
        const uint64_t fraction=bits&UINT64_C(0xfffffffffffff);
        uint32_t result=sign;
        if(exponent==2047u)result|=0x7f800000u|(fraction?(uint32_t(fraction>>29)|0x400000u):0u);
        else if(exponent>=873u){
            int32_t power=int32_t(exponent)-1023;
            const uint64_t significand=UINT64_C(0x10000000000000)|fraction;
            if(power>=-126){
                uint64_t value=rounded(significand,29);
                if(value==UINT64_C(0x1000000)){value>>=1;++power;}
                if(power>=128)return false;
                result|=(uint32_t(power+127)<<23)|(uint32_t(value)&0x7fffffu);
            }else result|=uint32_t(rounded(significand,uint32_t(-power-97)));
        }
        output=result;return true;
    }
};

// CONTROL-prepared scratch, owned by the heap-resident processor owner. The
// caller supplies an already whole-validated AAX map protecting this storage.
// AUDIO exclusively owns capture/native/check/copy as one uninterrupted phase;
// no foreign callback may mutate the scratch between validation and host copy.
// Host ranges stay live through the complete phase. No copy/move/heap in phase.
class AaxNativeDoubleAudio final {
    AaxNativeAudioLayout layout_{};
    std::array<std::array<double,1024>,64> inputs_{};
    std::array<std::array<double,1024>,64> outputs_{};
    std::array<const double*,64> inputPointers_{};
    std::array<double*,64> outputPointers_{};
    std::array<RavetNativeProcessorBusV1,9> buses_{};
    uint32_t frames_=0;
    bool captured_=false,checked_=false;
    const AaxNativeAudioBlock* capturedBlock_=nullptr;
    static bool overlap(const void* pointer,size_t size,const void* own,size_t ownSize)noexcept{
        const auto p=reinterpret_cast<uintptr_t>(pointer),o=reinterpret_cast<uintptr_t>(own);
        return !p||size>UINTPTR_MAX-p||(p<o+ownSize&&o<p+size);
    }
public:
    struct Failure{uint32_t bus=0,channel=0,frame=0;};
    AaxNativeDoubleAudio()=default;
    AaxNativeDoubleAudio(const AaxNativeDoubleAudio&)=delete;
    AaxNativeDoubleAudio& operator=(const AaxNativeDoubleAudio&)=delete;
    bool prepare(const AaxNativeAudioLayout& layout)noexcept{
        AaxNativeAudioMap validator;if(!validator.prepare(layout))return false;
        layout_=layout;captured_=checked_=false;frames_=0;return true;
    }
    // Whole shape/self-overlap preflight precedes any input capture. The mapper
    // owns all other metadata/sample/alias checks. Every raw input bit is valid.
    bool capture(const AaxNativeAudioBlock& block)noexcept{
        captured_=checked_=false;capturedBlock_=nullptr;
        if(!layout_.busCount||block.busCount()!=layout_.busCount||block.frames()>layout_.maximumFrames)return false;
        const auto frames=block.frames();
        for(uint32_t bus=0;bus<layout_.busCount;++bus){
            const auto& view=block.buses()[bus];const auto shape=layout_.buses[bus];
            if(view.input_channels!=shape.inputs||view.output_channels!=shape.outputs)return false;
            if(!frames)continue;
            if(shape.inputs&&(overlap(view.inputs,sizeof(float*)*shape.inputs,this,sizeof(*this))||reinterpret_cast<uintptr_t>(view.inputs)%alignof(float*)))return false;
            if(shape.outputs&&(overlap(view.outputs,sizeof(float*)*shape.outputs,this,sizeof(*this))||reinterpret_cast<uintptr_t>(view.outputs)%alignof(float*)))return false;
            const auto input=static_cast<const float*const*>(view.inputs);const auto output=static_cast<float*const*>(view.outputs);
            for(uint32_t channel=0;channel<shape.inputs;++channel)if(overlap(input[channel],sizeof(float)*frames,this,sizeof(*this))||reinterpret_cast<uintptr_t>(input[channel])%alignof(float))return false;
            for(uint32_t channel=0;channel<shape.outputs;++channel)if(overlap(output[channel],sizeof(float)*frames,this,sizeof(*this))||reinterpret_cast<uintptr_t>(output[channel])%alignof(float))return false;
        }
        uint32_t in=0,out=0;
        for(uint32_t bus=0;bus<layout_.busCount;++bus){
            const auto shape=layout_.buses[bus];const auto input=static_cast<const float*const*>(block.buses()[bus].inputs);
            for(uint32_t channel=0;channel<shape.inputs;++channel){inputPointers_[in+channel]=inputs_[in+channel].data();for(uint32_t frame=0;frame<frames;++frame){uint32_t source=0;std::memcpy(&source,input[channel]+frame,4);const auto target=AaxIeeePrecision::widen(source);std::memcpy(inputs_[in+channel].data()+frame,&target,8);}}
            for(uint32_t channel=0;channel<shape.outputs;++channel){outputPointers_[out+channel]=outputs_[out+channel].data();std::fill_n(outputs_[out+channel].data(),frames,0.);}
            buses_[bus]={shape.inputs,shape.outputs,frames&&shape.inputs?inputPointers_.data()+in:nullptr,frames&&shape.outputs?outputPointers_.data()+out:nullptr};in+=shape.inputs;out+=shape.outputs;
        }
        frames_=frames;capturedBlock_=&block;captured_=true;return true;
    }
    const RavetNativeProcessorBusV1* buses()const noexcept{return captured_?buses_.data():nullptr;}
    uint32_t busCount()const noexcept{return captured_?layout_.busCount:0;}
    bool check(Failure& failure)noexcept{
        checked_=false;if(!captured_)return false;
        uint32_t out=0;for(uint32_t bus=0;bus<layout_.busCount;++bus){for(uint32_t channel=0;channel<layout_.buses[bus].outputs;++channel)for(uint32_t frame=0;frame<frames_;++frame){uint64_t source=0;uint32_t target=0;std::memcpy(&source,outputs_[out+channel].data()+frame,8);if(!AaxIeeePrecision::narrow(source,target)){failure={bus,channel,frame};return false;}}out+=layout_.buses[bus].outputs;}
        checked_=true;return true;
    }
    // Same validated host block as capture, with no intervening foreign access.
    // The owner commits only after check succeeded; output tails are untouched.
    bool copy(const AaxNativeAudioBlock& block)noexcept{
        if(!checked_||&block!=capturedBlock_||block.frames()!=frames_||block.busCount()!=layout_.busCount)return false;
        for(uint32_t bus=0;bus<layout_.busCount;++bus)if(block.buses()[bus].input_channels!=layout_.buses[bus].inputs||block.buses()[bus].output_channels!=layout_.buses[bus].outputs)return false;
        uint32_t out=0;for(uint32_t bus=0;bus<layout_.busCount;++bus){const auto target=static_cast<float*const*>(block.buses()[bus].outputs);for(uint32_t channel=0;channel<layout_.buses[bus].outputs;++channel)for(uint32_t frame=0;frame<frames_;++frame){uint64_t source=0;uint32_t value=0;std::memcpy(&source,outputs_[out+channel].data()+frame,8);(void)AaxIeeePrecision::narrow(source,value);std::memcpy(target[channel]+frame,&value,4);}out+=layout_.buses[bus].outputs;}
        checked_=false;return true;
    }
};
} // namespace ravet::aax
