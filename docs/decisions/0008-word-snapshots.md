# 0008: Bounded coherent word snapshots

Status: portable mailbox implemented on targets with 64-bit atomics.

## Context and decision

Consumers with multiple parameters need complete control restores while the
processing thread continues. A scalar atomic cannot represent that transaction,
and a callback must not wait on a control-thread mutex. Provide a fixed-size
word mailbox with atomic storage, sequence reservation, and one-attempt reads
and publications. Conditional publication prevents stale audio reflection from
replacing a newer restore.

## Consequences and validation

Use sequentially consistent operations to make the coherence argument direct.
Contention is explicit and bounded; an audio consumer retains its previous
snapshot and retries at a later boundary. Control code owns retry/error policy.
The mailbox coalesces snapshots and is not an event queue or parameter schema.
Generation wrap must not occur across one operation. Tests cover stale updates
and two concurrent publishers without mixed generations. The existing scalar
gain export remains unchanged. See [snapshot guide](../snapshots.md).
