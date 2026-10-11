#pragma once
#include "RavetNativeProcessor.h"
#include "RavetNativeAudioState.h"
#include "RavetNativeState.h"
#include "RavetNativeTelemetry.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Portable reference: matching mono/stereo main input/output, one mono input-only
   detector and one mono/stereo output-only monitor, in either native bus source
   order. Gain ID1 (0..4, default1), Bypass ID2 (0/1, default0); complete global
   RAVETBUS/version1 state is separate from transient processed-frame telemetry.
   Both f32/f64 exact native tables are exported; format/host routing is external.
   Lifecycle, provenance, pointer capacities and exclusive ownership follow the
   included native contracts. Retain this library through every borrower. */
const RavetNativeProcessorEntryV1 *ravet_bus_routing_processor_v1(uint32_t sample_bits);
const RavetNativeParameterEntryV1 *ravet_bus_routing_parameters_v1(uint32_t sample_bits);
const RavetNativeCachedParameterEntryV1 *ravet_bus_routing_cached_parameters_v1(uint32_t sample_bits);
const RavetNativeAudioStateEntryV1 *ravet_bus_routing_audio_state_v1(uint32_t sample_bits);
const RavetNativeStateEntryV1 *ravet_bus_routing_state_v1(uint32_t sample_bits);
const RavetNativeTelemetryEntryV1 *ravet_bus_routing_telemetry_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
