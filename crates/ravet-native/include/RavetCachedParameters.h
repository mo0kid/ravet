#pragma once
#include <stdint.h>
#include "RavetNativeParameters.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Optional control-prepared immutable schema cache. Acquire entry, create/drop
   cache and parameters_entry on CONTROL. Cached info/mapping/convert_events may
   overlap on any thread and never call consumer providers or borrow DSP. Keep
   exact table/cache/library alive; destroy only after every borrower stops.
   Global f32 ranges/roles, <=1024 descriptors, complete immutable UTF8 metadata.
   Bounded binary ID lookup; no cached-call allocation/deallocation, locks or I/O.
   Roles declare behavior without implicit read-only/automation filtering.
   Pure scalar mapping uses Foundation finite clamp/log/step/ties-up rules.
   Native IDs retain full uint32_t, including UINT32_MAX; format consumers may
   require stricter ID/text policies. No automatic SDK notifications/scheduling.
   Valid live/aligned/disjoint foreign ranges remain caller obligations. Codes
   0 success/1 rejection; failed outputs unchanged. Factory returns NULL on
   invalid metadata or provider panic. Cached calls invoke no user callback. */
typedef struct RavetNativeNormalizedParameterEventV1 {
    uint32_t offset, id;
    double normalized;
} RavetNativeNormalizedParameterEventV1;
/* Bulk input: ordered offsets, equal-offset order preserved, finite 0..1 double
   normalized intent explicitly narrows to Foundation float normalized values.
   Result: tagged global parameter-value records with exact widened physical
   float. Unknown IDs/nonfinite/out-of-range/timing/capacity reject atomically.
   <=1024 input/output capacity, <=65536 frames; zero-frame offsets must be zero.
   Full preflight precedes output mutation, successful tails survive. Consumer
   explicitly routes/filters unsupported host IDs before conversion. */
typedef struct RavetNativeCachedParameterEntryV1 {
    uint32_t version, byte_size, sample_bits, parameter_count;
    const RavetNativeParameterEntryV1 *(*parameters_entry)(void);
    void *(*create)(void);
    void (*destroy)(void *);
    int32_t (*info)(const void *, uint32_t index, RavetNativeParameterInfoV1 *);
    int32_t (*normalize)(const void *, uint32_t id, float physical, float *normalized);
    int32_t (*denormalize)(const void *, uint32_t id, float normalized, float *physical);
    int32_t (*convert_events)(const void *, const RavetNativeNormalizedParameterEventV1 *, uint32_t count, uint32_t frames, RavetNativeEventV1 *, uint32_t capacity);
} RavetNativeCachedParameterEntryV1;
/* Public Gain symbol; consumer macro may choose another. Unsupported precision
   or invalid consumer metadata -> NULL; cache factory independently revalidates. */
const RavetNativeCachedParameterEntryV1 *ravet_native_cached_parameters_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
