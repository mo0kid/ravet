# 0012 — Complete CLAP gain state and bounded editor edits

Status: implemented as a prototype; macOS GUI and REAPER 7.82 validation completed
within the scope in [host validation](../host-validation.md).

## Decision

Expose portable gain and boolean bypass through CLAP metadata, automation, text
conversion, active/inactive flush and version-two complete save/restore. Preserve
version-one load compatibility by migrating bypass to off. Use one atomic word
for gain, bypass and a 31-bit restore generation, rather than separate atomics
that could allow mixed session values. This supersedes decision 0003's scalar
host protocol and extends the portable layer from decision 0010. The reduced
generation width is an explicit contract limit, not a general multi-field protocol.

Provide a product main-thread control API backed by 32 complete editor edits.
CLAP's serialized callback consumer emits begin/value/end and retains progression
on output backpressure. Apply values only after the host accepts the output value
so local requested state is not ahead of host notifications. Ordinary gain
smoothing applies to edits. Each committed edit is its own gesture; continuous
knob-drag sessions are specified separately by [0015](0015-continuous-editor-gestures.md).

Queued edits carry restore generations. Drop stale unstarted edits and close
stale already-begun gestures without applying their values. Defer edits when the
audio DSP has not yet synchronized a newly restored generation. Reflect accepted
editor values independently of incoming host-stream validation; host automation
then runs with normal event offsets. A newer restore wins over either reflection.
No audio retry loops, event-buffer allocation or callback logging are introduced.

Publish successful block meters through the existing separate coherent mailbox.
Rust editor code reads requested state and telemetry separately. A meter API is
not a standard CLAP meter extension, and no graphical embedding is implied.

## Consequences and evidence

The host output buffer can stall an edit, leaving a gesture open until later
callbacks accept end; queue overflow rejects complete new edits. Lifecycle
requires exactly one main-thread producer and one serialized callback consumer.
No delivery deadline is guaranteed when a host refuses output or stops callbacks.
Fewer than 2^31 restores are permitted across a callback or outstanding edit.

The [state guide](../session-state.md) and [control guide](../editor-control.md)
record ordering, record formats, bounded I/O and tests. Unit tests cover wrapping,
backpressure, stale edit handling and concurrent queue handoff; host ABI tests
cover complete recall and bypass automation; mock callback heap probes cover
control edits and meters. Executed bundle/validator evidence belongs in
[work status](../WORK_STATUS.md). Separate REAPER session/automation evidence is
recorded in [host validation](../host-validation.md); no native deadline claim.
