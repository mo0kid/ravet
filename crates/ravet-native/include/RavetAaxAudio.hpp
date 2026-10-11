#pragma once
#include "RavetNativeProcessor.h"
#include <algorithm>
#include <array>
#include <cstdint>
#include <limits>

namespace ravet::aax {
// Explicit accepted descriptor layout, not SDK registration or inferred routing.
// One main+eight auxiliary buses,64 total pointer channels per direction. Output
// channels follow descriptor source order. Input slices are explicit; at most one
// sidechain slice uses the host field index, which is not an input array length.
struct AaxNativeAudioLayout {
    static constexpr uint32_t Sidechain = UINT32_MAX;
    uint32_t busCount=0,maximumFrames=0,inputCapacity=0,outputCapacity=0;
    std::array<RavetNativeBusLayoutV1,9> buses{};
    std::array<uint32_t,9> inputFirst{};
    static AaxNativeAudioLayout main(uint32_t inputs,uint32_t outputs,uint32_t frames){
        AaxNativeAudioLayout value{};value.busCount=1;value.maximumFrames=frames;value.inputCapacity=inputs;value.outputCapacity=outputs;value.buses[0]={inputs,outputs};return value;
    }
};
struct AaxAudioProtectedRange {uintptr_t first=0,last=0;};
class AaxNativeAudioMap;
// Fixed borrowed view owns pointer metadata, never host samples. Copy retargets
// all native bus arrays into the destination. Lifetime extends through DSP call.
class AaxNativeAudioBlock {
    uint32_t count_=0,frames_=0;
    std::array<RavetNativeProcessorBusV1,9> buses_{};
    std::array<const float*,64> inputs_{};
    std::array<float*,64> outputs_{};
    void retarget(){
        uint32_t input=0,output=0;
        for(uint32_t i=0;i<count_;++i){auto &bus=buses_[i];bus.inputs=frames_&&bus.input_channels?inputs_.data()+input:nullptr;bus.outputs=frames_&&bus.output_channels?outputs_.data()+output:nullptr;input+=bus.input_channels;output+=bus.output_channels;}
    }
public:
    AaxNativeAudioBlock()=default;
    AaxNativeAudioBlock(const AaxNativeAudioBlock &other){*this=other;}
    AaxNativeAudioBlock &operator=(const AaxNativeAudioBlock &other){if(this!=&other){count_=other.count_;frames_=other.frames_;buses_=other.buses_;inputs_=other.inputs_;outputs_=other.outputs_;retarget();}return *this;}
    const RavetNativeProcessorBusV1 *buses()const{return buses_.data();}
    uint32_t busCount()const{return count_;}
    uint32_t frames()const{return frames_;}
    friend class AaxNativeAudioMap;
};
// Prepared mapping uses fixed storage/no heap/locks. Caller supplies live/aligned
// metadata of declared capacities and sample prefixes; raw AAX arrays have no byte
// capacity fields. Whole map/sample/alias validation precedes view publication.
// Zero-frame omission allows both arrays null; no sample pointers are consumed.
// Input/output sample alias is permitted; outputs cannot intersect each other,
// pointer metadata, map/block storage or explicit protected owner/MIDI ranges.
class AaxNativeAudioMap {
    AaxNativeAudioLayout layout_{};
    static bool range(const void *pointer,uintptr_t bytes,size_t alignment,AaxAudioProtectedRange &out){
        const auto first=reinterpret_cast<uintptr_t>(pointer);if(!first||first%alignment||first>std::numeric_limits<uintptr_t>::max()-bytes)return false;out={first,first+bytes};return true;
    }
    static bool overlap(AaxAudioProtectedRange a,AaxAudioProtectedRange b){return a.first<b.last&&b.first<a.last;}
    static bool protectedRange(AaxAudioProtectedRange value,const AaxAudioProtectedRange *ranges,uint32_t count){for(uint32_t i=0;i<count;++i)if(overlap(value,ranges[i]))return true;return false;}
    static bool validProtection(const AaxAudioProtectedRange *ranges,uint32_t count){
        if(count>16)return false;if(!count)return true;AaxAudioProtectedRange metadata{};if(!range(ranges,sizeof(*ranges)*count,alignof(AaxAudioProtectedRange),metadata))return false;for(uint32_t i=0;i<count;++i)if(ranges[i].first>ranges[i].last)return false;return true;
    }
public:
    bool prepare(const AaxNativeAudioLayout &next){
        if(!next.busCount||next.busCount>9||!next.maximumFrames||next.maximumFrames>1024||next.inputCapacity>64||next.outputCapacity>64)return false;
        uint32_t inputs=0,outputs=0,sidechain=0;
        for(uint32_t i=0;i<9;++i){const auto bus=next.buses[i];if(i>=next.busCount){if(bus.inputs||bus.outputs||next.inputFirst[i])return false;continue;}
            if(bus.inputs>64||bus.outputs>64||(!bus.inputs&&!bus.outputs)||inputs>64-bus.inputs||outputs>64-bus.outputs)return false;inputs+=bus.inputs;outputs+=bus.outputs;
            if(!bus.inputs){if(next.inputFirst[i])return false;continue;}
            if(next.inputFirst[i]==AaxNativeAudioLayout::Sidechain){if(i==0||++sidechain>1||bus.inputs>next.inputCapacity)return false;continue;}
            if(next.inputFirst[i]>next.inputCapacity||bus.inputs>next.inputCapacity-next.inputFirst[i]||(i==0&&next.inputFirst[i]))return false;
            for(uint32_t j=0;j<i;++j){const auto earlier=next.buses[j];if(!earlier.inputs||next.inputFirst[j]==AaxNativeAudioLayout::Sidechain)continue;const auto first=next.inputFirst[i],prior=next.inputFirst[j];if(first<prior+earlier.inputs&&prior<first+bus.inputs)return false;}
        }
        if(outputs!=next.outputCapacity||inputs>next.inputCapacity)return false;layout_=next;return true;
    }
    bool valid()const{return layout_.busCount!=0;}
    const AaxNativeAudioLayout &layout()const{return layout_;}
    // A dynamic index must select a disjoint live sidechain slice. No negative/
    // missing/inactive sidechain sample inference. Omitted frame0 reads none.
    bool map(const float *const *inputs,float *const *outputs,uint32_t frames,int32_t sidechainFirst,AaxNativeAudioBlock &block,const AaxAudioProtectedRange *protectedRanges=nullptr,uint32_t protectedCount=0)const{
        if(!valid()||frames>layout_.maximumFrames||!validProtection(protectedRanges,protectedCount))return false;
        AaxAudioProtectedRange inputMeta{},outputMeta{},mapMeta{},blockMeta{},protectionMeta{};
        range(this,sizeof(*this),alignof(AaxNativeAudioMap),mapMeta);range(&block,sizeof(block),alignof(AaxNativeAudioBlock),blockMeta);
        if(protectedCount)range(protectedRanges,sizeof(*protectedRanges)*protectedCount,alignof(AaxAudioProtectedRange),protectionMeta);
        const bool omitted=!frames&&!inputs&&!outputs;
        if(!omitted){
            if(layout_.inputCapacity&&(!range(inputs,sizeof(float*)*layout_.inputCapacity,alignof(float*),inputMeta)||overlap(inputMeta,mapMeta)||overlap(inputMeta,blockMeta)||protectedRange(inputMeta,protectedRanges,protectedCount)))return false;
            if(layout_.outputCapacity&&(!range(outputs,sizeof(float*)*layout_.outputCapacity,alignof(float*),outputMeta)||overlap(outputMeta,mapMeta)||overlap(outputMeta,blockMeta)||protectedRange(outputMeta,protectedRanges,protectedCount)))return false;
        }
        std::array<uint32_t,9> inputStarts{};
        for(uint32_t i=0;i<layout_.busCount;++i){const auto bus=layout_.buses[i];if(!bus.inputs)continue;
            if(layout_.inputFirst[i]==AaxNativeAudioLayout::Sidechain){if(omitted){inputStarts[i]=0;continue;}if(sidechainFirst<0||uint32_t(sidechainFirst)>layout_.inputCapacity||bus.inputs>layout_.inputCapacity-uint32_t(sidechainFirst))return false;inputStarts[i]=uint32_t(sidechainFirst);}else inputStarts[i]=layout_.inputFirst[i];
            if(!omitted)for(uint32_t j=0;j<i;++j){const auto prior=layout_.buses[j];if(prior.inputs&&inputStarts[i]<inputStarts[j]+prior.inputs&&inputStarts[j]<inputStarts[i]+bus.inputs)return false;}
        }
        AaxNativeAudioBlock next{};next.count_=layout_.busCount;next.frames_=frames;uint32_t in=0,out=0;
        std::array<AaxAudioProtectedRange,64> outputRanges{};uint32_t used=0;
        for(uint32_t i=0;i<layout_.busCount;++i){const auto bus=layout_.buses[i];next.buses_[i]={bus.inputs,bus.outputs,nullptr,nullptr};
            if(frames)for(uint32_t channel=0;channel<bus.inputs;++channel){const auto pointer=inputs[inputStarts[i]+channel];AaxAudioProtectedRange samples{};if(!range(pointer,sizeof(float)*frames,alignof(float),samples)||overlap(samples,inputMeta)||overlap(samples,outputMeta)||overlap(samples,mapMeta)||overlap(samples,blockMeta)||overlap(samples,protectionMeta)||protectedRange(samples,protectedRanges,protectedCount))return false;next.inputs_[in+channel]=pointer;}
            if(frames)for(uint32_t channel=0;channel<bus.outputs;++channel){const auto pointer=outputs[out+channel];AaxAudioProtectedRange samples{};if(!range(pointer,sizeof(float)*frames,alignof(float),samples)||overlap(samples,inputMeta)||overlap(samples,outputMeta)||overlap(samples,mapMeta)||overlap(samples,blockMeta)||overlap(samples,protectionMeta)||protectedRange(samples,protectedRanges,protectedCount))return false;for(uint32_t prior=0;prior<used;++prior)if(overlap(samples,outputRanges[prior]))return false;outputRanges[used++]=samples;next.outputs_[out+channel]=pointer;}
            in+=bus.inputs;out+=bus.outputs;
        }
        block=next;return true;
    }
    // Known supported prefixes only; metadata/owner/MIDI/protection storage never
    // cleared. Invalid raw sample capacity remains a foreign caller violation.
    void silence(const float *const *inputs,float *const *outputs,int32_t frames,const AaxAudioProtectedRange *protectedRanges=nullptr,uint32_t protectedCount=0)const{
        if(!valid()||frames<=0||!validProtection(protectedRanges,protectedCount))return;AaxAudioProtectedRange metadata{},inMeta{},self{},protectionMeta{};range(this,sizeof(*this),alignof(AaxNativeAudioMap),self);
        if(protectedCount)range(protectedRanges,sizeof(*protectedRanges)*protectedCount,alignof(AaxAudioProtectedRange),protectionMeta);
        if(!range(outputs,sizeof(float*)*layout_.outputCapacity,alignof(float*),metadata)||overlap(metadata,self)||protectedRange(metadata,protectedRanges,protectedCount))return;
        const bool haveInputs=layout_.inputCapacity&&range(inputs,sizeof(float*)*layout_.inputCapacity,alignof(float*),inMeta);
        const auto count=std::min(uint32_t(frames),layout_.maximumFrames);std::array<float*,64> pointers{};for(uint32_t i=0;i<layout_.outputCapacity;++i)pointers[i]=outputs[i];
        for(uint32_t i=0;i<layout_.outputCapacity;++i){AaxAudioProtectedRange samples{};if(range(pointers[i],sizeof(float)*count,alignof(float),samples)&&!overlap(samples,metadata)&&(!haveInputs||!overlap(samples,inMeta))&&!overlap(samples,self)&&!overlap(samples,protectionMeta)&&!protectedRange(samples,protectedRanges,protectedCount))std::fill_n(pointers[i],count,0.f);}
    }
};
} // namespace ravet::aax
