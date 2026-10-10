#pragma once
#include <stdint.h>
#include "RavetNativeProcessor.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Optional exact native processor/precision companion, separate from persisted
   parameters/state. Acquire table, create/drop cache on CONTROL; cached info
   reads may overlap; snapshot exclusively owns PREPARED audio DSP. <=1024 fields
   with full uint32 IDs, UTF8 names<=255/units<=63 (no NUL), finite ordered double
   display hints. Raw finite double values may exceed hints; no clipping/narrowing.
   Consumer fixed complete snapshot uses every declared ID once in any order;
   known unique IDs/finite values/known flags preflight before any foreign writes.
   Returned declared order, successful tails and failed output fields preserved.
   Exact uint64 measurement counts, nonfinite input flag, retention/reset/window
   policy belong to consumer. No byte codec/schema provider/framework heap/locks/
   I/O on normal audio calls; explicit consumer hooks carry RT obligations.
   Codes 0 success/1 rejection/2 snapshot panic or shared DSP poison until fresh
   stopped prepare. Cache creation returns NULL on invalid schema/provider panic.
   Library/table/types/cache/owner remain alive and exactly paired; foreign ranges
   live/aligned/disjoint with each other/cache/DSP; retire after borrowers stop.
   No atomics required, automatic cross-thread publication, UI/host scheduling,
   parameter automation, generated events, clearing or audio deadline guarantee. */
enum { RAVET_TELEMETRY_NONFINITE_SAMPLES = 1 };
typedef struct RavetNativeTelemetryInfoV1 {
    uint32_t id, reserved;
    double display_minimum, display_maximum;
    uint32_t name_length, unit_length;
    uint8_t name[256], unit[64];
} RavetNativeTelemetryInfoV1;
typedef struct RavetNativeTelemetryValueV1 {
    uint32_t id, flags;
    double value;
    uint64_t samples;
} RavetNativeTelemetryValueV1;
typedef struct RavetNativeTelemetryEntryV1 {
    uint32_t version, byte_size, sample_bits, telemetry_count;
    const RavetNativeProcessorEntryV1 *(*processor_entry)(void);
    void *(*create_cache)(void);
    void (*destroy_cache)(void *);
    int32_t (*info)(const void *, uint32_t index, RavetNativeTelemetryInfoV1 *);
    int32_t (*snapshot)(void *processor, const void *cache, RavetNativeTelemetryValueV1 *, uint32_t capacity);
} RavetNativeTelemetryEntryV1;
const RavetNativeTelemetryEntryV1 *ravet_native_telemetry_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
