#pragma once
// Optional external-SDK C++17 input translation; SDK headers stay consumer-owned.
#include "RavetNativeEvent.h"
#include "RavetNativeScalars.hpp"
#include "pluginterfaces/vst/ivstevents.h"
#include <array>
#include <cstddef>
#include <utility>
namespace ravet::vst3 {
enum class UnknownExpressionPolicy { Reject, Ignore };
enum class PerformanceInputResult { Rejected, Ignored, Mapped };
namespace performance_input_detail {
inline bool normalized(const float &value) { return native::scalar::finite(value) && value >= 0.f && value <= 1.f; }
inline bool normalized(const double &value) { return native::scalar::finite(value) && value >= 0. && value <= 1.; }
inline bool address(Steinberg::int16 channel, Steinberg::int16 pitch) { return channel >= 0 && channel < 16 && pitch >= 0 && pitch < 128; }
inline void target(RavetNativeEventV1 &output, Steinberg::int16 channel, Steinberg::int16 pitch, Steinberg::int32 id) {
    output.scope = RAVET_NATIVE_SCOPE_CHANNEL | RAVET_NATIVE_SCOPE_KEY;
    output.channel = uint32_t(channel); output.key = uint32_t(pitch);
    if (id != -1) { output.scope |= RAVET_NATIVE_SCOPE_ID; output.note_id = uint32_t(id); }
}
}
// One explicitly selected event bus. Negative SDK note IDs other than -1 retain
// their int32 bit identity in portable u32 (including the SDK plug-in range).
// -1 alone means absent. No voice matching, allocation or lifetime inference.
// Note tuning (cents) expands to a semitone expression, after On / before Off,
// only when nonzero; its default is zero. Optional length, ppqPosition and flags
// are not represented. Nonzero tuning must fit portable +/-120 semitones.
// Standard expression normalized values map volume*4 and tuning240*(x-.5);
// other supported values retain f64. Pressure maps from SDK poly pressure.
// Unknown expressions reject by default; Ignore still validates ID/value first.
// Other selected SDK types (including SysEx/text/int values) reject explicitly.
// All output fields/count remain unchanged on rejection or ignored events.
inline PerformanceInputResult nativePerformanceInput(
    const Steinberg::Vst::Event &input, Steinberg::int32 selectedBus,
    Steinberg::int32 frames, UnknownExpressionPolicy unknown,
    std::array<RavetNativeEventV1, 2> &output, size_t &count) {
    using E = Steinberg::Vst::Event;
    using namespace performance_input_detail;
    if (selectedBus < 0 || frames < 0 || frames > 65536 ||
        input.busIndex < 0 || input.sampleOffset < 0 ||
        (frames == 0 ? input.sampleOffset != 0 : input.sampleOffset >= frames) ||
        (unknown != UnknownExpressionPolicy::Reject && unknown != UnknownExpressionPolicy::Ignore)) return PerformanceInputResult::Rejected;
    if (input.busIndex != selectedBus) return PerformanceInputResult::Ignored;
    std::array<RavetNativeEventV1, 2> candidate{};
    auto &event = candidate[0]; event.offset = uint32_t(input.sampleOffset);
    size_t used = 1;
    if (input.type == E::kNoteOnEvent || input.type == E::kNoteOffEvent) {
        const bool on = input.type == E::kNoteOnEvent;
        const auto channel = on ? input.noteOn.channel : input.noteOff.channel;
        const auto pitch = on ? input.noteOn.pitch : input.noteOff.pitch;
        const auto id = on ? input.noteOn.noteId : input.noteOff.noteId;
        const float &velocity = on ? input.noteOn.velocity : input.noteOff.velocity;
        const float &tuning = on ? input.noteOn.tuning : input.noteOff.tuning;
        if (!address(channel, pitch) || !normalized(velocity) || !native::scalar::finite(tuning) || tuning < -12000.f || tuning > 12000.f) return PerformanceInputResult::Rejected;
        event.kind = on ? (id == -1 ? RAVET_NATIVE_NOTE_ON : RAVET_NATIVE_NOTE_ON_WITH_ID) : (id == -1 ? RAVET_NATIVE_NOTE_OFF : RAVET_NATIVE_NOTE_OFF_WITH_ID);
        event.channel = uint32_t(channel); event.key = uint32_t(pitch);
        if (id != -1) event.note_id = uint32_t(id);
        event.value = double(velocity);
        if (tuning != 0.f) {
            auto &expression = candidate[1]; expression.offset = event.offset;
            expression.kind = RAVET_NATIVE_NOTE_EXPRESSION; expression.detail = 2;
            target(expression, channel, pitch, id); expression.value = double(tuning) / 100.;
            used = 2; if (!on) std::swap(candidate[0], candidate[1]);
        }
    } else if (input.type == E::kPolyPressureEvent) {
        const auto &raw = input.polyPressure;
        if (!address(raw.channel, raw.pitch) || !normalized(raw.pressure)) return PerformanceInputResult::Rejected;
        event.kind = RAVET_NATIVE_NOTE_EXPRESSION; event.detail = 6;
        target(event, raw.channel, raw.pitch, raw.noteId); event.value = double(raw.pressure);
    } else if (input.type == E::kNoteExpressionValueEvent) {
        const auto &raw = input.noteExpressionValue;
        if (raw.noteId == -1 || !normalized(raw.value)) return PerformanceInputResult::Rejected;
        event.kind = RAVET_NATIVE_NOTE_EXPRESSION; event.scope = RAVET_NATIVE_SCOPE_ID;
        event.note_id = uint32_t(raw.noteId); event.value = raw.value;
        switch (raw.typeId) {
        case Steinberg::Vst::kVolumeTypeID: event.detail = 0; event.value *= 4.; break;
        case Steinberg::Vst::kPanTypeID: event.detail = 1; break;
        case Steinberg::Vst::kTuningTypeID: event.detail = 2; event.value = 240. * (event.value - .5); break;
        case Steinberg::Vst::kVibratoTypeID: event.detail = 3; break;
        case Steinberg::Vst::kExpressionTypeID: event.detail = 4; break;
        case Steinberg::Vst::kBrightnessTypeID: event.detail = 5; break;
        default: return unknown == UnknownExpressionPolicy::Ignore ? PerformanceInputResult::Ignored : PerformanceInputResult::Rejected;
        }
    } else return PerformanceInputResult::Rejected;
    output = candidate; count = used; return PerformanceInputResult::Mapped;
}
// Complete raw list preflight and translation before ANY visitor call. <=1024
// raw and expanded events, <=65536 frames; globally ordered offsets, ties retain
// list order and tuning expansion order. Each getEvent is called exactly once.
// Stack scratch is 56KiB plus one two-event candidate, with no heap/locks/I/O.
// Unselected buses filter only after header/timing/order validation. A visitor
// refusal can retain earlier visitor effects: stage privately and defer DSP,
// state publication and host delivery until the helper returns success.
// Foreign SDK callbacks must initialize successful outputs and obey consumer
// thread constraints. Null list is empty; zero-frame events must have offset0.
template <class Visitor>
bool visitValidatedPerformanceInput(Steinberg::Vst::IEventList *input,
    Steinberg::int32 selectedBus, Steinberg::int32 frames,
    UnknownExpressionPolicy unknown, Visitor &&visitor) {
    if (selectedBus < 0 || frames < 0 || frames > 65536 ||
        (unknown != UnknownExpressionPolicy::Reject && unknown != UnknownExpressionPolicy::Ignore)) return false;
    if (!input) return true;
    const auto rawCount = input->getEventCount(); if (rawCount < 0 || rawCount > 1024) return false;
    std::array<RavetNativeEventV1, 1024> events;
    size_t used = 0; Steinberg::int32 previous = 0;
    for (Steinberg::int32 index = 0; index < rawCount; ++index) {
        Steinberg::Vst::Event raw{};
        if (input->getEvent(index, raw) != Steinberg::kResultOk || raw.sampleOffset < previous) return false;
        previous = raw.sampleOffset;
        std::array<RavetNativeEventV1, 2> candidate{}; size_t count = 0;
        const auto result = nativePerformanceInput(raw, selectedBus, frames, unknown, candidate, count);
        if (result == PerformanceInputResult::Rejected || count > events.size() - used) return false;
        if (result == PerformanceInputResult::Mapped) for (size_t item = 0; item < count; ++item) events[used++] = candidate[item];
    }
    for (size_t index = 0; index < used; ++index) if (!visitor(events[index])) return false;
    return true;
}
} // namespace ravet::vst3
