#pragma once
// RAVET-owned external-SDK block-start mapping; no SDK source is vendored.
#include "RavetNativeScalars.hpp"
#include "RavetNativeTransport.h"
#include "pluginterfaces/vst/ivstprocesscontext.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
namespace ravet::vst3 {
// Immutable setup-time requirements for the fields this profile can represent.
// Always-valid sample position needs no optional request. Availability still
// depends on the host; no missing information is inferred.
inline constexpr uint32_t nativeTransportRequirements =
    Steinberg::Vst::IProcessContextRequirements::kNeedProjectTimeMusic |
    Steinberg::Vst::IProcessContextRequirements::kNeedCycleMusic |
    Steinberg::Vst::IProcessContextRequirements::kNeedTempo |
    Steinberg::Vst::IProcessContextRequirements::kNeedTimeSignature |
    Steinberg::Vst::IProcessContextRequirements::kNeedTransportState;

// Map a live/aligned SDK-owned optional context into canonical NativeTransportV1.
// Null means unknown (zero record), not stopped. Whole scalar validation before
// output writes; rejection preserves output. Exact i64/f64 timelines, no narrowing,
// extrapolation, rounding, guessed bar number or automatic discontinuity/reset.
// Known SDK flags select optional fields; unknown/unavailable fields are ignored.
// Signature must fit nonzero u16; finite positive tempo/rate, finite music/cycles.
// Active valid cycle bounds must be ordered. Inactive bounds (including zero
// extent) and bounds without a known musical position cannot be represented by
// native v1 and are omitted; recording/active-loop state still survives.
// SDK bar start has no exact bar number, so no native bar pair is invented.
// System/continuous time, frame rate/SMPTE, clock/chord, tempo increment, sample
// loops and pre-roll have no mapping in this profile. No callbacks/heap/locks/I/O.
inline bool nativeTransport(const Steinberg::Vst::ProcessContext *context,
                            RavetNativeTransportV1 &output) {
    using C = Steinberg::Vst::ProcessContext;
    using ::ravet::native::scalar::finite;
    RavetNativeTransportV1 mapped{};
    if (!context) { output = mapped; return true; }
    if (!finite(context->sampleRate) || context->sampleRate <= 0.) return false;
    mapped.present = RAVET_TRANSPORT_PLAYING | RAVET_TRANSPORT_SAMPLES | RAVET_TRANSPORT_DETAILS;
    mapped.sample_position = context->projectTimeSamples;
    if (context->state & C::kPlaying) mapped.flags |= RAVET_TRANSPORT_FLAG_PLAYING;
    if (context->state & C::kRecording) mapped.flags |= RAVET_TRANSPORT_FLAG_RECORDING;
    if (context->state & C::kCycleActive) mapped.flags |= RAVET_TRANSPORT_FLAG_LOOP_ACTIVE;
    if (context->state & C::kProjectTimeMusicValid) {
        if (!finite(context->projectTimeMusic)) return false;
        mapped.present |= RAVET_TRANSPORT_BEATS;
        mapped.beat_position = context->projectTimeMusic;
    }
    if (context->state & C::kTempoValid) {
        if (!finite(context->tempo) || context->tempo <= 0.) return false;
        mapped.present |= RAVET_TRANSPORT_TEMPO;
        mapped.tempo = context->tempo;
    }
    if (context->state & C::kTimeSigValid) {
        if (context->timeSigNumerator <= 0 || context->timeSigNumerator > 65535 ||
            context->timeSigDenominator <= 0 || context->timeSigDenominator > 65535) return false;
        mapped.present |= RAVET_TRANSPORT_SIGNATURE;
        mapped.signature_numerator = uint32_t(context->timeSigNumerator);
        mapped.signature_denominator = uint32_t(context->timeSigDenominator);
    }
    if (context->state & C::kCycleValid) {
        if (!finite(context->cycleStartMusic) || !finite(context->cycleEndMusic)) return false;
        if (context->state & C::kCycleActive) {
            if (context->cycleStartMusic >= context->cycleEndMusic) return false;
            if (mapped.present & RAVET_TRANSPORT_BEATS) {
                mapped.present |= RAVET_TRANSPORT_BEAT_LOOP;
                mapped.beat_loop_start = context->cycleStartMusic;
                mapped.beat_loop_end = context->cycleEndMusic;
            }
        }
    }
    output = mapped; return true;
}
} // namespace ravet::vst3
