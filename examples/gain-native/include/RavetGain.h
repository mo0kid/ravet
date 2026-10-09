#pragma once
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Native reference DSP only. SDKs/format ownership remain outside this ABI.
   Callbacks exclusively own the handle; prepare/reset/destroy after callbacks
   stop. Pointer ranges must be live/aligned and large enough for stated sizes.
   Inputs may alias outputs because input copying completes before output
   writes. Output channel ranges must not overlap. Result pointers must not overlap sample buffers. No control-thread access to
   live DSP. */
typedef struct RavetGain RavetGain;
typedef struct {
  uint32_t offset, id;
  float value;
} RavetGainEvent;
RavetGain *ravet_gain_create(void);
void ravet_gain_destroy(RavetGain *);
int32_t ravet_gain_prepare(RavetGain *, double rate, uint32_t channels,
                           uint32_t maximum_frames);
void ravet_gain_reset(RavetGain *);
int32_t ravet_gain_restore(RavetGain *, float gain, uint32_t bypass);
/* Returns 0 on success, 1 on validated rejection, 2 on contained panic.
   Valid output ranges are silenced on rejected blocks. Zero frames permits null
   sample pointers. At most 1,024 ordered parameter events; IDs 1=gain,
   2=bypass. Successful calls write the final requested gain/bypass to
   caller-owned outputs. */
int32_t ravet_gain_process(RavetGain *, const float *const *inputs,
                           float *const *outputs, uint32_t channels,
                           uint32_t frames, const RavetGainEvent *,
                           uint32_t event_count, float *requested_gain,
                           uint32_t *requested_bypass);
/* Separate native f64 handle, with identical parameter events/state semantics.
   Use only the matching 64-bit lifecycle/process functions for this handle.
   The sample buffers remain double; gain/bypass controls remain float/u32.
   All pointer, aliasing, capacity and result rules above also apply. */
typedef struct RavetGain64 RavetGain64;
RavetGain64 *ravet_gain64_create(void);
void ravet_gain64_destroy(RavetGain64 *);
int32_t ravet_gain64_prepare(RavetGain64 *, double rate, uint32_t channels,
                             uint32_t maximum_frames);
void ravet_gain64_reset(RavetGain64 *);
int32_t ravet_gain64_restore(RavetGain64 *, float gain, uint32_t bypass);
int32_t ravet_gain64_process(RavetGain64 *, const double *const *inputs,
                             double *const *outputs, uint32_t channels,
                             uint32_t frames, const RavetGainEvent *,
                             uint32_t event_count, float *requested_gain,
                             uint32_t *requested_bypass);
/* Pure control-thread state codec: fixed 32-byte complete state, legacy 20-byte
   decode also accepted. Decode validates before writing gain/bypass. */
int32_t ravet_gain_encode(float gain, uint32_t bypass, uint8_t *bytes,
                          size_t length);
int32_t ravet_gain_decode(const uint8_t *bytes, size_t length, float *gain,
                          uint32_t *bypass);
#ifdef __cplusplus
}
#endif
