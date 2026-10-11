# 0233: Owned VST3 controller parameters and timed DSP input

Status: Accepted contract for the explicit caller-linked prototype. Integration
and validation receipts belong in work status, separately from this decision.

## Context

Controller state and component DSP parameter input are distinct operations. A
controller setter can mutate before refusing, and a plug-in can retain a changes
interface or an individual per-ID queue after processing. Borrowed caller buffers
and stack queue objects cannot meet that retained interface lifetime.

## Decision

Keep existing provider and state ABIs and add an exact-pointer-paired parameter
companion. Main-only exclusive stopped control publishes complete owned bounded
catalogues, reads/converts text and values, and edits controller state. Timed
component delivery is an explicit separate operation on the scoped audio owner.
Failed foreign mutation and malformed edit admission invalidate preparation;
refresh/reprepare is explicit reconciliation without side-effect rollback.

Admit whole batches before effects, bounded to 2048 points and 64 writable IDs.
Global offsets are nondecreasing and per-ID offsets strictly increase. Cross-ID
ties follow the SDK's independent-queue semantics. Each core owns at most 16
prepared pools/64 MiB; each pool has 16 immutable fixed batches with permanent
host references to its changes object and every individual queue. AUDIO allocates
no queue storage and reads data only during its admitted synchronous thread phase.
Freeze and sticky protocol diagnostics share a coherent atomic phase.

Reuse requires every foreign lease gone. Foreign release never frees host payload.
Joined CONTROL end pins the exact already-reserved core before facade teardown
when retention was observed. CONTROL reclaims only detached host-only pools;
later release does not automatically unpin. Preserve original returned SDK results,
exceptions, owned synthetic diagnostics and queue faults as separate facts.

Core keeper geometry is shared across children. All mutable CONTROL operations
require every module audio capability joined. Already-prepared children can begin
independent scopes; begin/end alone are boundary exceptions and cannot reclaim or
grow shared geometry. Existing failed-end exceptional retention remains in force.

## Consequences

The contract provides bounded reusable host automation input and explicit failure
ownership. It does not synchronize controller edits automatically, record host
automation, expose program banks from flags, isolate native code, or establish
DAW/platform/deadline support. Events, output changes, transport, editors, f64,
auxiliary buses and graph consumption remain independent capabilities. See the
[parameter guide](../vst3-parameters.md) for exact admission and lifetime limits.
