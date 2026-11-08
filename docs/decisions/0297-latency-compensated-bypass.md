# 0297: Latency-compensated dry signal for plug-in bypass policies

**Date:** 6 November 2026  
**Status:** Implemented in `ravet-plugin` (`PolicyProcessor`,
`Product::with_maximum_latency`). Evidence: offline unit tests only; no host,
DAW or deadline qualification. Amends the bypass paragraph of
[0282](0282-plugin-policies-and-native-table.md).

## Context

Decision 0282 left the dry path of `BypassPolicy::Hard`, `Crossfade` and
`Frozen` undelayed. A product that reports latency `L` has the host delay every
other track by `L`, so its bypassed output arrived `L` samples early, and Hard
and Crossfade switched or mixed between two signals `L` samples apart (comb
filtering during a crossfade). Products had to keep `BypassPolicy::Processor`
to avoid this.

## Decision

- Every policy except `Processor` passes the dry input through a delay line per
  output channel. The delay is `Processor::latency_samples()`, read at the start
  of each block, so the dry signal lines up with the processed signal and the
  host's compensation.
- The delay is always fed while the processor runs, so engaging bypass
  continues the signal without a gap. With zero capacity nothing changes from
  0282: no delay line, and Frozen still uses `AudioBuffer::pass_through`.
- Capacity is fixed at `prepare` (processing never allocates): the larger of
  the latency reported after the inner `prepare` and the product's
  `with_maximum_latency`, at most `MAXIMUM_BYPASS_LATENCY` (65,536 samples).
  Both adapters apply cached parameters and state *after* `prepare`, so a
  product whose parameters or state can raise latency must declare the maximum.
  Latency above capacity is compensated only up to capacity until the next
  `prepare`. The adapters already request a host restart on a latency change.
- A declared maximum above the limit is rejected as
  `ProductError::InvalidBypassPolicy` (reused, so the error enum gains no
  variant). It is ignored with `BypassPolicy::Processor`.
- Reset clears the delayed history together with the processor's.
- A change of latency while active moves the dry read position at once, which
  may click while bypassed. The host's own compensation changes at the same
  time, and a smooth retiming would need a second crossfade.

## Alternatives considered

- **Resize after the adapters apply cached controls.** This is exact without a
  declaration, but it needs mutable access through `MainBusBridge` and
  `DenormalProcessor` in both adapters. We chose the declaration because it
  keeps the change inside the policy layer.
- **A fixed large capacity for every product.** This wastes up to 16 MiB per
  instance for the many products with no latency.
