# 0117: Buffered graph-worker output ownership

Status: prototype within the explicit no-input f32 queue contract.

## Decision

Keep a borrowed non-Send graph audio session on a dedicated consumer-owned worker
and publish prepared interleaved blocks into the existing bounded SPSC queue.
The device callback uses a separate RingOutput processor, which consumes only
queue frames and silences/counts underflow. Do not assume a device callback's
thread or native teardown can own graph stop/deactivation.

Prepare graph/scratch on control; move prepared storage to the worker, enter its
session, pump bounded blocks and finish there. Retain partial queue suffixes
without rerendering. Return stopped storage to main, retire the graph and reclaim
separately held native controls. Worker wake/priming, clock/rate matching, native
stream pause/drop and worker join stay explicit. Pump never waits or creates a
thread. Queue progress/counters do not measure hardware playback/latency.

## Evidence and limits

Tests exercise retained partial/full backpressure, order, heap-free pump/consume,
invalid dequeue preservation, underflow silence, graph faults/explicit reset and
worker stop/main retirement. A public loaded sine probe checks silent native graph
processing through the queue and offline/device renderer. No input/events/dynamic
editing/automatic recovery, resampling, device clocks or deadline guarantees.
Scratch limits are separate from graph/native memory caps; queues hold additional
prepared memory. Explicit reset preserves pending/queued audio rather than
implying an atomic audible seek. Native/platform evidence must be recorded as
executed validation, separately from portable tests.
