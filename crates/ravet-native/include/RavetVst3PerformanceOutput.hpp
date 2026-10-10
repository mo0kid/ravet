#pragma once
// Optional external-SDK C++17 generated-performance translation and delivery.
#include "RavetNativeEvent.h"
#include "RavetNativeScalars.hpp"
#include "pluginterfaces/vst/ivstevents.h"
#include <array>
#include <cstddef>
#include <cstring>
namespace ravet::vst3 {
namespace performance_output_detail {
inline bool velocity(const double &value, float &output) {
    if (!native::scalar::finite(value) || value < 0. || value > 1.) return false;
    const float candidate = float(value);
    if (!native::scalar::finite(candidate) || double(candidate) != value) return false;
    output = candidate; return true;
}
inline bool noteId(uint32_t input, Steinberg::int32 &output) {
    if (input == UINT32_MAX) return false; // SDK -1 is absent, not an identity.
    static_assert(sizeof(output) == sizeof(input));
    std::memcpy(&output, &input, sizeof(input)); return true;
}
inline bool volumeScaleIsExact(const double &value) {
    uint64_t bits = 0; std::memcpy(&bits, &value, sizeof(bits));
    const auto exponent = (bits >> 52) & 0x7ff;
    // Division by four loses two low significand bits for subnormals and
    // exponent1, or one for exponent2. Integer checks survive fast-math's
    // algebraic cancellation of (value/4)*4.
    return exponent >= 3 || (bits & (exponent == 2 ? 1u : 3u)) == 0;
}
}
// Supported canonical native records only. Concrete notes retain exactly widened
// f32 velocities and optional int32-bit IDs. Scoped Off/pressure require concrete
// channel+key and optional ID; standard expression output requires ID-only scope.
// No wildcard/ID/precision loss: UINT32_MAX concrete ID and non-f32 pressure reject.
// Volume/4 and tuning/240+.5 are explicit SDK normalized-unit mappings; tuning's
// floating arithmetic need not roundtrip exact physical bits. No inferred voice
// completion, choke, MIDI/UMP/SysEx/global/scoped parameter or text/int conversion.
// Header flags/ppq/optional note length/tuning default to canonical zero. Failed
// conversion leaves output unchanged; foreign record lifetime/alignment is owned
// by the caller. No native ABI layout or event selector changes.
inline bool nativePerformanceOutput(const RavetNativeEventV1 &input,
    Steinberg::int32 selectedBus, Steinberg::int32 frames, Steinberg::Vst::Event &output) {
    using E = Steinberg::Vst::Event;
    using namespace performance_output_detail;
    if (selectedBus < 0 || frames < 0 || frames > 65536 ||
        (frames == 0 ? input.offset != 0 : input.offset >= uint32_t(frames)) ||
        input.parameter_id || input.channel > 15 || input.key > 127 ||
        !native::scalar::finite(input.value)) return false;
    for (const auto word : input.words) if (word) return false;
    E candidate{}; candidate.busIndex = selectedBus; candidate.sampleOffset = Steinberg::int32(input.offset);
    const bool on = input.kind == RAVET_NATIVE_NOTE_ON || input.kind == RAVET_NATIVE_NOTE_ON_WITH_ID;
    const bool off = input.kind == RAVET_NATIVE_NOTE_OFF || input.kind == RAVET_NATIVE_NOTE_OFF_WITH_ID || input.kind == RAVET_NATIVE_NOTE_OFF_SCOPED;
    if (on || off) {
        if (input.detail) return false;
        const bool scoped = input.kind == RAVET_NATIVE_NOTE_OFF_SCOPED;
        const bool identified = input.kind == RAVET_NATIVE_NOTE_ON_WITH_ID || input.kind == RAVET_NATIVE_NOTE_OFF_WITH_ID || (scoped && (input.scope & RAVET_NATIVE_SCOPE_ID));
        if (scoped ? (input.scope != 3 && input.scope != 7) : input.scope != 0) return false;
        if (!identified && input.note_id) return false;
        Steinberg::int32 id = -1; if (identified && !noteId(input.note_id, id)) return false;
        float value = 0.f; if (!velocity(input.value, value)) return false;
        if (on) { candidate.type = E::kNoteOnEvent; candidate.noteOn = {Steinberg::int16(input.channel), Steinberg::int16(input.key), 0.f, value, 0, id}; }
        else { candidate.type = E::kNoteOffEvent; candidate.noteOff = {Steinberg::int16(input.channel), Steinberg::int16(input.key), value, id, 0.f}; }
    } else if (input.kind == RAVET_NATIVE_NOTE_EXPRESSION) {
        if (input.detail == 6) {
            if (input.scope != 3 && input.scope != 7) return false;
            const bool identified = input.scope == 7;
            if (!identified && input.note_id) return false;
            Steinberg::int32 id = -1; if (identified && !noteId(input.note_id, id)) return false;
            float value = 0.f; if (!velocity(input.value, value)) return false;
            candidate.type = E::kPolyPressureEvent;
            candidate.polyPressure = {Steinberg::int16(input.channel), Steinberg::int16(input.key), value, id};
        } else {
            if (input.scope != 4 || input.channel || input.key || input.detail > 5) return false;
            Steinberg::int32 id = -1; if (!noteId(input.note_id, id)) return false;
            double normalized = input.value;
            if (input.detail == 0) {
                if (normalized < 0. || normalized > 4. || !volumeScaleIsExact(input.value)) return false;
                normalized /= 4.;
            }
            else if (input.detail == 2) { if (normalized < -120. || normalized > 120.) return false; normalized = normalized / 240. + .5; }
            else if (normalized < 0. || normalized > 1.) return false;
            candidate.type = E::kNoteExpressionValueEvent;
            candidate.noteExpressionValue = {input.detail, id, normalized};
        }
    } else return false;
    output = candidate; return true;
}
enum class PerformanceOutputRejection { None, Unrepresentable, HostRejected };
struct PerformanceOutputDelivery {
    size_t delivered, undelivered;
    PerformanceOutputRejection reason;
};
// Whole representability/timing/order/capacity preflight before ANY addEvent.
// <=1024 records, <=65536 frames, globally ordered ties retained. Stack scratch
// is1024 SDK events. Native input storage is never passed to a mutable host call.
// Host refusal retains the accepted prefix and rendered DSP/history, discards the
// suffix and reports its counts; no rollback, replay or retry is attempted.
// Null host accepts an empty stream only. Caller supplies live input storage and
// serialized host callbacks complying with its own thread/retention contract.
inline PerformanceOutputDelivery deliverNativePerformanceOutput(
    const RavetNativeEventV1 *input, size_t count, Steinberg::Vst::IEventList *host,
    Steinberg::int32 selectedBus, Steinberg::int32 frames) {
    const PerformanceOutputDelivery rejected{0, count, PerformanceOutputRejection::Unrepresentable};
    if (selectedBus < 0 || frames < 0 || frames > 65536 || count > 1024 ||
        (count && (!input || !host))) return rejected;
    std::array<Steinberg::Vst::Event, 1024> mapped;
    uint32_t previous = 0;
    for (size_t index = 0; index < count; ++index) {
        if (input[index].offset < previous || !nativePerformanceOutput(input[index], selectedBus, frames, mapped[index])) return rejected;
        previous = input[index].offset;
    }
    for (size_t index = 0; index < count; ++index) {
        if (host->addEvent(mapped[index]) != Steinberg::kResultOk) return {index, count - index, PerformanceOutputRejection::HostRejected};
    }
    return {count, 0, PerformanceOutputRejection::None};
}
} // namespace ravet::vst3
