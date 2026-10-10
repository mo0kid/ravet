# 0156: Explicit prepared native telemetry

Status: implemented; Rust and linked macOS arm64 C/C++ validation passed.

## Context

Generic native processor consumers need portable readouts without accessing
product-specific DSP handles. Readouts are observations, separate from saved
parameter state; f32 normalized controls cannot preserve extreme f64 levels or
exact u64 window counts. Cross-thread UI publication is a separate responsibility.

## Decision

Add optional `NativeTelemetryProcessor<N>` facade hooks and an independent
`NativeTelemetryEntryV1`. Control providers declare immutable full-u32 IDs,
UTF-8 names/units and finite ordered f64 display hints. A control-created cache
preflights <=1024 descriptors before heap allocation, copies native metadata and
builds a sorted ID lookup. Shared metadata reads use no provider callbacks.

The exclusive prepared audio owner returns a complete fixed-size array of
ID/flags/f64-value/u64-window records. Whole known/unique ID, finite observation and
known-flag validation precedes foreign writes. Values reorder to declared order,
failed fields and successful tails survive. The nonfinite-samples flag describes
input samples, while observations themselves stay finite. Display bounds are hints,
not clipping; extreme/subnormal/signed-zero f64 and exact u64 counts are preserved.
Consumer window, retention/reset and measurement semantics remain explicit.

Snapshot hooks obey no heap actions, blocking or I/O. Normal framework validation
uses bounded stack and cached storage, without providers, codecs, locks or heap.
No atomics are required. Exact processor/cache/table/precision/library pairing and
foreign live/aligned/disjoint ranges remain caller duties. Snapshot panic shares
processor poison until fresh stopped preparation, without consumer rollback.
Control provider failures return None/null, isolating existing caches/owners.

## Limits and consequences

No state persistence/automation, generated events, implicit cross-thread DSP
reads/publication, clear, smoothing, decay, loudness/true-peak policy, host/UI
notifications or scheduling is added. Gain facades expose 12 stable readouts
(10..21): peak, RMS and nonfinite indication for two input/output channels,
matching the existing SDK consumer fields. f32 amplitudes widen exactly, while
f64 retains native levels. The public Gain DSP keeps its existing measurement
window and zero/rejected block retention. SDK display normalization is consumer
policy. Tests/actual-library evidence are recorded after gates; no DAW, device,
additional target, deadline or production signing claim follows.

Equivalent exported/paired immutable tables may have distinct addresses after
Rust constant promotion. Table address is not a type or owner identity; native
contracts verify callback/precision profiles, with exact originating facade/cache/
library pairing still a caller obligation.
