# 0287: Stereo block FIR and oversampling kernels

**Date:** 5 November 2026  
**Status:** Implemented in `ravet-signal`; tested and measured on one Apple M4
Pro (macOS arm64), x86_64 tested under Rosetta 2 only.

## Context

The 0.1.0 plan asks for a fixed-size stereo FIR and oversampler path with
competitive per-block cost. `FirConvolver` processes one f64 channel per sample
with a modular ring, and `Oversampler` runs one channel per sample through a
closure. Both are correct but cost an order of magnitude more than optimised
libraries. The workspace forbids `unsafe`, so intrinsics are not available.

## Decision

- `StereoFir<S>` (f32, f64) stores `taps - 1` history samples followed by the
  current block in one buffer per channel. The kernel keeps a register block of
  running sums and broadcasts one tap at a time across them: lane-wise
  multiply-adds over adjacent inputs, with no horizontal reduction, which the
  compiler vectorises from safe code. Block widths are 64 (f32) and 32 (f64)
  on arm64, half that elsewhere, followed by 8-wide and single steps.
- Each output is summed sequentially from the oldest tap, so results do not
  depend on block partitioning. On arm64 the step uses `mul_add`, a single
  instruction there; elsewhere a separate multiply and add, because `mul_add`
  without hardware FMA is a library call. Results may differ in the last bits
  between arm64 and other targets.
- `StereoOversampler` keeps the existing fixed half-band coefficients and
  processes a block one stage at a time. Both polyphase branches of both
  channels form one four-lane vector with the same scalar operations as
  `HalfBand2x`, so its output is bit-identical to the per-sample path.
- Benchmarks use the repository's plain timing style (an example with
  `Instant` and `black_box`) rather than adding a benchmarking dependency.

## Consequences

- f32 FIR cost matches Apple Accelerate `vDSP_conv` at 256–1,024 taps on the
  test machine; the oversampler costs about 3.3× less than before. No external
  oversampling baseline or native x86_64 measurement exists yet.
- Direct convolution stays O(taps); long responses should use
  `PartitionedConvolver`.
