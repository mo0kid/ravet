# 0121: Explicit global parameter gesture boundaries

Status: implemented portable routing and prototype CLAP mappings.

## Decision

Represent begin/end as typed `ParameterGesturePhase` carried by stable-ID
`EventData::ParameterGesture`. Boundaries carry no value, implicit state change,
owner or paired-edit invariant. Receivers explicitly decide pairing, cancellation,
recording and acknowledgement. Parameter graph filters include boundaries for the
same ID; performance filters exclude them. Complete worker queues preserve them
without inferring an edit scheduler.

Generated CLAP output maps boundaries without MIDI ports and preserves whole-stream
representability preflight and host prefix rejection. Hosted graph output is
separately opted into with `with_parameter_gesture_output(true)` and requires an
explicit global parameter output mapping before activation. Disabled, unmapped
or invalid native IDs filter; mapped read-only boundaries reject. Shared offset,
order and capacity faults clear the block with nonrollback backend failure.
Zero-frame parameter flushes allow offset-zero boundaries.

## Evidence and limits

Portable graph filters, queue round trips and generated CLAP prefix tests accompany
linked f32/f64 hosted fixture tests and loaded public gesture fixture graph probes.
Tests cover disabled filtering, repeated ends, same-frame ordering, read-only
rejection, overflow, zero-frame flush, live swaps and heap-free processing.
No automatic gesture pairing, GUI edits, host automation recording, native input
translation or other-format mapping is claimed.

Graph rendering continues to cap each direction at 1024 events, now exposed as
`ravet_graph::MAXIMUM_GRAPH_EVENTS`. The graph-worker constructor checks that cap
before preparing nodes. Standalone event queues may hold 4096 events; callers must
use batches fitting their destination graph. Earlier worker prose stating 4096
usable render events was incorrect; this is a contract clarification, not a cap
increase.
