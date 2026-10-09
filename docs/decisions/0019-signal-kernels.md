# 0019 — Prepared numerical signal kernels

Status: biquad, complex FFT, direct FIR convolution selectable oversampling and linear ADSR
implemented; the broader DSP programme is incomplete.

## Decision

Keep these kernels in `ravet-signal` with no new runtime dependencies. Allocate
FFT twiddles/FIR history at construction and require caller-owned transform
buffers. Validate sizes/designs before mutation. Use double precision for
biquads/FFT/FIR and retain the established f32 half-band profile for selectable
1x/2x/4x/8x oversampling. Consumers own per-channel state and high-rate DSP. ADSR uses validated rounded
sample durations, exact linear endpoints and current-level retrigger/release;
voice routing and sustain-pedal handling remain consumer responsibilities.

Use a simple radix-two FFT and direct convolution as independently testable
starting points. Direct convolution remains O(taps) and is not a long-response
performance claim. Filter designs validate finite parameters and strict Jury
stability; live coefficient smoothing is left to consumers. Cascade oversampling
preserves temporal order and fixed filter history without heap scratch.

## Consequences and evidence

There is no SIMD, real-packed FFT, partitioned convolution, coefficient crossfade,
arbitrary-ratio resampler or integer latency compensation. Nonfinite audio
propagates. Tests compare independent DFT/convolution/impulse calculations and
response landmarks, and exercise reset/partition invariance. A shared allocation
counter verifies 1,000 prepared workloads including resets. These tests establish
numerical behavior and framework heap discipline, not deadlines or host support.
See [signal guide](../signal-processing.md) and [work status](../WORK_STATUS.md).
