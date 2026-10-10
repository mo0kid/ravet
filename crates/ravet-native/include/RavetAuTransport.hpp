#pragma once
// Apple SDK declarations remain external; no SDK source is vendored.
#include <AudioToolbox/AudioToolbox.h>
#include "RavetNativeScalars.hpp"
#include "RavetNativeTransport.h"
#include <cmath>
#include <cstring>

namespace ravet::au {
namespace transport_detail {
// Initialize sentinels through their integer representation. Fast-math can
// discard a floating NaN initializer even though the host receives its address.
inline void invalidate(double &value) { const uint64_t bits = UINT64_C(0x7ff8000000000000); std::memcpy(&value, &bits, sizeof(value)); }
inline void invalidate(float &value) { const uint32_t bits = UINT32_C(0x7fc00000); std::memcpy(&value, &bits, sizeof(value)); }
} // namespace transport_detail
enum NativeAuTransportCallback : uint32_t {
    BeatAndTempo = 1, Signature = 2, LegacyTransport = 4, Transport2 = 8
};
// DETAILS groups native flags together. Callback availability distinguishes
// legacy recording (unknown) from Transport2 recording (known false/true).
// Signature-only DETAILS has unknown/default flags; callback kinds distinguish
// the fields actually supplied. Zero legacy recording and all pre-roll flags are
// unknown/default placeholders,
// not host-known false. Neither callback supplies native pre-roll/sample loops.
class NativeAuTransportSnapshot {
public:
    const RavetNativeTransportV1 &transport() const noexcept { return transport_; }
    uint32_t callbacks() const noexcept { return callbacks_; }
private:
    RavetNativeTransportV1 transport_{};
    uint32_t callbacks_ = 0;
    friend bool nativeTransport(const HostCallbackInfo &, NativeAuTransportSnapshot &);
};

// Collect at block start only within the host's serialized AudioUnitRender.
// Callback/user-data ownership and coherent values across callbacks are the
// host's responsibility. Each supplied callback is invoked at most once;
// Transport2 takes precedence, with no retry/fallback following its error.
// Missing/error tuples remain unavailable; successful independent tuples
// survive. Successful incomplete/malformed tuples reject atomically. This
// helper allocates no storage; arbitrary host callbacks may allocate or block.
// Sample conversion inherits Float64 precision, never rounds fractional data.
// No inferred clock/bar number, extrapolation, state reset or deadline promise.
inline bool nativeTransport(const HostCallbackInfo &host,
                            NativeAuTransportSnapshot &output) {
    using ::ravet::native::scalar::finite;
    NativeAuTransportSnapshot candidate{};
    auto &t = candidate.transport_;
    if (host.beatAndTempoProc) {
        Float64 beat, tempo;
        transport_detail::invalidate(beat); transport_detail::invalidate(tempo);
        if (host.beatAndTempoProc(host.hostUserData, &beat, &tempo) == noErr) {
            if (!finite(beat) || !finite(tempo) || tempo <= 0.) return false;
            candidate.callbacks_ |= BeatAndTempo;
            t.present |= RAVET_TRANSPORT_BEATS | RAVET_TRANSPORT_TEMPO;
            t.beat_position = beat; t.tempo = tempo;
        }
    }
    if (host.musicalTimeLocationProc) {
        Float32 numerator; transport_detail::invalidate(numerator);
        UInt32 denominator = 0;
        if (host.musicalTimeLocationProc(host.hostUserData, nullptr, &numerator,
                                       &denominator, nullptr) == noErr) {
            if (!finite(numerator) || numerator < 1.f || numerator > 65535.f ||
                std::trunc(numerator) != numerator || !denominator || denominator > 65535) return false;
            candidate.callbacks_ |= Signature;
            t.present |= RAVET_TRANSPORT_DETAILS | RAVET_TRANSPORT_SIGNATURE;
            t.signature_numerator = static_cast<uint32_t>(numerator);
            t.signature_denominator = denominator;
        }
    }
    if (host.transportStateProc2 || host.transportStateProc) {
        Boolean playing = 2, changed = 2, cycling = 2, recording = 2;
        Float64 samples, start, end;
        transport_detail::invalidate(samples); transport_detail::invalidate(start); transport_detail::invalidate(end);
        const bool modern = host.transportStateProc2 != nullptr;
        const auto status = modern ? host.transportStateProc2(host.hostUserData, &playing,
            &recording, &changed, &samples, &cycling, &start, &end) :
            host.transportStateProc(host.hostUserData, &playing, &changed, &samples,
                                    &cycling, &start, &end);
        if (status == noErr) {
            // +2^63 is the first nonrepresentable positive Float64 sample;
            // testing against INT64_MAX converted to double would accept it.
            constexpr double limit = 0x1p63;
            if (playing > 1 || changed > 1 || cycling > 1 || (modern && recording > 1) ||
                !finite(samples) || samples < -limit || samples >= limit ||
                std::trunc(samples) != samples) return false;
            if (cycling && (!finite(start) || !finite(end) || start >= end)) return false;
            candidate.callbacks_ |= modern ? Transport2 : LegacyTransport;
            t.present |= RAVET_TRANSPORT_PLAYING | RAVET_TRANSPORT_SAMPLES | RAVET_TRANSPORT_DETAILS;
            t.sample_position = static_cast<int64_t>(samples);
            if (playing) t.flags |= RAVET_TRANSPORT_FLAG_PLAYING;
            if (changed) t.flags |= RAVET_TRANSPORT_FLAG_DISCONTINUITY;
            if (modern && recording) t.flags |= RAVET_TRANSPORT_FLAG_RECORDING;
            if (cycling) {
                t.flags |= RAVET_TRANSPORT_FLAG_LOOP_ACTIVE;
                if (t.present & RAVET_TRANSPORT_BEATS) {
                    t.present |= RAVET_TRANSPORT_BEAT_LOOP;
                    t.beat_loop_start = start; t.beat_loop_end = end;
                }
            }
        }
    }
    output = candidate;
    return true;
}
} // namespace ravet::au
