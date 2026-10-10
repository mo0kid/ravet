#pragma once
#include "RavetAuTransport.hpp"
#include "pluginterfaces/vst/ivstprocesscontext.h"

namespace ravet::au {
// Project a successfully collected AU snapshot into an external SDK context.
// The native AU snapshot retains independently available callbacks. A nonnull
// VST3 context implies known playing and sample coordinates, so incomplete
// snapshots produce no context, deliberately omitting partial musical data.
// Discontinuity and exact per-callback availability have no VST3 representation.
// Recording survives only from Transport2. The collector is the preflight;
// snapshot exposes only read access to its validated scalar fields.
// No host-time/system clock, bar, sample-loop or pre-roll inference; no heap.
inline Steinberg::Vst::ProcessContext *vst3Transport(
    const NativeAuTransportSnapshot &snapshot, double sampleRate,
    Steinberg::Vst::ProcessContext &output) {
    using C = Steinberg::Vst::ProcessContext;
    using ::ravet::native::scalar::finite;
    const auto &t = snapshot.transport();
    constexpr auto required = RAVET_TRANSPORT_PLAYING | RAVET_TRANSPORT_SAMPLES;
    if ((t.present & required) != required || !finite(sampleRate) || sampleRate <= 0.) return nullptr;
    C candidate{};
    candidate.sampleRate = sampleRate;
    candidate.projectTimeSamples = t.sample_position;
    if (t.flags & RAVET_TRANSPORT_FLAG_PLAYING) candidate.state |= C::kPlaying;
    if ((snapshot.callbacks() & Transport2) && (t.flags & RAVET_TRANSPORT_FLAG_RECORDING)) candidate.state |= C::kRecording;
    if (t.flags & RAVET_TRANSPORT_FLAG_LOOP_ACTIVE) candidate.state |= C::kCycleActive;
    if (t.present & RAVET_TRANSPORT_BEATS) {
        candidate.state |= C::kProjectTimeMusicValid;
        candidate.projectTimeMusic = t.beat_position;
    }
    if (t.present & RAVET_TRANSPORT_TEMPO) {
        candidate.state |= C::kTempoValid; candidate.tempo = t.tempo;
    }
    if (t.present & RAVET_TRANSPORT_SIGNATURE) {
        candidate.state |= C::kTimeSigValid;
        candidate.timeSigNumerator = static_cast<int32_t>(t.signature_numerator);
        candidate.timeSigDenominator = static_cast<int32_t>(t.signature_denominator);
    }
    if (t.present & RAVET_TRANSPORT_BEAT_LOOP) {
        candidate.state |= C::kCycleValid;
        candidate.cycleStartMusic = t.beat_loop_start;
        candidate.cycleEndMusic = t.beat_loop_end;
    }
    output = candidate;
    return &output;
}
} // namespace ravet::au
