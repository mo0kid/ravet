#ifndef RAVET_VST3_EVENT_CODEC_HPP
#define RAVET_VST3_EVENT_CODEC_HPP
#include "EventWire.hpp"

namespace ravet::vst3::processing_draft {
struct NativePayload {
    const uint8_t* bytes=nullptr;
    uint32_t size=0;
    bool text=false;
};
inline bool isText(uint16_t type) {
    using Steinberg::Vst::Event;
    return type==Event::kNoteExpressionTextEvent||type==Event::kChordEvent||type==Event::kScaleEvent;
}
// Scalar extraction performs no foreign payload read. The list separately checks
// the counted range, SDK UTF-16 terminator and complete record before copying.
inline bool extract(const Steinberg::Vst::Event& event,Record& record,NativePayload& payload) {
    using namespace Steinberg::Vst;
    Record next{};next.struct_size=sizeof(Record);next.type=event.type;next.flags=event.flags;
    next.bus=uint32_t(event.busIndex);next.offset=uint32_t(event.sampleOffset);next.ppq_position=event.ppqPosition;
    NativePayload source{};
    switch(event.type){
    case Event::kNoteOnEvent:
        next.channel=event.noteOn.channel;next.pitch=event.noteOn.pitch;
        next.tuning=event.noteOn.tuning;next.velocity=event.noteOn.velocity;
        next.note_length=event.noteOn.length;next.note_id=event.noteOn.noteId;break;
    case Event::kNoteOffEvent:
        next.channel=event.noteOff.channel;next.pitch=event.noteOff.pitch;
        next.tuning=event.noteOff.tuning;next.velocity=event.noteOff.velocity;
        next.note_id=event.noteOff.noteId;break;
    case Event::kPolyPressureEvent:
        next.channel=event.polyPressure.channel;next.pitch=event.polyPressure.pitch;
        next.velocity=event.polyPressure.pressure;next.note_id=event.polyPressure.noteId;break;
    case Event::kNoteExpressionValueEvent:
        next.expression_id=event.noteExpressionValue.typeId;next.note_id=event.noteExpressionValue.noteId;
        next.value=event.noteExpressionValue.value;break;
    case Event::kNoteExpressionIntValueEvent:
        next.expression_id=event.noteExpressionIntValue.typeId;next.note_id=event.noteExpressionIntValue.noteId;
        next.integer_value=event.noteExpressionIntValue.value;break;
    case Event::kDataEvent:
        if(event.data.size>65536)return false;
        next.data_type=event.data.type;source={event.data.bytes,event.data.size,false};break;
    case Event::kNoteExpressionTextEvent:
        if(event.noteExpressionText.textLen>4096)return false;
        next.expression_id=event.noteExpressionText.typeId;next.note_id=event.noteExpressionText.noteId;
        source={reinterpret_cast<const uint8_t*>(event.noteExpressionText.text),event.noteExpressionText.textLen*2,true};break;
    case Event::kChordEvent:{
        if(event.chord.textLen>4096)return false;
        next.root=event.chord.root;next.bass=event.chord.bassNote;
        uint16_t mask=0;std::memcpy(&mask,&event.chord.mask,2);next.mask=mask;
        source={reinterpret_cast<const uint8_t*>(event.chord.text),uint32_t(event.chord.textLen)*2,true};break;
    }
    case Event::kScaleEvent:{
        if(event.scale.textLen>4096)return false;
        next.root=event.scale.root;uint16_t mask=0;std::memcpy(&mask,&event.scale.mask,2);next.mask=mask;
        source={reinterpret_cast<const uint8_t*>(event.scale.text),uint32_t(event.scale.textLen)*2,true};break;
    }
    case Event::kLegacyMIDICCOutEvent:
        next.controller=event.midiCCOut.controlNumber;next.channel=event.midiCCOut.channel;
        next.midi_value=uint32_t(event.midiCCOut.value);next.midi_value2=uint32_t(event.midiCCOut.value2);break;
    default:return false;
    }
    next.payload_bytes=source.size;record=next;payload=source;return true;
}
// Construct an SDK event only from admitted records and aligned pool-owned bytes.
// No pointer to the caller's original wire/payload storage escapes through SDK.
inline Steinberg::Vst::Event sdkEvent(const Record& r,const uint8_t* bytes) {
    using namespace Steinberg::Vst;
    static_assert(sizeof(TChar)==2);
    Event event{};event.type=r.type;event.flags=r.flags;event.busIndex=Steinberg::int32(r.bus);
    event.sampleOffset=Steinberg::int32(r.offset);event.ppqPosition=r.ppq_position;
    const auto* text=reinterpret_cast<const TChar*>(bytes+r.payload_offset);
    switch(r.type){
    case Event::kNoteOnEvent:
        event.noteOn={Steinberg::int16(r.channel),Steinberg::int16(r.pitch),r.tuning,r.velocity,r.note_length,r.note_id};break;
    case Event::kNoteOffEvent:
        event.noteOff={Steinberg::int16(r.channel),Steinberg::int16(r.pitch),r.velocity,r.note_id,r.tuning};break;
    case Event::kPolyPressureEvent:
        event.polyPressure={Steinberg::int16(r.channel),Steinberg::int16(r.pitch),r.velocity,r.note_id};break;
    case Event::kNoteExpressionValueEvent:
        event.noteExpressionValue={r.expression_id,r.note_id,r.value};break;
    case Event::kNoteExpressionIntValueEvent:
        event.noteExpressionIntValue={r.expression_id,r.note_id,r.integer_value};break;
    case Event::kDataEvent:
        event.data={r.payload_bytes,r.data_type,r.payload_bytes?bytes+r.payload_offset:nullptr};break;
    case Event::kNoteExpressionTextEvent:
        event.noteExpressionText={r.expression_id,r.note_id,r.payload_bytes/2,text};break;
    case Event::kChordEvent:{
        Steinberg::int16 mask=0;const uint16_t bits=uint16_t(r.mask);std::memcpy(&mask,&bits,2);
        event.chord={Steinberg::int16(r.root),Steinberg::int16(r.bass),mask,uint16_t(r.payload_bytes/2),text};break;
    }
    case Event::kScaleEvent:{
        Steinberg::int16 mask=0;const uint16_t bits=uint16_t(r.mask);std::memcpy(&mask,&bits,2);
        event.scale={Steinberg::int16(r.root),mask,uint16_t(r.payload_bytes/2),text};break;
    }
    case Event::kLegacyMIDICCOutEvent:
        event.midiCCOut={uint8_t(r.controller),Steinberg::int8(r.channel),Steinberg::int8(r.midi_value),Steinberg::int8(r.midi_value2)};break;
    }
    return event;
}
inline void storePayload(const Record& source,const uint8_t* bytes,uint8_t* destination,
        uint32_t& used,Record& stored) {
    const bool text=isText(source.type);
    const uint32_t offset=text?(used+1u)&~1u:used;
    stored=source;stored.payload_offset=offset;
    // The complete used prefix is published; reused storage must not expose a
    // byte from an earlier phase through UTF-16 alignment padding.
    if(offset>used)destination[used]=0;
    if(source.payload_bytes)std::memcpy(destination+offset,bytes+source.payload_offset,source.payload_bytes);
    if(text){destination[offset+source.payload_bytes]=0;destination[offset+source.payload_bytes+1]=0;}
    used=offset+source.payload_bytes+(text?2u:0u);
}
}
#endif
