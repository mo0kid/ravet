# 0015 — Continuous editor gestures

Status: prototype implemented for the gain CLAP control bridge and macOS editor.
Date: 8 October 2026. Extends [0012](0012-clap-complete-state-and-edits.md)
and [0013](0013-cargo-native-gain-editor.md).

## Decision

Retain complete committed edits, and add explicit begin/update/end commands to
the same SPSC queue. One main-thread continuous gesture may be open at a time.
Other begins and committed edits reject until end is enqueued. All commands carry
the generation at begin, so a restore cannot be overwritten by later movement
from an older drag. Valid live values apply only after the host accepts their
output event, with normal gain smoothing.

The queue remains fixed at 32 slots with at most 96 drain iterations per callback.
Begin and value commands use at most 31 occupied slots; the remaining slot
guarantees an end can be enqueued. Overflow rejects updates rather than silently
coalescing or overwriting entries. Accepted output events are never repeated.
No allocation, lock or retry loop is added to production enqueue/drain paths.

Consumer gesture ownership survives empty callbacks. After restore it emits end
for an old begun gesture, including an idle gesture, before continuing; stale
values and duplicate old ends are dropped. The producer ends its old gesture
before a new one starts. End backpressure retains ownership until accepted.
Fewer than 2^31 restores may occur across outstanding gesture ownership.

The gain page sends begin on pointer down (or the first input), values on input,
and end on change/release/cancellation/blur. The callback owner's destruction
enqueues end and requests host scheduling. Numeric and bypass edits remain
complete transactions. Rejected values are visible; end preserves a prior error.
The final pointer value is not guaranteed under overflow or a stalled host.

## Evidence and limits

Queue fixtures cover cross-callback live values, one-event output capacity, full
update queues with reserved end, idle restore cancellation and subsequent fresh
edits. A 1,000-gesture fixture checks zero Rust allocation/deallocation. The
built-editor fixture sends a value before release, verifies it reaches requested
state, then sends another value and end in a later flush.

This is a product control API, not a general framework attachment system. Delivery
depends on host callbacks and accepted output events; teardown cannot guarantee
a closing event reaches a host that stops scheduling. Native allocation, callback
deadlines remain unverified. REAPER 7.82/macOS arm64 GUI gain recording and audio
are exercised separately in [host validation](../host-validation.md); broader
host behavior remains unverified.
