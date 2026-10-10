#pragma once
#include "RavetNativeEvent.h"
#include "RavetNativeScalars.hpp"
#include <array>
#include <cmath>
#include <cstring>
namespace ravet::native {
// SDK-independent complete MIDI1 short-message output profile. Raw MIDI bytes
// remain exact; ordinary nonidentity notes quantize velocity nearest/ties-up.
// Positive On clamps to1, exact-zero On remains MIDI velocityzero-Off convention.
// Identity/scoped/expression/completion/parameter/UMP/SysEx mappings are absent.
struct NativeMidiMessage {
    uint32_t offset = 0, size = 0;
    std::array<uint8_t,3> bytes{};
};
namespace midi_output_detail {
inline uint64_t bits(const double &value) { uint64_t out=0;std::memcpy(&out,&value,sizeof(out));return out; }
inline uint32_t length(uint8_t status) {
    if(status>=0x80&&status<0xf0)return(status&0xe0)==0xc0?2:3;
    if(status==0xf1||status==0xf3)return 2;
    if(status==0xf2)return 3;
    if(status==0xf6||status==0xf8||status==0xfa||status==0xfb||status==0xfc||status==0xfe||status==0xff)return 1;
    return 0;
}
}
// Whole one-record canonical/timing checks precede output publication. Input must
// be live/aligned/disjoint from output. Whole-list source order/capacity/routing
// and host callbacks remain the consumer's responsibility. No heap/locks/I/O.
inline bool nativeMidiOutput(const RavetNativeEventV1 &input,uint32_t frames,NativeMidiMessage &output) {
    using namespace midi_output_detail;
    if(frames>65536||(frames?input.offset>=frames:input.offset!=0)||input.parameter_id||input.note_id||input.scope)return false;
    NativeMidiMessage candidate{};candidate.offset=input.offset;
    if(input.kind==RAVET_NATIVE_MIDI) {
        if(input.channel||input.key||bits(input.value)||input.detail<1||input.detail>3)return false;
        for(uint32_t i=1;i<4;++i)if(input.words[i])return false;
        if((input.words[0]>>(8*input.detail))!=0)return false;
        for(uint32_t i=0;i<input.detail;++i)candidate.bytes[i]=uint8_t(input.words[0]>>(8*i));
        if(length(candidate.bytes[0])!=input.detail)return false;
        for(uint32_t i=1;i<input.detail;++i)if(candidate.bytes[i]>127)return false;
        candidate.size=input.detail;
    } else if(input.kind==RAVET_NATIVE_NOTE_ON||input.kind==RAVET_NATIVE_NOTE_OFF) {
        if(input.detail||input.channel>15||input.key>127||!scalar::finite(input.value)||input.value<0.||input.value>1.)return false;
        for(const auto word:input.words)if(word)return false;
        const float small=float(input.value);const double exact=double(small);
        if(!scalar::finite(small)||bits(exact)!=bits(input.value))return false;
        const bool on=input.kind==RAVET_NATIVE_NOTE_ON;
        auto velocity=uint32_t(std::floor(input.value*127.+.5));
        if(on&&input.value>0.&&velocity==0)velocity=1;
        candidate.bytes={uint8_t((on?0x90:0x80)|input.channel),uint8_t(input.key),uint8_t(velocity)};candidate.size=3;
    } else return false;
    output=candidate;return true;
}
} // namespace ravet::native
