#ifndef RAVET_VST3_STATE_H
#define RAVET_VST3_STATE_H
#include "RavetVst3Provider.h"
#ifdef __cplusplus
extern "C" {
#endif
enum { RAVET_VST3_CONTROLLER_ABSENT=0, RAVET_VST3_CONTROLLER_UNSUPPORTED=1,
       RAVET_VST3_CONTROLLER_STATE=2 };
enum { RAVET_VST3_STATE_NONE=0, RAVET_VST3_COMPONENT_CAPTURE=1,
       RAVET_VST3_CONTROLLER_CAPTURE=2, RAVET_VST3_COMPONENT_RESTORE=3,
       RAVET_VST3_CONTROLLER_COMPONENT_RESTORE=4, RAVET_VST3_CONTROLLER_RESTORE=5 };
enum { RAVET_VST3_STATE_MUTATING=1, RAVET_VST3_STATE_THROWN=2,
       RAVET_VST3_STATE_POISONED=4, RAVET_VST3_STATE_PREPARE_REQUIRED=8,
       RAVET_VST3_STATE_ATTEMPTED=16, RAVET_VST3_STATE_RETURNED=32 };
typedef struct RavetVst3StateRecord {
    uint32_t struct_size, controller_mode, component_size, controller_size;
    uint8_t class_id[16];
} RavetVst3StateRecord;
typedef struct RavetVst3StateReport {
    uint32_t struct_size, stage, stream_failure, flags;
    int32_t native_result, controller_capture_result;
} RavetVst3StateReport;
// Separate exact-table companion: catalogue-v1/provider-v2 layouts stay unchanged.
// Main-owned, stopped, exclusive instance calls only. Capacities are explicit
// 1..8 MiB per capture stream; snapshots contain copied opaque bytes. Successful
// restore requires explicit reprepare. Failed foreign restore poisons the child
// without rollback, while joined retirement stays available. Reports preserve
// SDK status independently of stream faults and caught exceptions. native_result
// is an original returned SDK status only when RETURNED is set; after THROWN it
// is the local kInternalError placeholder. Rejected
// preflight preserves all caller bytes and reports. Capture publishes only after
// both mandatory component and supported optional controller captures succeed.
typedef struct RavetVst3StateProviderV1 {
    uint32_t struct_size, version;
    const RavetVst3ProviderV2* provider;
    int32_t (*capture)(RavetVst3Instance*, RavetVst3StateRecord*, uint8_t*, uint32_t,
                       uint8_t*, uint32_t, RavetVst3StateReport*);
    int32_t (*restore)(RavetVst3Instance*, const RavetVst3StateRecord*, const uint8_t*,
                       const uint8_t*, RavetVst3StateReport*);
} RavetVst3StateProviderV1;
const RavetVst3StateProviderV1* ravet_vst3_state_provider_v1(void);
#ifdef __cplusplus
}
static_assert(sizeof(RavetVst3StateRecord)==32);
static_assert(sizeof(RavetVst3StateReport)==24);
#endif
#endif
