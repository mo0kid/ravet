# 0290: Stereo block FIR and oversampling kernels

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
  test machine; the original oversampler measurement costs about 3.3× less than
  before. The external evidence below supersedes its original lack of an
  external baseline. Native Intel measurements remain absent.
- Direct convolution stays O(taps); long responses should use
  `PartitionedConvolver`.

## External baseline evidence — 5 November 2026

The follow-up scratch harness uses Apple's external `vDSP_biquadm` implementation
for the same rounded halfband IIR coefficients and polyphase/stage topology.
Pairs of first-order allpasses form two biquads per branch; four channels carry
the two stereo branches. This is a library-kernel baseline with original
oversampling glue, not a claim about all independent resampler designs. The
harness and all comparison implementation code stay outside the repository.
No Accelerate dependency or DSP source change is introduced.

With 512-frame f32 stereo blocks and identity processing, median nanoseconds per
host stereo frame (nine × 4,000 blocks, 1,000 warm-ups) on an Apple M4 Pro,
macOS 27.0.1, Rust 1.99.1 release and Apple Clang 21.0.0 `-O3`:

| Factor | arm64 RAVET / Accelerate | Rosetta x86_64 RAVET / Accelerate |
|---:|---:|---:|
| 2× | 4.658 / 31.301 | 5.241 / 31.886 |
| 4× | 13.481 / 95.794 | 15.183 / 96.304 |
| 8× | 31.137 / 218.174 | 34.704 / 226.529 |

Quality is matched: per-stage up pass/stop bands 0..0.225 / 0.275..0.5 and down
0..0.22 / 0.28..0.5, in cycles per high-rate sample. Float64 FFTs of 65,536-frame
f32 impulse executions measured RAVET up/down ripple 6.29e-7/8.67e-7 dB and
stop rejection 93.261/85.261 dB on both architectures. Accelerate arm64 measured
1.83e-6/2.54e-7 dB ripple and 93.261/85.262 dB rejection; Rosetta measured
1.21e-6/1.18e-6 dB and 93.267/85.261 dB. These finite-grid figures include runtime
rounding, unlike the ideal-coefficient response. Continuous random streams
agreed within 9.83e-7 upsampled and 3.69e-7 round-trip over 80 blocks per factor.

The cost is competitive against this matched external baseline: RAVET is
6.7–7.1× cheaper on arm64 and 6.1–6.5× cheaper under Rosetta. The release §4 cost
item can be checked within this measured scope. This does not qualify native
Intel performance, all oversampler implementations, nonlinear aliasing, or
callback deadlines. See the [full method and quality table](../signal-processing.md#external-oversampler-baseline--5-november-2026)
and [source/hash receipt](../WORK_STATUS.md#release-010-macos-architectures).
