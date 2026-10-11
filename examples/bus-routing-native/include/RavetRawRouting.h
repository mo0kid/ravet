#pragma once
#include "RavetNativeProcessor.h"
#include "RavetNativeParameters.h"
#include "RavetCachedParameters.h"
#include "RavetNativeAudioState.h"
#include "RavetNativeState.h"
#include "RavetNativeTelemetry.h"
// Distinct raw main/aux and Delay8/16 references; exact selected32/64 core and
// companion tables. Host/source-port mapping is external. Complete globals are
// Gain/Bypass (bus) or Level/Bypass (delay); MIDI observation is transient.
// Stopped telemetry10 IDs: frames, total raw packets, last ordered raw FNV1a64
// checksum (u32LE offset then exact wire), class mask, raw count, acknowledged
// gain/level(value), bypass(value), native Transport flags(sample/beat/tempo/play/
// discontinuity/details bits0..5), mixed checksum, mixed event count. Mixed FNV
// bytes: u32LE offset, tag14+wire or tag1+u32LE paramID+f32LE value; unsupported
// event kinds use tag0 only. No persisted event history or port identity implied.
#ifdef __cplusplus
extern "C" {
#endif
const RavetNativeProcessorEntryV1 *ravet_raw_bus_routing_processor_v1(uint32_t sample_bits);
const RavetNativeParameterEntryV1 *ravet_raw_bus_routing_parameters_v1(uint32_t sample_bits);
const RavetNativeCachedParameterEntryV1 *ravet_raw_bus_routing_cached_parameters_v1(uint32_t sample_bits);
const RavetNativeAudioStateEntryV1 *ravet_raw_bus_routing_audio_state_v1(uint32_t sample_bits);
const RavetNativeStateEntryV1 *ravet_raw_bus_routing_state_v1(uint32_t sample_bits);
const RavetNativeTelemetryEntryV1 *ravet_raw_bus_routing_telemetry_v1(uint32_t sample_bits);
const RavetNativeProcessorEntryV1 *ravet_raw_delay8_processor_v1(uint32_t sample_bits);
const RavetNativeParameterEntryV1 *ravet_raw_delay8_parameters_v1(uint32_t sample_bits);
const RavetNativeCachedParameterEntryV1 *ravet_raw_delay8_cached_parameters_v1(uint32_t sample_bits);
const RavetNativeAudioStateEntryV1 *ravet_raw_delay8_audio_state_v1(uint32_t sample_bits);
const RavetNativeStateEntryV1 *ravet_raw_delay8_state_v1(uint32_t sample_bits);
const RavetNativeTelemetryEntryV1 *ravet_raw_delay8_telemetry_v1(uint32_t sample_bits);
const RavetNativeProcessorEntryV1 *ravet_raw_delay16_processor_v1(uint32_t sample_bits);
const RavetNativeParameterEntryV1 *ravet_raw_delay16_parameters_v1(uint32_t sample_bits);
const RavetNativeCachedParameterEntryV1 *ravet_raw_delay16_cached_parameters_v1(uint32_t sample_bits);
const RavetNativeAudioStateEntryV1 *ravet_raw_delay16_audio_state_v1(uint32_t sample_bits);
const RavetNativeStateEntryV1 *ravet_raw_delay16_state_v1(uint32_t sample_bits);
const RavetNativeTelemetryEntryV1 *ravet_raw_delay16_telemetry_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
