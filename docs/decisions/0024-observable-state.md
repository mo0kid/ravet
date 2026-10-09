# 0024 — Transactional control-thread state with bounded snapshots

Status: accepted, 9 October 2026.

Use an explicit allocating crate for general hierarchical state. Keep portable
parameter schemas and coherent audio snapshots independent. Nodes retain
product-owned stable IDs; all edits pass through a shadow transaction so invalid
later operations cannot partially mutate state or destroy redo.

Use complete bounded snapshots for grouped undo/redo and a bounded revision
journal for polling subscriptions. Overflow requests resync. Avoid synchronous
listener callbacks and reentrancy; consumers choose delivery scheduling. Snapshot
cost is deliberate and documented, rather than hidden in an audio callback.

Use a deterministic typed versioned binary preset with product identity. Decode
constructs separately and live restore becomes one undoable edit. No product
path, authorization, host program policy or cross-thread locking enters the store.
See [the guide](../observable-state.md) for capacities and meaningful tests.
