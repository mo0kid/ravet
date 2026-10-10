# 0165: Public native performance reference

Status: prototype; local actual-library validation passed.

## Decision

Provide a separate public reference archive wrapping the portable 16-voice
performance sine at native f32/f64 precision. Consumer-specific entry names
`ravet_performance_*_v1` retain distinct identity from Gain while reusing native
processor/schema/cache/direct-audio-state and optional live-publication tables.
Keep SDK factories, device/editor ownership and host advertisements separate.
Do not add instrument DSP dependencies to the generic native framework crate.

Level has the existing stable ID 1, physical 0..1/default .2, canonical product
`RAVETPS\0` and version 1. Complete recall preserves voices/phase/envelopes. Voice
history is not persisted; input timing/exact expression and original completion
follow the portable consumer. Live publication remains gated by native 64-bit
atomics. Separate state calls do not imply atomic combined state/render operations;
consumers own scheduling and publication protocol explicitly.

## Validation and limits

The separate actual-library C++ fixture checks both precisions with reusable
native client/cache/audio-state ownership, metadata/codec identity, complete
recall, analytic unquantized audio and tails, full IDs/zero-velocity ownership,
late address rejection, mute/release/original completion, lifecycle and repeated
prepared heap actions. Preparation accepts mono/stereo no-input buses, no auxiliaries,
65536 frames and 1024 input/output events. Positive output capacity enables bounded
completion; zero suppresses notifications without extending internal lifetime.
The reference has no SDK factory/controller event advertisement, editor or DAW/
physical-device/deadline/other-platform proof from this fixture.
