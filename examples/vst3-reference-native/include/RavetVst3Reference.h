#pragma once
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Portable reference DSP behind the RAVET reference VST3 modules (ADR 0274).
   SDKs and format ownership stay outside this ABI. Handles are exclusively
   owned by the serialized lifecycle/processing caller: prepare/destroy only
   while callbacks are stopped. Prepare allocates. Process and reset make no
   Rust heap operations: measured with allocation-counter on every entry point
   in the crate's unit tests, and inside the loaded reference bundles through
   the test-only allocation-probe build feature, whose exported
   ravet_vst3_reference_allocation_count() returns this image's Rust heap
   operation total (alloc, alloc_zeroed, realloc, dealloc). C++ and direct
   malloc use by callers is outside this claim.
   Results: 0 success, 1 validated rejection (valid outputs silenced), 2
   contained panic. Zero frames permit null sample pointers. Overlapping
   output channels are rejected (1) instead of being written through aliases.
   A failed prepare leaves the handle unprepared; every process call on an
   unprepared handle, or through the entry point of the other precision, is
   rejected. */

/* Keyed gain: out[c][n] = main[c][n] * key[n], key = key0[n] (mono),
   (key0[n] + key1[n]) * 0.5 (stereo) or exactly 1 with key_channels 0 (an
   inactive auxiliary bus). sample_bits selects the only admitted process entry.
   Inputs (2 main, key_channels key) may alias outputs: every input is copied
   before any output write. The two output channels must not overlap. */
typedef struct RavetKeyedGain RavetKeyedGain;
RavetKeyedGain *ravet_keyed_gain_create(void);
void ravet_keyed_gain_destroy(RavetKeyedGain *);
int32_t ravet_keyed_gain_prepare(RavetKeyedGain *, uint32_t sample_bits,
                                 double rate, uint32_t key_channels,
                                 uint32_t maximum_frames);
int32_t ravet_keyed_gain_process32(RavetKeyedGain *, const float *const *main,
                                   const float *const *key,
                                   float *const *outputs, uint32_t frames);
int32_t ravet_keyed_gain_process64(RavetKeyedGain *, const double *const *main,
                                   const double *const *key,
                                   double *const *outputs, uint32_t frames);

/* Event probe record. code: SDK input event type 0..8 or 16 (parameter point).
   Field use (unused fields are ignored):
     0 note-on   a=channel b=pitch c=note_id d=length f0=tuning f1=velocity
     1 note-off  a=channel b=pitch c=note_id f0=tuning f1=velocity
     2 data      a=data type, payload=bytes, payload_count=byte count
     3 poly      a=channel b=pitch c=note_id f1=pressure
     4 expr      a=type id c=note_id value
     5 text      a=type id c=note_id, payload=UTF-16 units (2-aligned)
     6 chord     a=root b=bass d=mask(0..65535), payload=UTF-16 units
     7 scale     a=root d=mask(0..65535), payload=UTF-16 units
     8 int expr  a=type id c=note_id integer
     16 point    a=parameter id value=normalized value (bus/flags/ppq zero)
   Payloads exclude terminators and stay valid for the call; text must be
   2-byte aligned and a nonzero count needs a non-null payload. Unknown codes
   and masks outside 0..65535 reject the whole call. Offsets must be below
   frames, or zero for a zero-frame block. At most 4096 records per call (the
   VST3 Event Probe admits at most 2048 events and 4096 - events points). */
typedef struct RavetEventProbeRecord {
  uint32_t offset;
  uint16_t code, flags;
  int32_t bus, a, b, c, d;
  float f0, f1;
  uint32_t reserved;
  double ppq_position, value;
  uint64_t integer;
  const void *payload;
  uint32_t payload_count, reserved2;
} RavetEventProbeRecord;
/* Render: left[offset] += code + 1, right[offset] += hash20 * 2^-20 for every
   record; other samples are zero. Records sharing an offset are summed, so the
   left code sum alone is ambiguous there (note-on + note-off, 1 + 2, reads
   like one data event, 3); decoders must know the candidate records or use
   distinct offsets. Exact in f64; exact in f32 while at most 16 records share
   an offset. hash20 is the folded FNV-1a hash of the
   canonical serialization documented in the Rust record_hash API. The running
   digest (prepare/reset clear it) folds each record in order:
   digest = (digest * 0x01000193 ^ hash20) & 0xfffff, written to *digest
   (optional, may be null) on success. A rejected call leaves the digest
   unchanged. The VST3 wrapper renders before it posts output events, so a
   host refusing those posts sees kResultFalse with the digest advanced. */
typedef struct RavetEventProbe RavetEventProbe;
RavetEventProbe *ravet_event_probe_create(void);
void ravet_event_probe_destroy(RavetEventProbe *);
int32_t ravet_event_probe_prepare(RavetEventProbe *, uint32_t maximum_frames);
void ravet_event_probe_reset(RavetEventProbe *);
int32_t ravet_event_probe_process32(RavetEventProbe *,
                                    const RavetEventProbeRecord *,
                                    uint32_t count, float *left, float *right,
                                    uint32_t frames, uint32_t *digest);
int32_t ravet_event_probe_process64(RavetEventProbe *,
                                    const RavetEventProbeRecord *,
                                    uint32_t count, double *left,
                                    double *right, uint32_t frames,
                                    uint32_t *digest);
#ifdef __cplusplus
}
static_assert(sizeof(RavetEventProbeRecord) == 80, "probe record layout");
#endif
