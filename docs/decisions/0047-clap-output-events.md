# 0047 — CLAP generated-event delivery and DSP progression

## Decision

Extend the native main-bus bridge through explicit opt-in generated event storage
and host queues. Keep audio/DSP success separate from output delivery: a host can
accept only a prefix, and already-progressed DSP cannot be rolled back. Preflight
the whole bounded stream for representability, then stop at first host rejection.
Never retry/replay the suffix implicitly. Preserve the old zero-output API.

## Consequences and scope

Output maps global physical parameter values and selected-port raw MIDI/normalized
notes with explicit 7-bit velocity policy. CLAP's reserved ID cannot be represented;
missing MIDI output port also rejects whole generated delivery. Neither fault
invalidates already-successful audio. Invalid input/DSP faults deliver no events.
Capacity is bounded separately at preparation; no callback allocation is added.
Host queue behavior and consumer overflow recovery remain outside the adapter.
Auxiliary audio, CLAP note IDs/modulation, SysEx/UMP and other formats remain pending.

## Validation

Four fixture groups verify exact core-event fields/order/ports/wire padding and
note quantization, zero-frame output, unsupported stream atomicity, host accepted
prefix/discarded suffix, input/DSP fault discard and 10000 heap-free prepared
callbacks. Existing gain/sine export ABI/validator regressions are separate from
actual exported MIDI-output-port host validation, which remains pending.
