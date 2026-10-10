#pragma once
#include <stdint.h>
#include "RavetNativeProcessor.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Optional native global parameter/state companion. Match exact originating
   processor entry/handle/precision and keep its library loaded. State calls need
   exclusive STOPPED ownership; this is not live cross-thread publication.
   Schema/roles remain immutable for the entry lifetime. Call codes 0 success,
   1 rejection, 2 panic/owner poison; fresh stopped preparation recovers poison.
   Metadata roles describe consumer behavior, without implicit event filtering. */
enum RavetNativeParameterFlagsV1 {
    RAVET_PARAMETER_AUTOMATABLE = 1,
    RAVET_PARAMETER_BYPASS = 2,
    RAVET_PARAMETER_READ_ONLY = 4
};
/* UTF-8 names <=255/unit <=63 bytes; lengths exclude trailing NUL, unused bytes
   zero. Oversize/embedded NUL rejects rather than truncating. Physical bounds and
   default use float. scale=0 linear/1 logarithmic; intervals=0 continuous, else
   intervals+1 choices. Full uint32_t ID, including UINT32_MAX. */
typedef struct RavetNativeParameterInfoV1 {
    uint32_t id, flags, scale, intervals;
    float minimum, maximum, default_value;
    uint32_t name_length, unit_length, reserved;
    uint8_t name[256], unit[64];
} RavetNativeParameterInfoV1;
typedef struct RavetNativeParameterValueV1 { uint32_t id; float value; } RavetNativeParameterValueV1;
/* All foreign ranges live/aligned/disjoint from handle and each other. Snapshot
   capacity covers parameter_count; successful prefix follows declared schema IDs
   and tails survive. Save capacity covers state_bytes and preserves trailing bytes.
   Canonical state: Foundation little-endian product/version/count and ID/f32 pairs.
   Whole decoded snapshots validate before consumer atomic restore. Load <=8208
   bytes; explicit consumer decoder can migrate legacy records. Generic default
   codec accepts its exact canonical product/version/complete IDs only.
   Pure mapping follows finite clamp/snap Foundation f32 range rules; invalid IDs/
   nonfinite values reject and preserve output. No SDK registration, live state
   publisher, scoped granularity, formatting/group/program/editor API implied. */
typedef struct RavetNativeParameterEntryV1 {
    uint32_t version, byte_size, sample_bits, parameter_count, state_bytes, state_version;
    uint8_t state_product[8];
    const RavetNativeProcessorEntryV1 *(*processor_entry)(void);
    int32_t (*info)(uint32_t index, RavetNativeParameterInfoV1 *);
    int32_t (*normalize)(uint32_t id, float physical, float *normalized);
    int32_t (*denormalize)(uint32_t id, float normalized, float *physical);
    int32_t (*snapshot)(void *, RavetNativeParameterValueV1 *, uint32_t capacity);
    int32_t (*save_state)(void *, uint8_t *, uint32_t capacity);
    int32_t (*load_state)(void *, const uint8_t *, uint32_t length);
} RavetNativeParameterEntryV1;
/* Public Gain exports this; consumer macro can choose another symbol. 32/64
   select matching valid metadata; invalid precision/metadata returns NULL. */
const RavetNativeParameterEntryV1 *ravet_native_parameters_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
