#pragma once
#include "RavetNativeAudioState.h"
#include "RavetNativeState.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Public 16-voice normalized/identified sine reference. Select 32/64 bits;
   unsupported precision returns NULL. Exact library/table/handle pairing and
   stopped-control/exclusive-audio ownership follow the reusable native headers.
   Mono/stereo no-input buses; bounds65536 frames/1024 input and output events.
   Positive output capacity enables original-target completion; zero suppresses
   notifications only. Level ID1/schema state product RAVETPS\0/version1.
   Voices/history are not saved. No implicit recall/scheduling/SDK/editor.
   State publication table requires native64-bit atomics in the Rust build. */
const RavetNativeProcessorEntryV1 *ravet_performance_processor_v1(uint32_t sample_bits);
const RavetNativeParameterEntryV1 *ravet_performance_parameters_v1(uint32_t sample_bits);
const RavetNativeCachedParameterEntryV1 *ravet_performance_cached_parameters_v1(uint32_t sample_bits);
const RavetNativeAudioStateEntryV1 *ravet_performance_audio_state_v1(uint32_t sample_bits);
const RavetNativeStateEntryV1 *ravet_performance_state_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
