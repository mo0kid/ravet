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

`RVETTREE` records contain the magic, a caller-selected eight-byte product ID, a
u32 record format version, a nonzero u32 product schema version (format 2), then
the complete deterministic tree with typed little-endian values. `encode` writes
format 2 at schema 1; `encode_schema` writes a product-chosen schema. Decode
bounds input, recursion and allocation, rejects malformed/truncated/trailing
data and constructs a separate validated store. Live restore is one undoable
transaction. Presets exclude DSP history, undo history and subscription cursors.

Versions follow the [compatibility policy](compatibility.md#saved-state-and-presets).
Record formats other than 1 and 2 reject with `UnsupportedVersion`, and schemas
newer than the caller's current schema with `UnsupportedSchema`; neither is
reinterpreted. `read_header` reports both versions without decoding the tree.
Format-1 records written by RAVET 0.0.1 have no schema field and decode as
schema 1. `decode_schema` returns the saved schema for the caller to migrate,
and `restore_migrating` passes an older schema's decoded root to a product hook,
validates the result against the store limits and applies it as one undoable
transaction; a failing hook changes nothing. `decode` and `restore` accept
schema 1 only. See the [0.1.0 upgrade notes](releases/0.1.0-upgrade.md). File
paths/discovery, program selection and generic host registration remain
separate work; use the existing parameter schemas and runtime file services as
appropriate.

Tests exercise grouped atomic edits, stable-ID moves, failed branch preservation,
undo/redo, bounded history, journal overrun, complete preset recall, all truncated
record prefixes, hand-built malformed records, duplicate IDs/keys and capacities,
format and schema markers, rejection of newer formats and schemas without
touching live state, a hand-built 0.0.1 format-1 record, and atomic migration.
See [decision 0024](decisions/0024-observable-state.md).
