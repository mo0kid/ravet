#pragma once
#include "RavetNativeProcessor.h"
#include "RavetNativeAudioState.h"
#include "RavetNativeState.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Fixed matching mono/stereo main Delay8/16 effects, f32/f64 native origins.
   Level ID1 0..1/default1; master Bypass ID2 0/1/default0 retains fixed delay.
   Complete RAVETDLY/v1 globals do not contain FIFO; reset/prepare clear FIFO,
   global recall preserves it. Tail samples are native diagnostic only.
   Tables/caches/state/core must retain exact own-type provenance. */
const RavetNativeProcessorEntryV1* ravet_delay8_processor_v1(uint32_t);
const RavetNativeParameterEntryV1* ravet_delay8_parameters_v1(uint32_t);
const RavetNativeCachedParameterEntryV1* ravet_delay8_cached_parameters_v1(uint32_t);
const RavetNativeAudioStateEntryV1* ravet_delay8_audio_state_v1(uint32_t);
const RavetNativeStateEntryV1* ravet_delay8_state_v1(uint32_t);
const RavetNativeProcessorEntryV1* ravet_delay16_processor_v1(uint32_t);
const RavetNativeParameterEntryV1* ravet_delay16_parameters_v1(uint32_t);
const RavetNativeCachedParameterEntryV1* ravet_delay16_cached_parameters_v1(uint32_t);
const RavetNativeAudioStateEntryV1* ravet_delay16_audio_state_v1(uint32_t);
const RavetNativeStateEntryV1* ravet_delay16_state_v1(uint32_t);
#ifdef __cplusplus
}
#endif
