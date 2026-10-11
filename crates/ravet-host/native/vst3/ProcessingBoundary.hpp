#ifndef RAVET_VST3_PROCESSING_BOUNDARY_HPP
#define RAVET_VST3_PROCESSING_BOUNDARY_HPP
#include "RavetVst3Processing.h"
#include "RavetVst3HostedAudio.h"
#include "OwnershipRanges.hpp"
#include <array>
namespace ravet::vst3::processing_draft {
// Pure whole-call admission, before queue reservation or scratch mutation.
// Full capacities participate in ownership checks and SDK protection, although
// only active audio prefixes and successful output prefixes are written.
template<class Sample,class Block,class Input,class Output,
         size_t MaximumChannels,size_t RangeCapacity,size_t MinimumOutputs>
class BasicProcessingBoundary {
    using Range=parameters_draft::Range;
    using Ranges=std::array<Range,RangeCapacity>;
    Ranges protected_{};
    size_t count_=0;
public:
    const Range* ranges()const noexcept{return protected_.data();}
    size_t count()const noexcept{return count_;}
    bool append(Range value)noexcept {
        if(value.start>value.end||count_==protected_.size())return false;
        protected_[count_++]=value;return true;
    }
    bool appendOwner(Range owner,size_t callerCount)noexcept {
        if(callerCount>count_||owner.start>owner.end)return false;
        for(size_t i=0;i<callerCount;++i)if(parameters_draft::overlaps(protected_[i],owner))return false;
        return append(owner);
    }
    template<class External> bool admit(const Block& block,
            const RavetVst3ParameterPoint* points,uint32_t pointCount,
            const RavetVst3EventRecord* events,uint32_t eventCount,
            const uint8_t* payload,uint32_t payloadBytes,const RavetVst3ContextRecord* context,
            const RavetVst3ProcessingOutputs& outputs,const RavetVst3ProcessingReport& report,External external) {
        using parameters_draft::range;using parameters_draft::overlaps;
        BasicProcessingBoundary candidate;
        std::array<Range,7> reads{};std::array<Range,6> writes{};
        std::array<Range,2*MaximumChannels> audio{};std::array<Range,MaximumChannels> prefixes{};
        auto span=[&](const void* p,size_t n,size_t width,size_t alignment,Range& r){
            return range(p,n,width,alignment,r)&&external(p,n*width,alignment)&&candidate.append(r);
        };
        // Validate metadata ownership before interpreting any caller field.
        if(!span(&block,1,sizeof(block),alignof(Block),reads[0])||
           !span(&outputs,1,sizeof(outputs),alignof(RavetVst3ProcessingOutputs),writes[0])||
           !span(&report,1,sizeof(report),alignof(RavetVst3ProcessingReport),writes[1]))return false;
        if(block.struct_size!=sizeof(block)||block.input_channels>MaximumChannels||block.output_channels<MinimumOutputs||block.output_channels>MaximumChannels||
           outputs.struct_size!=sizeof(outputs)||outputs.reserved||report.struct_size!=sizeof(report)||
           pointCount>2048||eventCount>2048||payloadBytes>65536||outputs.event_capacity>2048||
           outputs.payload_capacity>65536||outputs.parameter_queue_capacity>64||outputs.parameter_point_capacity>2048)return false;
        if(!span(block.inputs,block.input_channels,sizeof(Input),alignof(Input),reads[1])||
           !span(block.outputs,block.output_channels,sizeof(Output),alignof(Output),reads[2])||
           !span(points,pointCount,sizeof(*points),alignof(RavetVst3ParameterPoint),reads[3])||
           !span(events,eventCount,sizeof(*events),alignof(RavetVst3EventRecord),reads[4])||
           !span(payload,payloadBytes,1,1,reads[5])||
           !span(context,context?1:0,sizeof(RavetVst3ContextRecord),alignof(RavetVst3ContextRecord),reads[6])||
           !span(outputs.events,outputs.event_capacity,sizeof(*outputs.events),alignof(RavetVst3EventRecord),writes[2])||
           !span(outputs.payload,outputs.payload_capacity,1,1,writes[3])||
           !span(outputs.parameter_ids,outputs.parameter_queue_capacity,sizeof(uint32_t),alignof(uint32_t),writes[4])||
           !span(outputs.parameter_points,outputs.parameter_point_capacity,sizeof(*outputs.parameter_points),alignof(RavetVst3ParameterPoint),writes[5]))return false;
        size_t audioCount=0;
        for(uint32_t ch=0;ch<block.input_channels;++ch){const auto& v=block.inputs[ch];
            if(v.reserved||v.capacity<block.frames||!span(v.samples,v.capacity,sizeof(Sample),alignof(Sample),audio[audioCount++]))return false;
        }
        for(uint32_t ch=0;ch<block.output_channels;++ch){const auto& v=block.outputs[ch];
            if(v.reserved||v.capacity<block.frames||!span(v.samples,v.capacity,sizeof(Sample),alignof(Sample),audio[audioCount++])||
               !range(v.samples,block.frames,sizeof(Sample),alignof(Sample),prefixes[ch]))return false;
        }
        for(size_t i=0;i<writes.size();++i){
            for(size_t j=0;j<i;++j)if(overlaps(writes[i],writes[j]))return false;
            for(auto r:reads)if(overlaps(writes[i],r))return false;
            for(size_t j=0;j<audioCount;++j)if(overlaps(writes[i],audio[j]))return false;
        }
        for(uint32_t ch=0;ch<block.output_channels;++ch){
            for(auto r:reads)if(overlaps(prefixes[ch],r))return false;
            for(uint32_t j=0;j<ch;++j)if(overlaps(prefixes[ch],prefixes[j]))return false;
        }
        // Audio inputs may alias outputs: the complete input is copied before
        // DSP. Other borrowed metadata/payload cannot be overwritten by audio.
        *this=candidate;return true;
    }
};
// Legacy entry retains its original two-channel/128-range admission. The
// additive hosted entry explicitly opts into all 64 packed channels and 256
// numeric ranges; no existing table/wire layout or failure domain is changed.
using ProcessingBoundary=BasicProcessingBoundary<float,RavetVst3AudioBlock,RavetVst3InputChannel,RavetVst3OutputChannel,2,128,1>;
using HostedBoundary32=BasicProcessingBoundary<float,RavetVst3AudioBlock,RavetVst3InputChannel,RavetVst3OutputChannel,64,256,0>;
using HostedBoundary64=BasicProcessingBoundary<double,RavetVst3HostedAudioBlock64,RavetVst3HostedInputChannel64,RavetVst3HostedOutputChannel64,64,256,0>;
static constexpr size_t HostedCallerRanges=7+6+64+64;
static constexpr size_t HostedChildRanges=1+2+4+2+33;
static constexpr size_t HostedOuterRanges=4+17+17+33;
static constexpr size_t HostedMaximumRanges=HostedCallerRanges+HostedChildRanges+HostedOuterRanges;
static_assert(HostedCallerRanges==141&&HostedChildRanges==42&&HostedOuterRanges==71);
static_assert(HostedMaximumRanges==254&&HostedMaximumRanges<=256);
}
#endif
