# Observable state, undo and presets

The explicit `ravet-state` crate owns hierarchical control-thread state. It is
outside the portable processing umbrella because transactions allocate and clone
snapshots. Send processor-specific validated values through a bounded bridge;
never access this store inside an audio callback.

Nodes have product-owned stable u64 IDs, UTF-8 names, sorted properties and ordered
children. Property values include null, booleans, i64, finite f64, text and bytes.
Immutable access prevents changes outside the transaction machinery. Builders
are checked when admitted to a store. Duplicate IDs, empty/oversized names/keys,
nonfinite properties and structural/capacity errors are rejected.

Commit accepts a sequence of property edits, inserts, removals, moves and complete
root replacements. It applies edits to a shadow, checking intermediate bounds,
then publishes one revision. Removing the root and moving a node into its own
subtree fail. A failed or net no-op transaction preserves the tree, revision,
undo/redo and journal. Child move indices are measured after source removal.
Snapshots provide grouped undo and redo; a successful new branch discards redo.

Subscriptions poll complete revision batches. A lagging subscriber gets a current
root resync rather than silently missing edits. References from a poll borrow
the store, preventing mutation until released. Cursors belong to their original
store. Polling avoids synchronous user callbacks and reentrant commits. Thread
scheduling, OS notifications and cross-thread delivery are consumer responsibilities.

Limits are selected at store creation: up to 4096 nodes, depth 64, 16 MiB encoded
state, 128 undo snapshots and 128 journal batches, with at most 4096 transaction
operations. Transaction payload totals also respect the byte limit before copy.
The defaults use maximum tree limits, 64 undo snapshots and 64 journal batches.
History is bounded by count; large states multiply memory/clone cost by retained
snapshots. Choose smaller limits for large documents. This is control-thread
correctness, not a performance guarantee.

Version-one `RVETTREE` records contain a caller-selected eight-byte product ID,
complete deterministic tree and typed little-endian values. Decode bounds input,
recursion and allocation, rejects malformed/truncated/trailing data and constructs
a separate validated store. Live restore is one undoable transaction. Presets
exclude DSP history, undo history and subscription cursors. File paths/discovery,
version migration, program selection and generic host registration remain separate
work; use the existing parameter schemas and runtime file services as appropriate.

Tests exercise grouped atomic edits, stable-ID moves, failed branch preservation,
undo/redo, bounded history, journal overrun, complete preset recall, all truncated
record prefixes, hand-built malformed records, duplicate IDs/keys and capacities.
See [decision 0024](decisions/0024-observable-state.md).
