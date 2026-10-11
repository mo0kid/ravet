#ifndef RAVET_VST3_HOSTED_AUDIO_PROFILE_HPP
#define RAVET_VST3_HOSTED_AUDIO_PROFILE_HPP
#include "RavetVst3HostedAudio.h"
#include "OwnershipRanges.hpp"
#include <array>
#include <bitset>
#include <cstring>
#include <limits>
namespace ravet::vst3::hosted_audio {
// Pure structural validation. Arrangement semantics and returned SDK identity
// are checked separately against the actual selected component/processor.
inline bool validProfile(const RavetVst3HostedAudioProfile& p) noexcept {
    static_assert(sizeof(double)==sizeof(uint64_t)&&std::numeric_limits<double>::is_iec559);
    uint64_t bits=0;std::memcpy(&bits,&p.sample_rate,sizeof(bits));
    if(p.struct_size!=sizeof(p)||p.reserved||p.maximum_frames<1||p.maximum_frames>65536||
       (p.sample_bits!=32&&p.sample_bits!=64)||p.input_buses>16||p.output_buses>16||
       (!p.input_buses&&!p.output_buses)||(bits&UINT64_C(0x7ff0000000000000))==UINT64_C(0x7ff0000000000000)||
       !(p.sample_rate>0)||p.sample_rate>768000)return false;
    for(bool output:{false,true})for(uint32_t i=0;i<16;++i){
        const auto& request=(output?p.outputs:p.inputs)[i];const auto count=output?p.output_buses:p.input_buses;
        if(request.reserved||request.active>1||(i>=count&&(request.arrangement||request.active)))return false;
    }
    return true;
}
struct PackedBus { uint32_t channels=0, offset=0; bool active=false; uint64_t arrangement=0; };
struct PackedLayout {
    std::array<PackedBus,16> inputs{},outputs{};
    uint32_t inputBuses=0,outputBuses=0,inputChannels=0,outputChannels=0;
    // Complete output-last construction, consuming already inspected channel
    // counts/arrangements in exact SDK index order. Empty inactive buses are
    // admitted; an active empty bus is not a usable audio channel selection.
    bool operator==(const PackedLayout& other)const noexcept {
        if(inputBuses!=other.inputBuses||outputBuses!=other.outputBuses||inputChannels!=other.inputChannels||outputChannels!=other.outputChannels)return false;
        for(bool output:{false,true})for(size_t i=0;i<16;++i){const auto& a=(output?outputs:inputs)[i];const auto& b=(output?other.outputs:other.inputs)[i];
            if(a.channels!=b.channels||a.offset!=b.offset||a.active!=b.active||a.arrangement!=b.arrangement)return false;}
        return true;
    }
};
inline bool packedLayout(const RavetVst3HostedAudioProfile& p,
        const std::array<uint32_t,16>& inputs,const std::array<uint32_t,16>& outputs,PackedLayout& destination)noexcept {
    if(!validProfile(p))return false;
    PackedLayout candidate;candidate.inputBuses=p.input_buses;candidate.outputBuses=p.output_buses;
    for(bool output:{false,true}){
        const auto count=output?p.output_buses:p.input_buses;auto& total=output?candidate.outputChannels:candidate.inputChannels;
        auto& buses=output?candidate.outputs:candidate.inputs;
        for(uint32_t i=0;i<16;++i){const auto channels=(output?outputs:inputs)[i];const auto& request=(output?p.outputs:p.inputs)[i];
            if(channels>64||(i>=count&&channels)||(request.active&&!channels)||
               (i<count&&channels!=std::bitset<64>(request.arrangement).count()))return false;
            if(i<count){buses[i]={channels,total,bool(request.active),request.arrangement};if(request.active){if(channels>64-total)return false;total+=channels;}}
        }
    }
    if(!candidate.inputChannels&&!candidate.outputChannels)return false;
    destination=candidate;return true;
}
inline bool requestedLayout(const RavetVst3HostedAudioProfile& p,PackedLayout& destination)noexcept {
    if(!validProfile(p))return false;
    std::array<uint32_t,16> inputs{},outputs{};
    for(uint32_t i=0;i<p.input_buses;++i)inputs[i]=uint32_t(std::bitset<64>(p.inputs[i].arrangement).count());
    for(uint32_t i=0;i<p.output_buses;++i)outputs[i]=uint32_t(std::bitset<64>(p.outputs[i].arrangement).count());
    return packedLayout(p,inputs,outputs,destination);
}

}
#endif
