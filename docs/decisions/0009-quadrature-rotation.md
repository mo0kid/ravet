# 0009 — Caller-owned quadrature rotation

Status: arithmetic kernel implemented; public API provisional.

Repeated scalar trigonometric calls can dominate modulated DSP. `PhaseRotation`
precomputes the sine and cosine of a finite signed increment in `-1..=1` cycles
and rotates caller-owned double-precision quadrature pairs or fixed banks. The
per-sample operation allocates nothing and does not quantize the increment.
Separate sine/cosine arrays permit compiler vectorization without reassociating
operations within a lane. AArch64 and FMA-enabled targets fuse the final
multiply-add; other targets retain separate operations. Rounding can therefore
differ by target. No architecture-specific dependency is required.

This kernel deliberately leaves phase accumulation, reset, automation, and
reanchor cadence to its consumer. Repeated rotations accumulate floating-point
error; consumers must periodically evaluate `sin_cos` at their independently
tracked phase. A frequency change may also require reanchoring. It is not a
bit-exact replacement for scalar trigonometry, and no global precision or
deadline guarantee is inferred from local measurements.

Generic tests exercise rejection, empty banks, signed increments, 16 independent
phases, and repeated reanchors against scalar `sin_cos`. Proprietary effect
comparisons and CPU measurements stay in the private consumer project.
