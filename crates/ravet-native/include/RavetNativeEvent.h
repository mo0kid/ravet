#pragma once
#include <stdint.h>
/* In-process v1 records, native endian/alignment; NOT wire/preset serialization.
   No exported functions or SDK registration. Consumers select endpoint routing,
   own foreign range safety and provide stopped/prepared destination storage.
   Unknown tags/bits, nonzero unused fields and lossy f32 scalars reject.
   Offsets order events within the active block; zero frames require offset zero.
   NoteEnd offset is delivery order, not exact voice termination time. */
enum RavetNativeEventKindV1 {
    RAVET_NATIVE_PARAMETER_VALUE = 1,
    RAVET_NATIVE_PARAMETER_MODULATION = 2,
    RAVET_NATIVE_PARAMETER_GESTURE = 3,
    RAVET_NATIVE_NOTE_ON = 4,
    RAVET_NATIVE_NOTE_OFF = 5,
    RAVET_NATIVE_NOTE_ON_WITH_ID = 6,
    RAVET_NATIVE_NOTE_OFF_WITH_ID = 7,
    RAVET_NATIVE_NOTE_OFF_SCOPED = 8,
    RAVET_NATIVE_NOTE_CHOKE = 9,
    RAVET_NATIVE_NOTE_END = 10,
    RAVET_NATIVE_NOTE_EXPRESSION = 11,
    RAVET_NATIVE_PARAMETER_VALUE_SCOPED = 12,
    RAVET_NATIVE_PARAMETER_MODULATION_SCOPED = 13,
    RAVET_NATIVE_MIDI = 14,
    RAVET_NATIVE_UMP = 15
};
enum RavetNativeScopeV1 { RAVET_NATIVE_SCOPE_CHANNEL = 1, RAVET_NATIVE_SCOPE_KEY = 2, RAVET_NATIVE_SCOPE_ID = 4 };
/* scope bits select channel/key/id; unset is wildcard. Concrete note tags use
   channel/key without scope bits; identity tags use note_id even when UINT32_MAX.
   detail: gestures 0 begin/1 end; expressions 0 volume/1 pan/2 tuning/3 vibrato/
   4 expression/5 brightness/6 pressure; MIDI byte count; UMP word count.
   MIDI bytes are packed at shifts 0,8,16 in words[0]. UMP words are numeric in
   specification order. Zero unused words. value: exact widened float for global
   scalars/velocities, double for scoped scalars/expressions; otherwise +0.
   Volume expression is finite0..4 with zero mute; CLAP excludes zero. */
typedef struct RavetNativeEventV1 {
    uint32_t offset, kind, parameter_id, note_id, scope, channel, key, detail;
    double value;
    uint32_t words[4];
} RavetNativeEventV1;
/* Legacy physical global parameter layout, shared by native Gain. */
typedef struct RavetNativeParameterEvent {
    uint32_t offset, id;
    float value;
} RavetNativeParameterEvent;
