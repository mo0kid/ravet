# 0056: Prepared graph handoff and retirement

Whole-graph editing must not rebuild or destroy owning graphs in audio callbacks.
Pair a prepared active graph with bounded pending and retirement SPSC queues,
using pinned MIT-or-Apache-2.0 rtrb 0.4.0. Control offers prepared graphs with the
exact initial config and latency, retaining ownership on failure. One FIFO graph
may be selected per valid block; invalid input consumes none.

Audio returns old graphs to control rather than dropping them. Retirement
backpressure prevents another replacement while the active graph continues.
A defensive retained slot also prevents destruction on unexpected queue refusal.
Dropping control disables replacements. Both endpoints are destroyed outside
callbacks. Control retirement drains at most queue capacity per call, including
when audio concurrently retires more graphs. Aggregate in-flight/node memory
is consumer-budgeted; every graph retains its own 64 MiB scratch cap.

Graph state transfer, old-tail draining and crossfades are not inferred. New
history is the proposed prepared history; config/latency remain fixed. This
closes explicit whole-graph handoff without changing the static graph's explicit
nonrollback fault contract or admitting feedback/per-node event routes.

Tests cover FIFO/backpressure/invalid-input/control-loss, rejected ownership,
config mismatch, f64 precision, destructor thread probes, 10,000 allocation-free
callback reuse cycles and separate control/audio thread transfers. Native
deadline/host/device evidence remains separate work.
