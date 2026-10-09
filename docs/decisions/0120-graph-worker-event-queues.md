# 0120: Complete event batches across graph worker/control owners

Status: implemented within explicit bounded SPSC ownership.

## Decision

Use fixed-size batches in the pinned rtrb queue, committing all header/context/tag,
validated events and unused slots atomically. Bound frames/events/queued batches
at preparation and allocate one retained consumer buffer. Never expose a partial
input prefix or replace an unacknowledged retained batch.

Return immutable borrowed batches until explicit acknowledge after delivery or
deliberate discard. Dropping the borrow retains values. Preserve caller tags and
transport without inferring uniqueness/clock mapping or native context validity.
Use the same value queue for input and output under separate producer/consumer
policies; no event replay, retiming, hidden wait/wake or automatic scheduler.
Queue publication and acknowledgement themselves remain heap/lock/I/O free.

## Evidence and limits

Tests cover retained tags/context/MIDI/UMP/same-frame order, empty batches, full
and malformed rejection without prefixes, disconnect, and 10000 cross-thread
heap-instrumented batches. Loaded public native parameter fixture runs 1000
control/worker input-output rounds with exact telemetry and retained audio suffix
checks. No GUI/device/MIDI hardware/clock/deadline support claim. Queue capacity
is 1..64 batches plus one separately retained consumer batch; scratch/queue memory
is distinct from graph/native caps. Consumer pacing/drop/recovery and endpoint
shutdown/join remain explicit.
