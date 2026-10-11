# Native identified-performance reference

This public reference exports reusable native v1 processor/schema/cache/audio-state
and (with native64-bit atomics) independent live-publication tables for a16-voice
`PerformanceSine`. It supports native f32/f64 mono/stereo no-input processing and
explicit bounded original-target completion output. Level uses stable ID1, with
canonical complete state product `RAVETPS\0`, version1; voice/history is not saved.

The consumer-specific `ravet_performance_*_v1` symbols preserve a separate identity
from the Gain reference while using the same reusable table layouts. Keep its tables
and library alive until all owners/callbacks are retired. Control prepare/drop and
exclusive audio/state scheduling follow the native contracts. Companion state
calls are separate operations; callers own publication/recall/processing ordering.

This archive is not a plug-in factory, editor or SDK event-bus advertisement. See
[the instrument guide](../../docs/midi.md) for synthesis/rejection/precision policy
and [native processing](../../docs/native-processing.md) for foreign ownership.


The separate `midi` module exports `ravet_midi_sine_*_v1` native32/64 processor,
parameters/cache/audio-state/state/telemetry tables for a raw-short-MIDI sine
reference. It uses the existing portable sine kernel with automatic program Level
replacement explicitly disabled. Raw bank/program requests stay observable while
acknowledged global Level remains coherent. State product `RAVETMS\0`/version1
persists only Level; voices/history/raw telemetry are transient. Its distinct
[AAX-only MidiSine reference](../../docs/aax-short-midi.md) has its own public IDs,
no-input mono/stereo buses and no VST3 DSP class in the required SDK bootstrap.
Existing identified Performance exports, state and output-completion behavior
remain separate. See `include/RavetMidiSine.h` for exact telemetry fields and lifetime.
