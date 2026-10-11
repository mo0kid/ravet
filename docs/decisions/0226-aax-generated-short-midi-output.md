# 0226: Bounded native AAX generated short-MIDI output

Status: Accepted prototype; promoted public local packages validated.

Use actual public LocalOutput registration and PostMIDIPacket through optional
fixed-capacity native owner/model adoption. Keep existing zero-capacity Context
layouts and factories unchanged. Add separate NoteRouting exports inside the
existing portable routing reference, with Level-only complete state and explicit
channel/key/no-ID scoped release lowering. Ordinary note identity cannot be
represented by dropping IDs; ID0 is still an identity. No SDK source or private
layout copies are introduced.

Pass the full prepared native output-event capacity on every call, regardless of
produced prefix. Whole known routing/MIDI/capacity preflight precedes native
state/DSP. Generated records are checked only after native success: late encoding
failure cannot roll back DSP/history/intent. Queue consumption precedes delivery.
The SDK has no transactional output reservation; first/late refusal consumes
readiness, preserves exact original returned result and partial counts, and never
replays an old batch. Exceptions have attempted/notreturned diagnostics with no
fabricated SDK result. Backend failure is failclosed; local encoding/host refusal
keeps native ready for a fresh block. CONTROL stop/join owns lifecycle/diagnostics.

Whole callback admission starts before the first foreign input getter, so input
getter and output post reentry cannot run nested getter/DSP/silence/diagnostic
mutation. Fixed packet storage is protected throughout synchronous host borrows.
Staging rejects unsupported system/CC/UMP/SysEx/rich note data as whole lists and
retains old bytes while invalidating stale readiness. The SDK's documented raw
subset is supported by the stage, while actual reference production proves notes.

Target evidence: strict normal/fast stage and actual registered SDK mono/stereo
native production, plus unchanged default Gain/Performance,8/8 CTest. Rust10
state/layout/precision/order/scope/capacity/native-codec tests and release/lint/doc
pass. Real phase/history/capacity oracles distinguish pre-DSP rejection from late
corruption/native0 advanced history and no posts. Original host refusal/throw,
getters/Post reentry, no replay and repeated maximum allocation-counter cycles
pass. Counters instrument C++ allocation only, not Rust/host/deadlines. Promoted
public native Gain30/30, Performance4/4 and stock14/14 packages and actual
loaded-module contracts pass. Public Rust10/doctests, warning-denied Clippy/Rustdoc,
Python8 and strict documentation checks pass; new combined workspace acceptance
is pending. No production plugin,
Pro Tools/signing/arbitrary routing, AAXf64/aux generated, identity/voice cleanup,
SysEx/system/UMP or other-platform/deadline support is supplied by these fixtures.
