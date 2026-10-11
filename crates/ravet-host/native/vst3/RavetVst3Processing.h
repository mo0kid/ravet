#ifndef RAVET_VST3_PROCESSING_H
#define RAVET_VST3_PROCESSING_H
#include "RavetVst3Parameters.h"
#ifdef __cplusplus
extern "C" {
#endif

/* Owned wire carrier, never an SDK union or borrowed SDK payload. Payload
 * offsets/counts are bytes in a separately bounded immutable byte span. Text
 * counts exclude a terminator; native pool construction adds its own UTF-16 NUL.
 * Unused kind-specific fields are ignored; reserved must be zero. Input note
 * identities are -1/nonnegative, output preserves every signed SDK identity.
 * Normalized expressions/velocity/pressure are finite [0,1]; tuning is cents.
 * velocity carries PolyPressure's f32 pressure; value carries f64 expressions.
 * Integer expressions retain all 64 bits. Chord/scale masks retain 16 bits.
 * Data subtype0 is opaque counted SysEx, with no framing changes or MIDI parser.
 */
typedef struct RavetVst3EventRecord {
    uint32_t struct_size;
    uint16_t type, flags;
    uint32_t bus, offset;
    double ppq_position;
    int32_t channel, pitch, note_id, note_length;
    float tuning, velocity;
    uint32_t expression_id, data_type;
    double value;
    uint64_t integer_value;
    int32_t root, bass;
    uint32_t mask, controller, midi_value, midi_value2;
    uint32_t payload_offset, payload_bytes, reserved;
} RavetVst3EventRecord;
enum {
    RAVET_VST3_EVENT_NOTE_ON=0, RAVET_VST3_EVENT_NOTE_OFF=1,
    RAVET_VST3_EVENT_DATA=2, RAVET_VST3_EVENT_POLY_PRESSURE=3,
    RAVET_VST3_EVENT_EXPRESSION_VALUE=4, RAVET_VST3_EVENT_EXPRESSION_TEXT=5,
    RAVET_VST3_EVENT_CHORD=6, RAVET_VST3_EVENT_SCALE=7,
    RAVET_VST3_EVENT_EXPRESSION_INTEGER=8, RAVET_VST3_EVENT_LEGACY_CC_OUTPUT=65535
};
/* Block-start SDK context with exact declared presence bits. sample_rate and
 * project_time_samples are always present. Unavailable fields are ignored and
 * native SDK construction writes zero for them. Times/music remain signed;
 * system_time is nanoseconds, SMPTE offset is 1/80-frame subframes, next-clock
 * samples may be negative. Chord key/root keep SDK uint8 domains and mask is
 * 12 bits, distinct from the 16-bit event chord/scale masks. Frame-rate accepts
 * any positive uint32 integer FPS and actual pull-down/drop flag bits; the SDK
 * examples are not an exhaustive FPS whitelist. reserved must be zero.
 */
typedef struct RavetVst3ContextRecord {
    uint32_t struct_size,state;
    double sample_rate;
    int64_t project_time_samples,system_time,continuous_time_samples;
    double project_time_music,bar_position_music,cycle_start_music,cycle_end_music,tempo;
    int32_t time_signature_numerator,time_signature_denominator;
    uint32_t chord_key,chord_root,chord_mask;
    int32_t smpte_offset_subframes;
    uint32_t frames_per_second,frame_rate_flags;
    int32_t samples_to_next_clock;
    uint32_t reserved;
} RavetVst3ContextRecord;
/* Caller-prepared whole output. Count fields are ignored on input and updated
 * together only after successful native processing and complete publication
 * admission. All four destination spans and this complete metadata object must
 * be mutually disjoint; capacity suffixes remain untouched. Parameter IDs retain
 * queue creation order, including empty queues; points retain insertion order,
 * strictly sample-ordered per ID rather than sorted across independent IDs.
 */
typedef struct RavetVst3ProcessingOutputs {
    uint32_t struct_size,reserved;
    RavetVst3EventRecord* events;
    uint32_t event_capacity,event_count;
    uint8_t* payload;
    uint32_t payload_capacity,payload_used;
    uint32_t* parameter_ids;
    uint32_t parameter_queue_capacity,parameter_queue_count;
    RavetVst3ParameterPoint* parameter_points;
    uint32_t parameter_point_capacity,parameter_point_count;
} RavetVst3ProcessingOutputs;
enum {
    RAVET_VST3_CONTEXT_PLAYING=1u<<1,RAVET_VST3_CONTEXT_CYCLE_ACTIVE=1u<<2,
    RAVET_VST3_CONTEXT_RECORDING=1u<<3,RAVET_VST3_CONTEXT_SYSTEM_TIME=1u<<8,
    RAVET_VST3_CONTEXT_MUSIC=1u<<9,RAVET_VST3_CONTEXT_TEMPO=1u<<10,
    RAVET_VST3_CONTEXT_BAR=1u<<11,RAVET_VST3_CONTEXT_CYCLE=1u<<12,
    RAVET_VST3_CONTEXT_TIME_SIGNATURE=1u<<13,RAVET_VST3_CONTEXT_SMPTE=1u<<14,
    RAVET_VST3_CONTEXT_CLOCK=1u<<15,RAVET_VST3_CONTEXT_CONTINUOUS=1u<<17,
    RAVET_VST3_CONTEXT_CHORD=1u<<18,
    RAVET_VST3_FRAME_PULL_DOWN=1,RAVET_VST3_FRAME_DROP=2
};
/* Explicit stopped selection. Masks name copied event buses, not audio buses. */
typedef struct RavetVst3ProcessingSelection {
    uint32_t struct_size, input_mask, output_mask, reserved;
} RavetVst3ProcessingSelection;
typedef struct RavetVst3ProcessingInfo {
    uint32_t struct_size, input_bus_count, output_bus_count;
    uint32_t selected_input_mask, selected_output_mask, flags;
    uint32_t context_requirements, reserved;
} RavetVst3ProcessingInfo;
typedef struct RavetVst3EventBusRecord {
    uint32_t struct_size, index, channels, bus_type, flags, reserved;
    char name[512];
} RavetVst3EventBusRecord;
/* No original native result exists unless RETURNED is set. A thrown process
 * leaves native_result zero. HISTORY_MAY_HAVE_ADVANCED is conservative: it does
 * not assert that a particular plug-in changed its state. Per-list protocol
 * failures are separate; their numeric values do not imply a temporal ordering.
 * Local AUDIO admission failures leave this record and all caller output intact.
 */
typedef struct RavetVst3ProcessingReport {
    uint32_t struct_size, stage, failure, index, flags;
    uint32_t input_parameter_failure, output_event_failure, reserved;
    int32_t native_result;
    uint32_t event_count, payload_used, parameter_queue_count, parameter_point_count, reserved2;
} RavetVst3ProcessingReport;
enum {
    RAVET_VST3_PROCESSING_MAPPING=1, RAVET_VST3_PROCESSING_CONTEXT_REQUIREMENTS=2,
    RAVET_VST3_PROCESSING_ATTEMPTED=1, RAVET_VST3_PROCESSING_RETURNED=2,
    RAVET_VST3_PROCESSING_THROWN=4, RAVET_VST3_PROCESSING_INVALIDATED=8,
    RAVET_VST3_PROCESSING_HISTORY_MAY_HAVE_ADVANCED=16,
    RAVET_VST3_PROCESSING_PUBLISHED=32,
    RAVET_VST3_PROCESSING_STAGE_REFRESH=1, RAVET_VST3_PROCESSING_STAGE_PREPARE=2,
    RAVET_VST3_PROCESSING_STAGE_PROCESS=3, RAVET_VST3_PROCESSING_STAGE_PUBLICATION=4,
    RAVET_VST3_PROCESSING_FAILURE_NATIVE=1, RAVET_VST3_PROCESSING_FAILURE_THROWN=2,
    RAVET_VST3_PROCESSING_FAILURE_PROTOCOL=3, RAVET_VST3_PROCESSING_FAILURE_PUBLICATION=4
};
/* Exact identity pairing, independent of the unchanged v2/state/parameter ABI.
 * refresh requires a prior successful stopped parameter refresh, including an
 * empty catalogue for controllerless components. bus_info and midi_assignment
 * only read owned copied metadata. Unassigned mappings return UINT32_MAX.
 * A nullable block context supplies no synthetic fields; SDK requirements are
 * copied requests, not a demand that a host invent unavailable data.
 * process requires complete caller-owned output storage even for empty output.
 * After native/late failure audio prefixes are silent, output metadata/counts
 * and backing storage are untouched, and stopped reconciliation is required.
 */
typedef struct RavetVst3ProcessingProviderV1 {
    uint32_t struct_size, version;
    const RavetVst3ProviderV2* instance_provider;
    int32_t (*refresh)(RavetVst3Instance*, const RavetVst3ProcessingSelection*, RavetVst3ProcessingInfo*, RavetVst3ProcessingReport*);
    int32_t (*bus_info)(const RavetVst3Instance*, uint32_t, uint32_t, RavetVst3EventBusRecord*);
    int32_t (*midi_assignment)(const RavetVst3Instance*, uint32_t, uint32_t, uint32_t, uint32_t*);
    int32_t (*prepare)(RavetVst3Instance*, const RavetVst3AudioProfile*, RavetVst3ProcessingReport*);
    int32_t (*process)(RavetVst3Audio*, const RavetVst3AudioBlock*, const RavetVst3ParameterPoint*, uint32_t,
        const RavetVst3EventRecord*, uint32_t, const uint8_t*, uint32_t,
        const RavetVst3ContextRecord*, RavetVst3ProcessingOutputs*, RavetVst3ProcessingReport*);
} RavetVst3ProcessingProviderV1;
const RavetVst3ProcessingProviderV1* ravet_vst3_processing_provider_v1(void);
#ifdef __cplusplus
}
static_assert(sizeof(RavetVst3EventRecord)==112);
static_assert(sizeof(RavetVst3ContextRecord)==120);
static_assert(sizeof(RavetVst3ProcessingOutputs)==72);
static_assert(sizeof(RavetVst3ProcessingSelection)==16);
static_assert(sizeof(RavetVst3ProcessingInfo)==32);
static_assert(sizeof(RavetVst3EventBusRecord)==536);
static_assert(sizeof(RavetVst3ProcessingReport)==56);
static_assert(sizeof(RavetVst3ProcessingProviderV1)==56);
#endif
#endif
