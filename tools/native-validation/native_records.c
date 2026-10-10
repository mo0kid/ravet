#include "RavetNativeAudio.h"
#include "RavetNativeEvent.h"
#include "RavetNativeTransport.h"
#include "RavetNativeProcessor.h"
#include "RavetNativeParameters.h"
#include "RavetNativeState.h"
#include "RavetCachedParameters.h"
#include "RavetNativeAudioState.h"
#include "RavetNativeTelemetry.h"
#include "RavetGain.h"
#include <stddef.h>
#include <stdio.h>
#ifdef __cplusplus
#define RECORD_ASSERT static_assert
#else
#define RECORD_ASSERT _Static_assert
#endif
RECORD_ASSERT(sizeof(RavetNativeEventV1) == 56, "native event size");
RECORD_ASSERT(offsetof(RavetNativeEventV1, value) == 32, "native scalar offset");
RECORD_ASSERT(offsetof(RavetNativeEventV1, words) == 40, "native UMP offset");
RECORD_ASSERT(sizeof(RavetNativeParameterEvent) == sizeof(RavetGainEvent), "legacy event size");
RECORD_ASSERT(offsetof(RavetNativeParameterEvent, value) == offsetof(RavetGainEvent, value), "legacy scalar offset");
RECORD_ASSERT(offsetof(RavetNativeBusF32, inputs) == 8, "bus pointer offset");
RECORD_ASSERT(sizeof(RavetNativeBusF32) == sizeof(RavetNativeBusF64), "bus precision layout");
RECORD_ASSERT(RAVET_NATIVE_NOTE_END == 10 && RAVET_NATIVE_UMP == 15, "v1 tags");
RECORD_ASSERT(sizeof(RavetNativeTransportV1) == 96, "native transport size");
RECORD_ASSERT(offsetof(RavetNativeTransportV1, sample_position) == 8, "native sample position offset");
RECORD_ASSERT(offsetof(RavetNativeTransportV1, bar_start) == 72, "native bar offset");
RECORD_ASSERT(offsetof(RavetNativeTransportV1, reserved) == 92, "native reserved offset");
RECORD_ASSERT(RAVET_TRANSPORT_BAR == 512 && RAVET_TRANSPORT_FLAG_PRE_ROLL == 16, "transport conventions");
RECORD_ASSERT(sizeof(RavetNativeProcessorConfigV1) == 104, "native config size");
RECORD_ASSERT(offsetof(RavetNativeProcessorConfigV1, buses) == 32, "native config layouts");
RECORD_ASSERT(offsetof(RavetNativeProcessorEntryV1, create) == 16, "native entry callbacks");
RECORD_ASSERT(sizeof(RavetNativeProcessorEntryV1) == 16 + 7 * sizeof(void (*)(void)), "native entry size");
RECORD_ASSERT(sizeof(RavetNativeProcessorResultV1) == 8, "native result size");
RECORD_ASSERT(sizeof(RavetNativeProcessorTimingV1) == 16, "native timing size");
RECORD_ASSERT(sizeof(RavetNativeParameterInfoV1) == 360, "native parameter info size");
RECORD_ASSERT(offsetof(RavetNativeParameterInfoV1, name) == 40, "native parameter name offset");
RECORD_ASSERT(offsetof(RavetNativeParameterInfoV1, unit) == 296, "native parameter unit offset");
RECORD_ASSERT(sizeof(RavetNativeParameterValueV1) == 8, "native parameter value size");
RECORD_ASSERT(offsetof(RavetNativeParameterEntryV1, processor_entry) == 32, "native parameter callbacks");
RECORD_ASSERT(sizeof(RavetNativeParameterEntryV1) == 32 + 7 * sizeof(void (*)(void)), "native parameter table size");
RECORD_ASSERT(sizeof(RavetNativeStateRevisionV1) == 16, "state revision size");
RECORD_ASSERT(sizeof(RavetNativeStateApplyV1) == 24, "state apply size");
RECORD_ASSERT(offsetof(RavetNativeStateEntryV1, parameters_entry) == 16, "state callbacks");
RECORD_ASSERT(sizeof(RavetNativeStateEntryV1) == 16 + 8 * sizeof(void (*)(void)), "state table size");
RECORD_ASSERT(sizeof(RavetNativeNormalizedParameterEventV1) == 16, "normalized record size");
RECORD_ASSERT(offsetof(RavetNativeNormalizedParameterEventV1, normalized) == 8, "normalized value offset");
RECORD_ASSERT(offsetof(RavetNativeCachedParameterEntryV1, parameters_entry) == 16, "cached callbacks");
RECORD_ASSERT(sizeof(RavetNativeCachedParameterEntryV1) == 16 + 7 * sizeof(void (*)(void)), "cached table size");
RECORD_ASSERT(offsetof(RavetNativeAudioStateEntryV1, cached_parameters_entry) == 16, "audio state callbacks");
RECORD_ASSERT(sizeof(RavetNativeAudioStateEntryV1) == 16 + 3 * sizeof(void (*)(void)), "audio state table size");
RECORD_ASSERT(sizeof(RavetNativeTelemetryInfoV1) == 352, "telemetry info size");
RECORD_ASSERT(offsetof(RavetNativeTelemetryInfoV1, name) == 32, "telemetry name offset");
RECORD_ASSERT(sizeof(RavetNativeTelemetryValueV1) == 24, "telemetry value size");
RECORD_ASSERT(offsetof(RavetNativeTelemetryValueV1, samples) == 16, "telemetry count offset");
RECORD_ASSERT(offsetof(RavetNativeTelemetryEntryV1, processor_entry) == 16, "telemetry callbacks");
RECORD_ASSERT(sizeof(RavetNativeTelemetryEntryV1) == 16 + 5 * sizeof(void (*)(void)), "telemetry table size");
int main(void) {
    RavetNativeEventV1 midi = {0};
    midi.kind = RAVET_NATIVE_MIDI;
    midi.detail = 3;
    midi.words[0] = 0x007f129f;
    if ((midi.words[0] & 255) != 0x9f || ((midi.words[0] >> 8) & 255) != 0x12 || ((midi.words[0] >> 16) & 255) != 0x7f) return 1;
    if ((RAVET_NATIVE_SCOPE_CHANNEL | RAVET_NATIVE_SCOPE_KEY | RAVET_NATIVE_SCOPE_ID) != 7) return 2;
    puts("native C event/bus layouts and v1 numeric conventions passed");
    return 0;
}
