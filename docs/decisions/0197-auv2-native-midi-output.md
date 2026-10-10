# 0197: Bounded AUv2 native MIDI output delivery

Status: prototype; strict normal/fast-math helpers, authoritative native acceptance,
synthetic in-process AudioComponent contracts, the normal public Performance AU
package and the existing Gain component regression pass locally.

A separate reusable SDK-independent converter maps exact canonical MIDI1 short
records and ordinary nonidentity notes. Raw messages retain exact bytes; notes use
nearest/ties-up MIDI7 velocity with positive On minimum 1 and exact-zero On wire 0
(MIDI receiver Off convention), matching current CLAP MIDI policy. Canonical field,
finite/exact-f32 scalar, defined status/length/data and active-slice checks precede
publication. Identity/scoped notes, completion/choke, expressions, parameters,
UMP, SysEx and unsupported tags reject without inventing semantics.

A fixed external CoreMIDI packet owner stages at most 1024 native events into one
complete <=64KiB legacy packet list. Whole-prefix validation precedes callback
publication. Source timestamps are ordered sample offsets, not host mach time,
with same-frame source order retained through optional packet coalescing. The
caller supplies exact render timestamp, output index and callback; output routing
has no automatic audio-bus association. No recursive flush, allocation, logging,
protocol negotiation, property registration or output-name policy is added.

Fresh staging invalidates readiness before validation, while rejection retains
previous diagnostic bytes/count. No old-block accidental delivery follows failure.
Every delivery attempt consumes readiness before invocation. One host callback
borrows complete data; reentrant stage/delivery rejects without changing bytes,
copy/move is disabled and the owner must remain alive through the callback.
Nonzero host status reports attempted records and exact error, with acceptance
unknown because host prefix effects may already be irreversible. No replay,
retry or audio/history rollback follows. Empty output needs no callback; missing
callback/time cannot leave a deliverable stale block. Host callback exceptions,
concurrent mutation, ownership/lifetime and blocking behavior remain host duties.

Local AudioToolbox legacy setters normalize original sizes/elements and install
an internal protocol adapter; subsequent setters can bypass the owned SDK method.
Raw factory-selector exact size/scope/element/stopped lifecycle validation therefore
does not promise facade rejection/mutation atomicity. Fixtures explicitly separate
these boundaries and use valid serialized callback ownership during actual Render.

Public Performance continues to advertise no MIDI output and suppress completion.
A separate synthetic test consumer owns output metadata/callback properties and
generates controlled native records after a real public native processor call.
Actual in-process AudioComponent registration/render validates borrowed MIDI data,
retained CF metadata, copied callbacks, refusal receipts/no retry, pre-DSP late
faults and recovery. This establishes neither a new distributed factory nor Rust
Performance MIDI generation. Real native full-identity NoteEnd output is explicitly
unrepresentable and produces no MIDI callback.

Normal and optimized-fast-math strict fixtures cross-check supported canonical
records against real Rust native ingress and check late faults/previous output,
exact unusedzero/detail/identity/scalar limits, maximumpacket lists, reentrancy and
stale delivery. 2000 maximum accepted/late-fault cycles show zero counted ordinary
C++ heap operations, not all CoreMIDI/native allocations or deadlines. SDK sources
remain external; no DAW routing, device, other architecture, UMP/SysEx, production
factory or platform/deadline support follows from offline macOS arm64 evidence.
