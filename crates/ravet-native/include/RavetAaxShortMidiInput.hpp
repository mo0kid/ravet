#pragma once
#include "RavetAaxNoteInput.hpp"
namespace ravet::aax {
// All defined complete MIDI1 messages of 1..3 bytes, canonical raw native kind14.
// F7 is standalone EndOfExclusive only: no SysEx inference/reassembly or running
// status. F0/fragments, undefined statuses and malformed full lists reject before
// publication. Input live ranges/count/output are aligned/disjoint for capacity.
class AaxShortMidiInput final {
public:
    static constexpr uint32_t maximumPackets=AaxShortMidiPreflight::maximumPackets;
    static bool convert(const AAX_CMidiStream *stream,uint32_t frames,RavetNativeEventV1 *output,uint32_t capacity,uint32_t &count){
        return AaxShortMidiPreflight::convert<true>(stream,frames,output,capacity,count);
    }
};
} // namespace ravet::aax
