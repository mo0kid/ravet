#pragma once
#include "RavetNativeMidiOutput.hpp"
#include <AudioToolbox/AudioToolbox.h>
#include <CoreMIDI/CoreMIDI.h>
#include <array>
#include <cstddef>
#include <cstring>
#include <limits>
namespace ravet::au {
enum class MidiOutputRejection { None, Busy, NotStaged, MissingCallback, HostRejected };
struct MidiOutputDelivery {
    MidiOutputRejection reason;
    uint32_t attempted;
    OSStatus hostStatus;
};
// Fixed legacy CoreMIDI packet staging, separate from AU property/routing and
// native processor scheduling. Only serialized Render delivery with stopped
// callback/metadata changes; host callbacks may allocate/block/retain effects.
// Host callbacks must not throw or destroy the owner while borrowing packets.
// No implicit output names/indices, protocol negotiation or realtime promise.
template<size_t MaximumEvents=1024> class NativeAuMidiOutput final {
    static_assert(MaximumEvents>0&&MaximumEvents<=1024);
    // Upper bound handles independent max3-byte packets plus ARM4-byte padding;
    // official CoreMIDI builder can coalesce equal-offset complete messages.
    static constexpr size_t packetBytes=offsetof(MIDIPacketList,packet)+MaximumEvents*(offsetof(MIDIPacket,data)+3+3);
    static_assert(packetBytes<=65536);
public:
    static constexpr size_t capacity=MaximumEvents;
    NativeAuMidiOutput()=default;
    NativeAuMidiOutput(const NativeAuMidiOutput &)=delete;
    NativeAuMidiOutput &operator=(const NativeAuMidiOutput &)=delete;
    // Validate all records/ranges/order and stage one complete packet list before
    // publication. Every nonreentrant stage attempt invalidates delivery readiness
    // first; failure retains previous bytes/count as diagnostic snapshots only. Live input
    // must not alias writable instance/storage; no arbitrary memory ownership
    // proof from scalar/address preflight. Zero-frame offsets0 only.
    bool stage(const RavetNativeEventV1 *input,size_t count,uint32_t frames) {
        if(delivering_)return false;
        ready_=false;
        if(frames>65536||count>MaximumEvents||(count&&(!input||reinterpret_cast<uintptr_t>(input)%alignof(RavetNativeEventV1))))return false;
        if(count&&count*sizeof(*input)>std::numeric_limits<uintptr_t>::max()-reinterpret_cast<uintptr_t>(input))return false;
        std::array<native::NativeMidiMessage,MaximumEvents> messages{};
        uint32_t previous=0;
        for(size_t i=0;i<count;++i){if(input[i].offset<previous||!native::nativeMidiOutput(input[i],frames,messages[i]))return false;previous=input[i].offset;}
        alignas(MIDIPacketList) std::array<uint8_t,packetBytes> candidate{};
        auto *list=reinterpret_cast<MIDIPacketList *>(candidate.data());auto *packet=MIDIPacketListInit(list);
        for(size_t i=0;i<count;++i){const auto &message=messages[i];packet=MIDIPacketListAdd(list,candidate.size(),packet,MIDITimeStamp(message.offset),message.size,message.bytes.data());if(!packet)return false;}
        bytes_=candidate;count_=uint32_t(count);ready_=true;return true;
    }
    // Exactly one nonempty callback; consumed before invocation, even on refusal.
    // Callback failure may already have irreversible host prefix effects, with
    // acceptance unknown. No retry/logging/DSP or audio rollback. Time/userdata
    // and packet borrowing last only through callback; host must copy data to retain it.
    // Every attempt consumes ready state; failed staging cannot replay stale
    // data. Reentrant stage/deliver is rejected and cannot touch borrowed bytes.
    MidiOutputDelivery deliver(const AUMIDIOutputCallbackStruct &callback,const AudioTimeStamp *time,UInt32 output) {
        if(delivering_)return {MidiOutputRejection::Busy,0,noErr};
        if(!ready_)return {MidiOutputRejection::NotStaged,0,noErr};
        ready_=false;
        if(count_&&(!callback.midiOutputCallback||!time))return {MidiOutputRejection::MissingCallback,0,noErr};
        if(!count_)return {MidiOutputRejection::None,0,noErr};
        const auto installed=callback;const auto count=count_;
        delivering_=true;
        const auto status=installed.midiOutputCallback(installed.userData,time,output,reinterpret_cast<const MIDIPacketList *>(bytes_.data()));
        delivering_=false;
        return {status?MidiOutputRejection::HostRejected:MidiOutputRejection::None,count,status};
    }
    const MIDIPacketList *packets() const noexcept { return reinterpret_cast<const MIDIPacketList *>(bytes_.data()); }
    uint32_t count() const noexcept { return count_; }
    bool ready() const noexcept { return ready_; }
private:
    alignas(MIDIPacketList) std::array<uint8_t,packetBytes> bytes_{};
    uint32_t count_=0;bool ready_=false,delivering_=false;
};
} // namespace ravet::au
