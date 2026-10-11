#ifndef RAVET_VST3_EVENT_WIRE_HPP
#define RAVET_VST3_EVENT_WIRE_HPP
#include "RavetVst3Processing.h"
#include "BoundedParameterChanges.hpp"
#include "pluginterfaces/vst/ivstevents.h"
#include <array>
#include <cstring>

namespace ravet::vst3::processing_draft {
using Record=RavetVst3EventRecord;
using parameters_draft::finite;
// Zero means unselected; otherwise immutable actual prepared bus channel count.
using EventBuses=std::array<uint8_t,16>;
inline bool finiteFloat(const float& value) {
    uint32_t bits=0;std::memcpy(&bits,&value,4);
    volatile uint32_t observed=bits;
    return (observed&0x7f800000u)!=0x7f800000u;
}
inline bool unit(const float& value) {return finiteFloat(value)&&value>=0&&value<=1;}
inline bool utf16(const uint8_t* bytes,uint32_t count) {
    if(count%2||count>8192)return false;
    for(uint32_t at=0;at<count;at+=2){
        uint16_t word=0;std::memcpy(&word,bytes+at,2);
        if(!word)return false;
        if(word>=0xd800&&word<=0xdbff){
            if(at+2==count)return false;
            uint16_t low=0;std::memcpy(&low,bytes+at+2,2);
            if(low<0xdc00||low>0xdfff)return false;
            at+=2;
        }else if(word>=0xdc00&&word<=0xdfff)return false;
    }
    return true;
}
// Pure admission: reads only caller-validated record/payload ranges and copied
// profile, never mutates a pool, output, history or count. UTF-16 bytes are native
// endian, matching the existing ABI scalars; byte offsets need no source alignment.
inline bool validate(const Record& record,const uint8_t* payload,uint32_t bytes,
        uint32_t frames,const EventBuses& buses,bool output) {
    using namespace Steinberg::Vst;
    if(record.struct_size!=sizeof(Record)||record.reserved||frames>65536||
       record.bus>=buses.size()||!buses[record.bus]||buses[record.bus]>16||
       (record.flags&~uint16_t(Event::kIsLive|Event::kUserReserved1|Event::kUserReserved2))||
       !finite(record.ppq_position)||(frames?record.offset>=frames:record.offset!=0)||
       bytes>65536||(bytes&&!payload)||record.payload_offset>bytes||
       record.payload_bytes>bytes-record.payload_offset)return false;
    const bool channel=record.channel>=0&&record.channel<buses[record.bus];
    const bool pitch=record.pitch>=0&&record.pitch<=127;
    const bool note=output||record.note_id>=-1;
    const bool expression=output||record.note_id>=0;
    const bool noPayload=record.payload_bytes==0;
    switch(record.type){
    case Event::kNoteOnEvent:
        return channel&&pitch&&note&&record.note_length>=0&&
            finiteFloat(record.tuning)&&unit(record.velocity)&&noPayload;
    case Event::kNoteOffEvent:
        return channel&&pitch&&note&&finiteFloat(record.tuning)&&unit(record.velocity)&&noPayload;
    case Event::kPolyPressureEvent:
        return channel&&pitch&&note&&unit(record.velocity)&&noPayload;
    case Event::kNoteExpressionValueEvent:
        return expression&&record.expression_id!=UINT32_MAX&&finite(record.value)&&
            record.value>=0&&record.value<=1&&noPayload;
    case Event::kNoteExpressionIntValueEvent:
        return expression&&record.expression_id!=UINT32_MAX&&noPayload;
    case Event::kDataEvent:
        return record.data_type==DataEvent::kMidiSysEx;
    case Event::kNoteExpressionTextEvent:
        return expression&&record.expression_id!=UINT32_MAX&&
            utf16(record.payload_bytes?payload+record.payload_offset:nullptr,record.payload_bytes);
    case Event::kChordEvent:
        return record.root>=0&&record.root<=127&&record.bass>=0&&record.bass<=127&&
            record.mask<=UINT16_MAX&&
            utf16(record.payload_bytes?payload+record.payload_offset:nullptr,record.payload_bytes);
    case Event::kScaleEvent:
        return record.root>=0&&record.root<=127&&record.mask<=UINT16_MAX&&
            utf16(record.payload_bytes?payload+record.payload_offset:nullptr,record.payload_bytes);
    case Event::kLegacyMIDICCOutEvent:
        return output&&channel&&record.controller<=255&&record.midi_value<=127&&
            record.midi_value2<=127&&noPayload;
    default:return false;
    }
}
// Validated destination budget including aligned native UTF-16 termination.
// Caller preflights every record before starting any destination write.
inline bool storedPayloadEnd(const Record& record,uint32_t used,uint32_t& result) {
    using namespace Steinberg::Vst;
    const bool text=record.type==Event::kNoteExpressionTextEvent||
        record.type==Event::kChordEvent||record.type==Event::kScaleEvent;
    if(used>65536)return false;
    const uint32_t aligned=text?(used+1u)&~1u:used;
    const uint32_t terminator=text?2u:0u;
    if(aligned>65536||terminator>65536-aligned||
       record.payload_bytes>65536-aligned-terminator)return false;
    result=aligned+record.payload_bytes+terminator;return true;
}
} // namespace ravet::vst3::processing_draft
#endif
