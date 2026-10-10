#ifndef RAVET_EMBEDDED_GAIN_H
#define RAVET_EMBEDDED_GAIN_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Reference ABI only. Fields are opaque. Exclusively own state and callback
   buffers; stop callbacks before init. Gain 0..4 is immediate, without smoothing.
   Layout/float ABI must match the Rust target. No heap or device driver. */
typedef struct RavetEmbeddedGain {
    double sample_rate;
    uint32_t maximum_frames;
    float gain;
} RavetEmbeddedGain;
int32_t ravet_embedded_gain_init(RavetEmbeddedGain *state, double sample_rate, uint32_t maximum_frames);
int32_t ravet_embedded_gain_set(RavetEmbeddedGain *state, float gain);
/* Stereo f32 lengths <=128 samples, frames <=prepared maximum (1..64).
   Zero-frame calls accept null audio pointers with zero lengths. Input/output
   may alias; neither overlaps state. Returns 0 or -1. On rejection valid output
   active prefix is zeroed, tails preserved; invalid output pointer/length is
   rejected without writes. Nonnull pointers must be aligned/live for lengths. */
int32_t ravet_embedded_gain_process(RavetEmbeddedGain *state, const float *input, size_t input_samples,
                                  float *output, size_t output_samples, size_t frames);
#ifdef __cplusplus
}
#endif
#endif
