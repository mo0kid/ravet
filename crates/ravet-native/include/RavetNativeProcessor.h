#pragma once
#include <stddef.h>
#include <stdint.h>
#include "RavetNativeEvent.h"
#include "RavetNativeTransport.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Optional SDK-independent processor entry, not a plug-in format factory.
   Exact origin table/handle/sample precision and loaded-library lifetime required.
   Exclusive serialized DSP ownership; stop callbacks for prepare/deactivate/destroy
   and control timing queries. Create/prepare/drop may allocate; normal processing
   and reset use prepared storage. Foreign live/aligned pointer/range validity
   remains caller responsibility even on rejection. Unwinding panics return code 2;
   exception handlers and consumer code have no callback deadline guarantee. */
typedef struct RavetNativeBusLayoutV1 { uint32_t inputs, outputs; } RavetNativeBusLayoutV1;
typedef struct RavetNativeProcessorConfigV1 {
    uint32_t version, maximum_frames, maximum_events, maximum_output_events, bus_count, reserved;
    double sample_rate;
    RavetNativeBusLayoutV1 buses[9];
} RavetNativeProcessorConfigV1;
/* bus inputs/outputs are ADDRESSES OF native float or double pointer ARRAYS, in the originating table's precision. Main bus first,
   followed by prepared auxiliaries. Samples may alias inputs across all buses;
   output ranges must be disjoint. Metadata/results/event records never overlap
   writable samples or the opaque handle. Zero frames permit null sample arrays. */
typedef struct RavetNativeProcessorBusV1 {
    uint32_t input_channels, output_channels;
    const void *inputs, *outputs;
} RavetNativeProcessorBusV1;
typedef struct RavetNativeProcessorResultV1 { uint32_t status, event_count; } RavetNativeProcessorResultV1;
typedef struct RavetNativeProcessorTimingV1 { uint32_t latency_samples, tail_kind; uint64_t tail_samples; } RavetNativeProcessorTimingV1;
/* All call codes: 0 success, 1 rejection/backend fault, 2 panic/poison. Fresh
   stopped preparation recovers poison; reset/deactivate do not conceal it.
   Prepare config version=1/reserved=0, bus_count=1..9, unused bus layouts zero.
   Process result status=0 continue/1 silence/2 fault, zero events on fault.
   Input/output events <=1024; native output capacity covers the entire prepared
   generated capacity even if this block generates fewer events. Null event arrays
   only for zero count/capacity. Null transport means all context unknown.
   Supported raw output prefixes silence on faults; backing tails survive.
   Decoder failures precede DSP; backend/output failures cannot roll back history.
   Timing requires prepared stopped ownership; tail_kind=0 none/1 samples/2 infinite.
   No schema/state/editor/host-notification or format registration API is implied. */
typedef struct RavetNativeProcessorEntryV1 {
    uint32_t version, byte_size, sample_bits, reserved;
    void *(*create)(void);
    void (*destroy)(void *);
    int32_t (*prepare)(void *, const RavetNativeProcessorConfigV1 *);
    int32_t (*reset)(void *);
    int32_t (*deactivate)(void *);
    int32_t (*process)(void *, const RavetNativeProcessorBusV1 *, uint32_t bus_count, uint32_t frames,
                       const RavetNativeEventV1 *, uint32_t input_count, const RavetNativeTransportV1 *,
                       RavetNativeEventV1 *, uint32_t output_capacity, RavetNativeProcessorResultV1 *);
    int32_t (*timing)(void *, RavetNativeProcessorTimingV1 *);
} RavetNativeProcessorEntryV1;
/* Public Gain exports this symbol; consumers choose their own symbol with the
   Rust macro. 32/64 select the corresponding table; other values return NULL. */
const RavetNativeProcessorEntryV1 *ravet_native_processor_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
