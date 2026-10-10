# 0149: Explicit sample-wise waveshapers

Status: portable custom f32/f64 transfer functions and f64 clipping curves implemented.

Store the caller's function inline in `Waveshaper<F>`, with matching input/output
sample types and no boxing, dynamic dispatch or sample conversion introduced by
the wrapper. Use `Fn`, with shared access: the wrapper introduces no mutable
sample history, reset requirement, sample-rate setting, latency or tail. Any
interior state/captured resources and their lifecycle belong to the consumer.
Processing calls the function once per sample in order. Separate buffer lengths
preflight before all calls/writes; empty buffers do nothing.

Provide f64 hard-clipping, hyperbolic-tangent and cubic-soft-clipping constructors.
Hard clip clamps to +/-1; tanh has unity small-signal slope and asymptotic +/-1.
Cubic uses x - x^3/3 with knees at +/-1 and constant endpoints +/-2/3, preserving
unity slope at zero instead of implicit peak normalization. Clamp before cubic
arithmetic to bound extreme finite/infinite input. Built-ins propagate NaN.
Custom functions receive nonfinite values unchanged and define their own policy.

Keep drive/output gain, smoothing, DC removal and oversampling explicit. The
existing f32 oversampler can wrap custom f32 curves; no f64 oversampling engine is
introduced. Nonlinear shaping may alias. Wrapper construction/processing have no
heap actions, but captured functions remain responsible for allocation, blocking,
I/O and numerical behavior. No dependency or alias-free/deadline claim is added.

Offline tests check independent curve landmarks, symmetry/monotonicity/knees,
extreme/nonfinite values, native precision, captured controls, once-only ordered
calls, sample/in-place/separate and split-block identity, atomic mismatched/empty
buffers, explicit oversampling composition and heap-instrumented workloads.
