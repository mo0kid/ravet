# 0163: Native performance voice ownership

Status: implemented; local tests, lint and documentation validation passed.

## Decision

Provide a separate fixed 1..128-slot receiver for normalized/identified note
events within one routed endpoint. Preserve optional full-u32 identities,
unquantized f32 velocity/release velocity and seven exact f64 expressions. A
zero-velocity On allocates a held voice. Existing MIDI/MPE receivers retain their
wire and pedal policies; this receiver does not infer MIDI conversion or pedals.

Every present target field is conjunctive. Unidentified On events stack; Off
releases all held matches, and repeated Off preserves the first release velocity.
Released ownership remains until explicit DSP completion. Expressions affect
held and released voices, without staging defaults for future notes. Volume zero
mutes without terminating. Defaults are volume/expression/brightness one, pan
half and tuning/vibrato/pressure zero; DSP application stays consumer-owned.

A duplicate concrete ID retriggers its existing slot only at the same channel/key,
resetting expressions without completing an identity still owned. Another address
rejects before mutation until completion/reset. Allocation prefers idle, oldest
released, then oldest held. Bounded unique rank ages avoid sequence overflow.
Steal and choke report original completed targets in fixed slot order. Explicit
finish returns the original target once; completion is never converted to Off.
Reset deliberately discards ownership under consumer lifecycle coordination.

Complete timed-stream preflight simulates on a copied fixed bank, checking all
offset/order/capacity/scalar/identity semantics before consumer DSP/state changes.
It counts immediate event-driven completions only. Consumers retain exclusive
ownership, budget later DSP completions separately and control output staging.
This receiver does not reserve storage or publish events implicitly.

## Validation and limits

Tests cover identity sign halves/max-u32, unquantized zero/subnormal velocities,
same-ID retrigger/address rejection, every wildcard scope, live release-tail
expression replacement, exact expression zeros/subnormals/defaults/bounds,
malformed atomic rejection, priority/chronological stealing beyond u8 ages,
late timed semantic failure, zero/max blocks, all128 masks and original completion
targets. Allocation-instrumented repeated maximum-bank and full-list preflight
workloads establish only local absence of Rust heap actions. No synthesis, format
export, physical device, DAW, other-platform or callback-deadline claim follows.
