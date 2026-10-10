#pragma once
#include <stdint.h>
#include "RavetCachedParameters.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Optional exact processor/cache/precision companion; all earlier v1 tables stay
   unchanged. Acquire tables/create cache on CONTROL; retire after borrowers stop.
   Apply/snapshot exclusively borrow PREPARED audio DSP, with cached metadata and
   explicit consumer RT hooks: no codecs/providers/framework heap/locks/I/O.
   Complete physical float global base state, <=1024 fields/capacity, full uint32
   IDs. Apply count must equal declared count; any input order, known unique IDs,
   canonical finite ranges. Whole validation precedes one atomic consumer restore.
   Caller schedules only needed restores (repetition can restart smoothing).
   Snapshot validates/reorders complete values to cached declared order before
   writes. Rejection leaves outputs unchanged; success preserves unused tails.
   Codes 0 success/1 rejection/2 panic or shared DSP poison until fresh stopped
   preparation. Consumer panic may have changed its internal state; no rollback.
   Loaded library, exact originating table/cache/processor types stay alive.
   Foreign live/aligned/disjoint ranges must not overlap cache/DSP or each other.
   No atomics required, automatic publication/generation/scheduling/notifications
   or retries. Mixing controller protocols is explicit consumer policy. */
typedef struct RavetNativeAudioStateEntryV1 {
    uint32_t version, byte_size, sample_bits, parameter_count;
    const RavetNativeCachedParameterEntryV1 *(*cached_parameters_entry)(void);
    int32_t (*apply)(void *processor, const void *cache, const RavetNativeParameterValueV1 *, uint32_t count);
    int32_t (*snapshot)(void *processor, const void *cache, RavetNativeParameterValueV1 *, uint32_t capacity);
} RavetNativeAudioStateEntryV1;
const RavetNativeAudioStateEntryV1 *ravet_native_audio_state_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
