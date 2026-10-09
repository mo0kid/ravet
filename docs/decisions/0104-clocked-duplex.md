# 0104: Explicit clocked duplex conversion

Status: implemented bounded prepared adapter; native evidence is recorded separately.

The matching-rate FIFO adapter retains its existing behavior. A separate clocked
adapter owns the raw consumer, one variable sinc converter per input channel and
one FIFO-depth controller. Nominal input/output rates, target depth, correction
bounds and scratch size are explicit; no device settings or fallback change.
All channels use identical temporal progress and the processor prepares at the
output rate. An internal converted ring is drained immediately on the same
callback and reuses existing duplex processing/fault/time semantics.

Feedback counts raw queued plus prefetched input, capped at raw ring capacity.
Initial/recovery priming supplies intentional zeros until target depth. Underflow
supplies counted zeros, resets conversion/control history and reprimes, while DSP
history and logical output clock continue. Pause preserves all histories; reopen
resets them. Native callback counts must not count each scratch partition, so the
existing renderer exposes an internal block operation without changing its public
matching-rate behavior. Telemetry is independently atomic, not a coherent block.

Prepared tests cover signed nominal clock drift, variable native buffers, stereo
alignment, no steady underflow, correction equilibrium, priming/starvation/reprime,
malformed-frame rejection, one callback count per buffer, heap-free workloads and
invalid preparation before processor calls. A silent exact-ID native probe covers
three generations, pause/resume and control-thread lifecycle. Neither establishes
arbitrary controller stability, physical phase alignment, round-trip latency,
long-session hardware drift, hotplug/recovery or callback deadlines.
