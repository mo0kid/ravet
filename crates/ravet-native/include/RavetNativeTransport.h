#pragma once
#include <stdint.h>
/* In-process native v1 snapshot, not disk/wire serialization. No timeline
   extrapolation, fixed-point quantization or automatic DSP reset. Zero unused
   fields/reserved; used signed zero survives, unused doubles require +0. */
enum RavetNativeTransportPresenceV1 {
    RAVET_TRANSPORT_PLAYING = 1,
    RAVET_TRANSPORT_SAMPLES = 2,
    RAVET_TRANSPORT_BEATS = 4,
    RAVET_TRANSPORT_TEMPO = 8,
    RAVET_TRANSPORT_DETAILS = 16,
    RAVET_TRANSPORT_SIGNATURE = 32,
    RAVET_TRANSPORT_TEMPO_INCREMENT = 64,
    RAVET_TRANSPORT_BEAT_LOOP = 128,
    RAVET_TRANSPORT_SAMPLE_LOOP = 256,
    RAVET_TRANSPORT_BAR = 512
};
enum RavetNativeTransportFlagsV1 {
    RAVET_TRANSPORT_FLAG_PLAYING = 1,
    RAVET_TRANSPORT_FLAG_DISCONTINUITY = 2,
    RAVET_TRANSPORT_FLAG_RECORDING = 4,
    RAVET_TRANSPORT_FLAG_LOOP_ACTIVE = 8,
    RAVET_TRANSPORT_FLAG_PRE_ROLL = 16
};
/* Playing flag requires playing availability; details flags/fields require
   details availability. Tempo finite/positive, increment finite with tempo;
   signature nonzero u16 numerator/denominator; beats/bar finite, bar requires
   beats. Loop ranges require active loop and matching known timeline, finite
   beat endpoints and strictly increasing bounds. Unknown bits reject. */
typedef struct RavetNativeTransportV1 {
    uint32_t present, flags;
    int64_t sample_position;
    double beat_position, tempo, tempo_increment;
    double beat_loop_start, beat_loop_end;
    int64_t sample_loop_start, sample_loop_end;
    double bar_start;
    int32_t bar_number;
    uint32_t signature_numerator, signature_denominator, reserved;
} RavetNativeTransportV1;
