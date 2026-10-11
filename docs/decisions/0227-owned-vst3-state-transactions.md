# 0227: Owned stopped VST3 state transactions

Status: Accepted for the explicit caller-linked prototype. Local receipts and
integration status belong in the work-status log, not this contract record.

## Context

An initialized component may expose a separate or combined controller. Component
state and controller-own state are separate SDK operations, and a later setter
can refuse after the component has already changed. SDK streams are reference
counted foreign interfaces; stack streams or borrowed caller buffers cannot safely
outlive a synchronous call when the plug-in retains a reference.

## Decision

Keep catalogue-v1 and instance-v2 layouts unchanged and add an exact-pointer-paired
state companion. Safe Rust owns copied opaque snapshots and requires exclusive
stopped main access to a child borrowing its parent. Class UID/presence and bounded
sizes are checked; plug-in schema/version remains caller-owned trust.

Capture publishes only after mandatory component and supported controller-own
getters succeed. Controller absence, explicit SDK `NotImplemented` and successful
zero-length bytes remain distinct. Restore preflights all leases/copies and applies
component, independent controller-component and optional own-controller state in
that order. Success invalidates preparation; no implicit restart reconciliation.
Foreign setter failure poisons processing because rollback cannot be proved.
Joined stopped retirement remains available; fresh-child construction is recovery.
Malformed restore diagnostics fail closed while genuine preflight failure preserves
the previous profile. Original returned SDK results, exceptions and stream faults
remain separate facts.

Each real SDK stream owns fixed storage and a coherent lock-free admitted/fault
word. Cursor/data effects are main-only during one phase, then permanently frozen.
Reference counting and validated `QueryInterface` remain usable after freeze.
Native-core host keepers reserve at most 16 streams/64 MiB before foreign calls;
host-only frozen payloads reclaim on control, never on the releasing foreign worker.

Foreign stream retention converts the already-reserved core registry slot to
process-pinned ownership before parent retirement. This requires no new pin slot
or deferred capacity acquisition. Same-core v2 reopen and process-executable
lifetime retain existing pin contracts. Releasing all streams later permits
payload reclamation, not module unpinning. No SDK call occurs under the registry
lock and pinned cores have no static exit destructor.

## Consequences

The implementation gives bounded host storage and explicit partial-failure facts,
not isolation from arbitrary native access/side effects or a whole-plugin memory
bound. It does not make live restore safe, infer configuration changes, negotiate
parameters/events/editors/graphs, or establish DAW/platform/deadline support.
See the [state guide](../vst3-state.md) and
[instance guide](../vst3-instances.md) for executable linking and ownership.
