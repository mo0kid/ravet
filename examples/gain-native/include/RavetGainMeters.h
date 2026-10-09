#pragma once
#include "RavetGain.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Always available public reference meter ABI. Fixed layouts: reading 32 bytes,
   complete snapshot 128 bytes. Levels retain native f64 precision for the f64
   DSP and widen the f32 readings for the f32 DSP. Nonfinite samples count as
   silence/diagnostic in metering; the audio itself is not sanitized. */
typedef struct {
  double peak, rms;
  uint64_t samples;
  uint32_t nonfinite, reserved;
} RavetGainMeterReading;
typedef struct {
  RavetGainMeterReading input[2], output[2];
} RavetGainMeters;
/* Last successful nonempty block; unused mono channel is zero. Prepare/reset
   clear meters. Zero-frame/rejected blocks retain the previous snapshot.
   Copy only under the DSP handle's exclusive serialized ownership, including
   preventing concurrent teardown. This is NOT a concurrent control/UI read.
   Output must be live aligned distinct writable storage and may not overlap
   the DSP handle or any live DSP storage. No reset/allocation; returned record
   is caller-owned. Returns 0 success, 1 null rejection, 2 contained panic.
   Rejection leaves output unchanged; reserved fields are zero on success. */
int32_t ravet_gain_meters(const RavetGain *, RavetGainMeters *);
int32_t ravet_gain64_meters(const RavetGain64 *, RavetGainMeters *);
#ifdef __cplusplus
}
#endif
