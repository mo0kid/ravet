# 0270: Native-precision convolution schedules and bounded banks

Status: accepted; implemented in `ravet-signal` with offline numerical, allocation and Graph-consumer tests only.

## Context

`PartitionedConvolver` provides one f64 channel with uniform partitions and exactly one partition
of latency. It has no zero-latency head, no nonuniform schedule, no f32 path and no multichannel
owner. `FftPlan` is f64 only and its constructor cannot report a storage bound.

## Decision

Add separate fixed engines `Convolver32`/`Convolver64` and diagonal banks
`ConvolutionBank32`/`ConvolutionBank64` with an immutable `ConvolutionPartitioning`:
`UniformDelayed{partition}`, `ZeroLatency{head}` and two-stage `NonUniform{head, tail}`.
Each FFT stage's intrinsic partition delay equals its original tap offset, so the zero-latency
schedules need no alignment FIFO. Partitions are powers of two: head 16..=4096, tail
head..=16384. Responses are 1..=262,144 finite taps. Empty intervals are omitted.

Arithmetic runs in the named sample precision throughout. Add `Complex32`/`FftPlan32` as a
separate type rather than making `FftPlan` generic, so the f64 plan's layout, results and
constructor remain unchanged. Twiddles are computed in f64 during preparation and rounded to
f32 explicitly. Add `FftPlan::try_new` and `FftPreparationError` additively; `FftPlan::new`
keeps its infallible allocation policy and error contract.

Preparation admits schedule, response, maximum block (1..=65,536) and a numeric byte budget
(1..=256 MiB, aggregate across a bank of 1..=64 channels) before allocating; storage is reserved
fallibly and actual capacities are rechecked. Processing and reset never allocate. Whole calls
(and whole banks) are refused before any history or output change.

`ravet-signal` keeps no runtime dependency: no public signal type implements the Foundation
`Processor` trait. A test-support adapter (`tests/support/convolution_processor.rs`, through the
existing dev-dependencies) proves Foundation/Graph use with exact latency and tail. Consumers
needing per-frame channel access use `engine_mut`, which keeps per-engine admission.

## Consequences

- `UniformDelayed` f64 matches `PartitionedConvolver` bit-for-bit; that type stays as is.
- Native f32 results differ from f64 within measured, recorded rounding; no gain correction.
- Direct-head work is O(head) per sample; FFT work is bursty. No deadline claim.
- Max-length f64 NonUniform{16,16384} uses about 17.96 MB, so the 256 MiB cap limits such banks to
  14 channels (29 for f32).

## Not decided here (separate follow-ons)

- Source-rate IR preparation: file/memory readers, sample-rate conversion, trim, normalization.
- Live transitions: worker preparation, owner transfer, same-latency crossfade and retirement.
- True-stereo IR matrices, real-FFT packing, SIMD kernels and optimal
  arbitrary nonuniform schedules.
