#pragma once
#include "RavetNativeAudioState.h"
#include "RavetNativeState.h"
#include "RavetNativeTelemetry.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Distinct 16-voice raw-short-MIDI sine reference; typed32/64, None input/mono
   or stereo output. Existing ADSR/voice/pedal/bend/RPN/MPE contracts apply.
   Level ID1/default0.2 is the complete global state, product RAVETMS\0/version1.
   Raw Program Change/bank requests are observed but do not replace Level.
   Telemetry IDs1..7: cumulative frames/packets, last raw checksum/classes/count,
   voice slot0, Level. IDs8..23: last request per channel, exact samples=1+(bank<<8|program),
   zero absent. Checksum is FNV1a64 of little-endian u32 offset then wire bytes;
   class mask channel=status>>4, system=16+(status&15). Voice slot0 samples packs
   lifecycle<<16 | (1+(channel<<8|key)), absent note=0; value is wire-normalized velocity.
   CONTROL acquisition/retirement and exact processor/cache/library provenance;
   stopped exclusive snapshots only, no automatic cross-thread publication or
   session telemetry codec. Reset/prepare clear history/requests/counts, keep Level.
   No generic MIDI output, SDK registration, device/editor or deadline guarantee. */
const RavetNativeProcessorEntryV1 *ravet_midi_sine_processor_v1(uint32_t sample_bits);
const RavetNativeParameterEntryV1 *ravet_midi_sine_parameters_v1(uint32_t sample_bits);
const RavetNativeCachedParameterEntryV1 *ravet_midi_sine_cached_parameters_v1(uint32_t sample_bits);
const RavetNativeAudioStateEntryV1 *ravet_midi_sine_audio_state_v1(uint32_t sample_bits);
const RavetNativeStateEntryV1 *ravet_midi_sine_state_v1(uint32_t sample_bits);
const RavetNativeTelemetryEntryV1 *ravet_midi_sine_telemetry_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
