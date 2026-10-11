#ifndef RAVET_VST3_PARAMETERS_H
#define RAVET_VST3_PARAMETERS_H
#include "RavetVst3Provider.h"
#ifdef __cplusplus
extern "C" {
#endif
enum { RAVET_VST3_PARAMETER_READ=0,RAVET_VST3_PARAMETER_PLAIN=1,
    RAVET_VST3_PARAMETER_NORMALIZE=2,RAVET_VST3_PARAMETER_EDIT=3 };
enum { RAVET_VST3_PARAMETER_ATTEMPTED=1,RAVET_VST3_PARAMETER_RETURNED=2,
    RAVET_VST3_PARAMETER_THROWN=4,RAVET_VST3_PARAMETER_INVALIDATED=8 };
enum { RAVET_VST3_PARAMETER_STAGE_COUNT=0,RAVET_VST3_PARAMETER_STAGE_INFO=1,
    RAVET_VST3_PARAMETER_STAGE_READ=2,RAVET_VST3_PARAMETER_STAGE_PLAIN=3,
    RAVET_VST3_PARAMETER_STAGE_NORMALIZE=4,RAVET_VST3_PARAMETER_STAGE_FORMAT=5,
    RAVET_VST3_PARAMETER_STAGE_PARSE=6,RAVET_VST3_PARAMETER_STAGE_EDIT=7,
    RAVET_VST3_PARAMETER_STAGE_PROCESS=8,RAVET_VST3_PARAMETER_STAGE_PREPARE=9,
    RAVET_VST3_PARAMETER_STAGE_CREATE=100,RAVET_VST3_PARAMETER_STAGE_QUERY=101,
    RAVET_VST3_PARAMETER_STAGE_CONTROLLER_ID=102,RAVET_VST3_PARAMETER_STAGE_INITIALIZE=103,
    RAVET_VST3_PARAMETER_STAGE_CONNECT=104,RAVET_VST3_PARAMETER_STAGE_BUSES=105,
    RAVET_VST3_PARAMETER_STAGE_ARRANGEMENT=106,RAVET_VST3_PARAMETER_STAGE_PRECISION=107,
    RAVET_VST3_PARAMETER_STAGE_SETUP=108,RAVET_VST3_PARAMETER_STAGE_ACTIVATE_BUS=109,
    RAVET_VST3_PARAMETER_STAGE_ACTIVATE=110,RAVET_VST3_PARAMETER_STAGE_START=111,
    RAVET_VST3_PARAMETER_STAGE_NATIVE_PROCESS=112,RAVET_VST3_PARAMETER_STAGE_STOP=113,
    RAVET_VST3_PARAMETER_STAGE_DEACTIVATE=114,RAVET_VST3_PARAMETER_STAGE_HANDLER=115,
    RAVET_VST3_PARAMETER_STAGE_DISCONNECT=116,RAVET_VST3_PARAMETER_STAGE_TERMINATE=117,
    RAVET_VST3_PARAMETER_STAGE_THREAD=118,RAVET_VST3_PARAMETER_STAGE_PROFILE=119,
    RAVET_VST3_PARAMETER_STAGE_LIFECYCLE=120 };
enum { RAVET_VST3_PARAMETER_FAILURE_NONE=0,RAVET_VST3_PARAMETER_FAILURE_QUEUE_PROTOCOL=1,
    RAVET_VST3_PARAMETER_FAILURE_QUEUE_THREAD=2,RAVET_VST3_PARAMETER_FAILURE_QUEUE_MUTATION=3,
    RAVET_VST3_PARAMETER_FAILURE_CAPACITY=16,RAVET_VST3_PARAMETER_FAILURE_METADATA=17,
    RAVET_VST3_PARAMETER_FAILURE_UNKNOWN_ID=18,RAVET_VST3_PARAMETER_FAILURE_READ_ONLY=19,
    RAVET_VST3_PARAMETER_FAILURE_NATIVE=20,RAVET_VST3_PARAMETER_FAILURE_THROWN=21 };
typedef struct RavetVst3ParameterPoint {uint32_t id,offset;double value;} RavetVst3ParameterPoint;
typedef struct RavetVst3ParameterRecord {
    uint32_t struct_size,id,flags;int32_t steps,unit;double default_normalized;
    uint8_t title[512],short_title[512],units[512];
} RavetVst3ParameterRecord;
typedef struct RavetVst3ParameterReport {
    uint32_t struct_size,stage,failure,index,flags;int32_t native_result;
} RavetVst3ParameterReport;
// Default-disabled, exact-v2 companion; existing provider/state tables unchanged.
// All operations except process are MAIN/stopped/exclusive.
// All mutable CONTROL operations across a module require every audio scope to
// have joined; already-prepared children may each begin a distinct audio scope.
// Metadata and text are copied UTF-8; no SDK interface escapes. Refresh is explicit reconciliation, not
// component delivery. Controller edits require separate timed audio points.
// Counts <=1024, points <=2048 across <=64 IDs, offsets globally nondecreasing
// and strictly increasing per ID. Returned diagnostics preserve native refusals
// and exceptions separately from immutable input-queue protocol faults.
// native_result is an original SDK status only when RETURNED is set. A thrown
// call has no returned status. Owned metadata/profile rejection may include a
// synthetic diagnostic without RETURNED. Admission rejection leaves reports
// unchanged and performs no foreign attempt; initialize reports per call rather
// than interpreting stale fields. Successful CONTROL omits per-call SDK statuses.
typedef struct RavetVst3ParameterProviderV1 {
    uint32_t struct_size,version;
    const RavetVst3ProviderV2* provider;
    int32_t (*refresh)(RavetVst3Instance*,uint32_t*,RavetVst3ParameterReport*);
    int32_t (*info)(const RavetVst3Instance*,uint32_t,RavetVst3ParameterRecord*);
    int32_t (*value)(RavetVst3Instance*,uint32_t,uint32_t,double,double*,RavetVst3ParameterReport*);
    int32_t (*format)(RavetVst3Instance*,uint32_t,double,uint8_t*,uint32_t,uint32_t*,RavetVst3ParameterReport*);
    int32_t (*parse)(RavetVst3Instance*,uint32_t,const uint8_t*,uint32_t,double*,RavetVst3ParameterReport*);
    int32_t (*prepare)(RavetVst3Instance*,const RavetVst3AudioProfile*,RavetVst3ParameterReport*);
    int32_t (*process)(RavetVst3Audio*,const RavetVst3AudioBlock*,const RavetVst3ParameterPoint*,uint32_t,RavetVst3ParameterReport*);
} RavetVst3ParameterProviderV1;
const RavetVst3ParameterProviderV1* ravet_vst3_parameter_provider_v1(void);
#ifdef __cplusplus
}
static_assert(sizeof(RavetVst3ParameterPoint)==16);
static_assert(sizeof(RavetVst3ParameterReport)==24);
static_assert(sizeof(RavetVst3ParameterRecord)==1568);
#endif
#endif
