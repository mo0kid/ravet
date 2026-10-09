# 0057: Explicit event graph scheduling and faults

Keep default input broadcast unchanged, but allow inactive graphs to select
explicit filtered event connections. Source/destination endpoints are host or
stable nodes. Audio and event edges share topological ordering; combined cycles
and missing IDs reject before edits. Repeated connections duplicate messages.

Prepared input/output/host event buffers share the 64 MiB graph scratch budget.
Each input and writer uses the configured maximum (at most 1024). Stable bounded
insertion merges by offset without allocation, preserving connection/source order
for ties. Event offsets are not implicitly delayed by audio latency compensation.

Overflow can occur after earlier node execution. A distinct EventFault records
route source/destination, clears audio/generated host output and disables the
graph until reset. Host events are published only after the full merge succeeds.
This preserves explicit nonrollback graph semantics rather than claiming the
Processor atomic fault contract. State/timing alignment remains consumer policy.

Tests exercise generated chains, filters, combined ordering/cycles, equal frames,
zero frames, duplicate overflow/reset, failed edits, opaque UMP/native f64 and
10,000 heap-free callbacks. Native host/device deadlines remain unvalidated.
