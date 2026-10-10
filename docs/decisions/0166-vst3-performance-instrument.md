# 0166: Public VST3 performance instrument

Status: prototype; local SDK and REAPER offline render validation passed.

## Decision

Connect the separate public performance archive to an external SDK factory and
controller with distinct processor/controller identities. Keep one Level schema
parameter, no audio input, mono/stereo output and one 16-channel event input.
Use the reusable native audio, transport, parameter and performance input helpers.
Advertise only the five expressions that affect this sine reference's sound.
Accept/store brightness and pressure without advertising audible timbre changes.

Canonical 24-byte RAVETPS state is decoded/encoded by a control-owned native
schema instance. A lock-free 64-bit word carries level and a wrapping generation.
Prepared audio reads once and conditionally reflects once; newer control recall
survives a failed comparison. Controller-only recall synchronizes on connection.
Serialized control callbacks and exclusive audio ownership remain host duties.

Schedule pending recall as the first offset-zero event in the same private stream
as global automation and normalized performance events. Stable merging preserves
each source's order; recall precedes global points, which precede performance
events at equal offsets. Transactional native rendering then makes late semantic
failure atomic with respect to level, voice history and pending recall. Do not
apply complete state separately before potentially failing rendering.

Native voice completion has no VST3 representation. Suppress completion output
explicitly and let the internal voice lifetime finish; never synthesize NoteOff
as a substitute. Keep a host generic parameter editor for this reference.

## Validation and limits

The actual public native archive through the C++ SDK fixture passes controller
registration, five-expression metadata/text bounds, complete state and deferred
connection recall, exact f32/f64 mono/stereo analytic audio, signed SDK identities,
note cents/absolute expressions, mute/release/ID reuse, oscillator/envelope
preservation after late faults and shared capacity rejection. One thousand
prepared render cycles count no C++ heap actions. The dynamically loaded bundle
passes 47 SDK validator tests with zero failures on macOS arm64.

Maximum preparation is 65536 frames; the private merged stream holds 1024 events,
including recall and tuning expansion. Each raw input helper also has its own
1024-item bound. A pending recall plus 1024 mapped inputs rejects; no event is
silently dropped. Callback scratch is bounded stack storage, including nested
helper staging; no deadline or small-stack deployment claim follows. Framework
state snapshot failure after a successfully rendered block cannot roll back DSP;
the known valid archive/profile must uphold its paired snapshot contract.

A guarded disposable REAPER 7.82/macOS arm64 fixture loads the actual bundle and
renders synthetic A4 notes at 48 kHz, complete saved Level recall and held Level
automation. Independent PCM sample checks verify stereo peak/RMS, frequency and
pre/post-note silence. Hardware sends stay muted and transport stays stopped.
This is offline DAW MIDI/state/audio evidence, not host note-expression editing.

No custom editor, instrument programs, MIDI controller mapping, generated output,
physical-device quality, accessibility, other-platform or production-distribution
claim follows. DAW evidence is recorded separately in work status.
