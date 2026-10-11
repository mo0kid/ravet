#pragma once
#include "RavetNativeProcessor.h"
#include "RavetNativeAudioState.h"
#include "RavetNativeState.h"
#include "RavetNativeTelemetry.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Portable note-routing reference: no input, main mono/stereo silent audio.
   Level ID1 0..1/default1; complete RAVETNTE/version1/24-byte state. Ordinary
   nonidentity notes route at original offsets; On velocity scales by Level,
   Off velocity retained. Only exact channel+key/no-ID scoped release lowers to
   ordinary Off under explicit reference policy. Rich identities/wildcards reject.
   Generated event capacity 1..1024 and maximum frames/events 1024. Full prepared
   capacity is required on every native process call, even empty generated prefix.
   Transient frame counter clears on prepare/reset, survives complete Level recall.
   Exact f32/f64 table provenance and exclusive ownership follow native headers. */
const RavetNativeProcessorEntryV1 *ravet_note_routing_processor_v1(uint32_t sample_bits);
const RavetNativeParameterEntryV1 *ravet_note_routing_parameters_v1(uint32_t sample_bits);
const RavetNativeCachedParameterEntryV1 *ravet_note_routing_cached_parameters_v1(uint32_t sample_bits);
const RavetNativeAudioStateEntryV1 *ravet_note_routing_audio_state_v1(uint32_t sample_bits);
const RavetNativeStateEntryV1 *ravet_note_routing_state_v1(uint32_t sample_bits);
const RavetNativeTelemetryEntryV1 *ravet_note_routing_telemetry_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
