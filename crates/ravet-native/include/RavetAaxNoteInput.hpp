#pragma once
#include "AAX.h"
#include "RavetNativeEvent.h"
#include <array>
#include <cstring>
#include <cstdint>

namespace ravet::aax {
// Borrowed aligned stream/packets and output/count must be live for their declared
// capacities and disjoint. No SDK calls, pointer provenance or SysEx reassembly.
// Whole raw stream validates before publication. Explicit compile-time policy
// chooses legacy notes or canonical raw defined short channel/system messages.
class AaxShortMidiPreflight final {
    static bool overlap(uintptr_t a,uintptr_t b,uintptr_t c,uintptr_t d){return a<d&&c<b;}
    static bool extent(const void *p,size_t bytes,size_t alignment,uintptr_t &a,uintptr_t &b){a=reinterpret_cast<uintptr_t>(p);if(!a||a%alignment||bytes>UINTPTR_MAX-a)return false;b=a+bytes;return true;}
public:
    static constexpr uint32_t maximumPackets=1024;
    template<bool RawShort> static bool convert(const AAX_CMidiStream *stream,uint32_t frames,RavetNativeEventV1 *output,uint32_t capacity,uint32_t &count){
        if(frames>1024||capacity>maximumPackets)return false;
        uintptr_t first=0,last=0,packetsFirst=0,packetsLast=0,outFirst=0,outLast=0,countFirst=0,countLast=0;
        if(!extent(stream,sizeof(*stream),alignof(AAX_CMidiStream),first,last)||!extent(&count,sizeof(count),alignof(uint32_t),countFirst,countLast))return false;
        AAX_CMidiStream header{};std::memcpy(&header,stream,sizeof(header));
        if(header.mBufferSize>maximumPackets)return false;
        if(header.mBufferSize&&!extent(header.mBuffer,sizeof(AAX_CMidiPacket)*size_t(header.mBufferSize),alignof(AAX_CMidiPacket),packetsFirst,packetsLast))return false;
        if(capacity&&!extent(output,sizeof(RavetNativeEventV1)*size_t(capacity),alignof(RavetNativeEventV1),outFirst,outLast))return false;
        if(overlap(first,last,packetsFirst,packetsLast)||overlap(first,last,countFirst,countLast)||overlap(packetsFirst,packetsLast,countFirst,countLast)||overlap(outFirst,outLast,countFirst,countLast)||overlap(first,last,outFirst,outLast)||overlap(packetsFirst,packetsLast,outFirst,outLast))return false;
        std::array<RavetNativeEventV1,maximumPackets> staged{};uint32_t notes=0,previous=0;
        for(uint32_t i=0;i<header.mBufferSize;++i){
            AAX_CMidiPacket packet{};std::memcpy(&packet,header.mBuffer+i,sizeof(packet));
            // The installed SDK compatibility contract says this flag is not
            // currently set and timestamp0 identifies immediate packets. Accept
            // boolean1, retaining timestamp as the authoritative ordered offset;
            // do not infer a different delivery time or ignore malformed offsets.
            if((packet.mIsImmediate!=0&&packet.mIsImmediate!=1)||(frames?packet.mTimestamp>=frames:packet.mTimestamp!=0)||(i&&packet.mTimestamp<previous))return false;
            previous=packet.mTimestamp;
            if(!packet.mLength||packet.mLength>3)return false;
            const auto status=packet.mData[0];uint32_t length=0;
            if(status>=0x80&&status<0xf0)length=(status&0xe0)==0xc0?2:3;
            else if(status==0xf1||status==0xf3)length=2;
            else if(status==0xf2)length=3;
            else if(status==0xf6||(RawShort&&status==0xf7)||status==0xf8||status==0xfa||status==0xfb||status==0xfc||status==0xfe||status==0xff)length=1;
            if(packet.mLength!=length)return false;
            for(uint32_t b=1;b<length;++b)if(packet.mData[b]>=0x80)return false;
            if constexpr(RawShort){
                if(notes==capacity)return false;
                auto &message=staged[notes++];message.offset=packet.mTimestamp;
                message.kind=RAVET_NATIVE_MIDI;message.detail=length;
                for(uint32_t b=0;b<length;++b)message.words[0]|=uint32_t(packet.mData[b])<<(8*b);
            }else if((status&0xf0)==0x80||(status&0xf0)==0x90){
                if(notes==capacity)return false;
                auto &note=staged[notes++];note.offset=packet.mTimestamp;note.channel=status&15;note.key=packet.mData[1];
                note.value=double(float(packet.mData[2])/127.f);
                if((status&0xf0)==0x90&&packet.mData[2])note.kind=RAVET_NATIVE_NOTE_ON;
                else{note.kind=RAVET_NATIVE_NOTE_OFF_SCOPED;note.scope=RAVET_NATIVE_SCOPE_CHANNEL|RAVET_NATIVE_SCOPE_KEY;}
            }
        }
        for(uint32_t i=0;i<notes;++i)output[i]=staged[i];count=notes;return true;
    }
};
// Existing note-only policy: defined non-notes omit, standalone F7 rejects.
class AaxNoteInput final {
public:
    static constexpr uint32_t maximumPackets=AaxShortMidiPreflight::maximumPackets;
    static bool convert(const AAX_CMidiStream *stream,uint32_t frames,RavetNativeEventV1 *output,uint32_t capacity,uint32_t &count){
        return AaxShortMidiPreflight::convert<false>(stream,frames,output,capacity,count);
    }
};
} // namespace ravet::aax
