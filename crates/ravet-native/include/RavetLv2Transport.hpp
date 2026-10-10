#pragma once
#include "RavetNativeTransport.h"
#include "RavetNativeScalars.hpp"
#include "lv2/atom/atom.h"
#include <array>
#include <cmath>
#include <cstring>
#include <limits>
namespace ravet::native {
// Cache these exact URIs once on stopped control. All must be distinct/nonzero;
// collection never calls urid:map. Long frame, Double beat, Float BPM/speed/
// beatsPerBar and Int beatUnit are the selected exact Atom encoding profile.
struct Lv2TimePositionTypes {
    uint32_t sequence, frameTime, object, position, longType, doubleType, floatType,
        intType, frame, beat, tempo, speed, numerator, denominator;
    bool valid() const noexcept {
        const std::array<uint32_t,14> ids{sequence,frameTime,object,position,longType,doubleType,
            floatType,intType,frame,beat,tempo,speed,numerator,denominator};
        for (size_t i=0;i<ids.size();++i) {
            if (!ids[i]) return false;
            for (size_t j=0;j<i;++j) if (ids[i]==ids[j]) return false;
        }
        return true;
    }
};
class Lv2TimePosition final {
public:
    static constexpr uint32_t maximumBodyBytes=65536, maximumEvents=1024, maximumProperties=128;
    // Whole sequence/time object preflight before publication; errors retain
    // output. Foreign 8-byte-aligned storage must cover the declared body and
    // remain live/disjoint from output. Default/audio-frame units only. Known
    // fields require context0/exact encoding; unknown values remain opaque but
    // bounded, without recursive validation. Duplicate property keys reject.
    // At most one offset0 Position is published. Later positions are validated
    // then omitted, never extrapolated; no previous-block context is retained.
    // Speed0/1 means known stopped/playing; other speeds reject, never guess.
    // Missing fields stay unknown. Signature requires both valid properties.
    // Complete signature requires native DETAILS; its recording/loop/pre-roll
    // false flags are unknown/default placeholders, never host-known false. No
    // bar/loops/recording/pre-roll/clock or host/deadline inference.
    // Bounded local storage, no heap/locks/map callbacks/I/O.
    static bool collect(const LV2_Atom_Sequence *sequence, const Lv2TimePositionTypes &types,
                        uint32_t frames, RavetNativeTransportV1 &output) {
        if (!sequence || reinterpret_cast<uintptr_t>(sequence)%8 || !types.valid() || frames>65536) return false;
        LV2_Atom_Sequence header{}; std::memcpy(&header,sequence,sizeof(header));
        if (header.atom.type!=types.sequence || header.atom.size<sizeof(header.body) ||
            header.atom.size>maximumBodyBytes || (header.body.unit && header.body.unit!=types.frameTime) || header.body.pad) return false;
        const uint64_t total=sizeof(LV2_Atom)+uint64_t(header.atom.size);
        if (total>std::numeric_limits<uintptr_t>::max()-reinterpret_cast<uintptr_t>(sequence)) return false;
        const auto *bytes=reinterpret_cast<const uint8_t *>(sequence);
        uint64_t cursor=sizeof(header); uint32_t events=0, previous=0; bool blockStart=false;
        RavetNativeTransportV1 candidate{};
        while(cursor<total) {
            if (++events>maximumEvents || total-cursor<sizeof(LV2_Atom_Event)) return false;
            LV2_Atom_Event event{}; std::memcpy(&event,bytes+cursor,sizeof(event));
            const uint64_t padded=(uint64_t(event.body.size)+7)&~uint64_t(7);
            if (padded>total-cursor-sizeof(event) || event.time.frames<0 ||
                (frames?uint64_t(event.time.frames)>=frames:event.time.frames!=0)) return false;
            const auto offset=static_cast<uint32_t>(event.time.frames);
            if(events>1 && offset<previous) return false;
            previous=offset;
            if(event.body.type==types.object) {
                if(event.body.size<sizeof(LV2_Atom_Object_Body)) return false;
                LV2_Atom_Object_Body body{}; std::memcpy(&body,bytes+cursor+sizeof(event),sizeof(body));
                if(body.otype==types.position) {
                    RavetNativeTransportV1 position{};
                    if(!object(bytes+cursor+sizeof(event),event.body.size,types,position)) return false;
                    if(!offset) {
                        if(blockStart) return false;
                        blockStart=true;candidate=position;
                    }
                }
            }
            cursor+=sizeof(event)+padded;
        }
        if(cursor!=total) return false;
        output=candidate;return true;
    }
private:
    static bool object(const uint8_t *bytes,uint32_t size,const Lv2TimePositionTypes &types,
                       RavetNativeTransportV1 &output) {
        using ::ravet::native::scalar::finite;
        uint64_t cursor=sizeof(LV2_Atom_Object_Body);uint32_t count=0;
        std::array<uint32_t,maximumProperties> keys{};
        bool numerator=false,denominator=false;
        while(cursor<size) {
            if(count==maximumProperties || size-cursor<sizeof(LV2_Atom_Property_Body)) return false;
            LV2_Atom_Property_Body property{};std::memcpy(&property,bytes+cursor,sizeof(property));
            const uint64_t padded=(uint64_t(property.value.size)+7)&~uint64_t(7);
            if(!property.key || !property.value.type || padded>size-cursor-sizeof(property)) return false;
            for(uint32_t i=0;i<count;++i) if(keys[i]==property.key) return false;
            keys[count++]=property.key;
            const auto *value=bytes+cursor+sizeof(property);
            const bool known=property.key==types.frame || property.key==types.beat || property.key==types.tempo ||
                property.key==types.speed || property.key==types.numerator || property.key==types.denominator;
            if(known && property.context) return false;
            if(property.key==types.frame) {
                if(property.value.type!=types.longType || property.value.size!=sizeof(int64_t)) return false;
                std::memcpy(&output.sample_position,value,sizeof(int64_t));output.present|=RAVET_TRANSPORT_SAMPLES;
            } else if(property.key==types.beat) {
                if(property.value.type!=types.doubleType || property.value.size!=sizeof(double)) return false;
                double beat;std::memcpy(&beat,value,sizeof(beat));if(!finite(beat)) return false;
                output.beat_position=beat;output.present|=RAVET_TRANSPORT_BEATS;
            } else if(property.key==types.tempo || property.key==types.speed || property.key==types.numerator) {
                if(property.value.type!=types.floatType || property.value.size!=sizeof(float)) return false;
                float scalar;std::memcpy(&scalar,value,sizeof(scalar));if(!finite(scalar)) return false;
                if(property.key==types.tempo) {
                    if(scalar<=0.f) return false;
                    output.tempo=static_cast<double>(scalar);output.present|=RAVET_TRANSPORT_TEMPO;
                } else if(property.key==types.speed) {
                    if(scalar!=0.f && scalar!=1.f) return false;
                    output.present|=RAVET_TRANSPORT_PLAYING;
                    if(scalar==1.f) output.flags|=RAVET_TRANSPORT_FLAG_PLAYING;
                } else {
                    if(scalar<1.f || scalar>65535.f || std::trunc(scalar)!=scalar) return false;
                    output.signature_numerator=static_cast<uint32_t>(scalar);numerator=true;
                }
            } else if(property.key==types.denominator) {
                if(property.value.type!=types.intType || property.value.size!=sizeof(int32_t)) return false;
                int32_t scalar;std::memcpy(&scalar,value,sizeof(scalar));if(scalar<1 || scalar>65535) return false;
                output.signature_denominator=static_cast<uint32_t>(scalar);denominator=true;
            }
            cursor+=sizeof(property)+padded;
        }
        if(cursor!=size) return false;
        if(numerator && denominator) output.present|=RAVET_TRANSPORT_DETAILS|RAVET_TRANSPORT_SIGNATURE;
        else output.signature_numerator=output.signature_denominator=0;
        return true;
    }
};
} // namespace ravet::native
