# 0088: Fixed cascades and even-order Butterworth designs

Status: implemented; portable numerical/heap evidence.

## Decision and scope

Use fixed const-generic arrays of 1..16 validated biquad sections. Immutable
CascadeCoefficients provides explicit stages and order-2N Butterworth low/high
pass design; mutable BiquadCascade owns each stage's independent history.
Butterworth pole-pair Q values progress from low to high Q and reuse existing
bilinear stable biquad constructors. No high-order polynomial is formed.
[Reference poles](https://github.com/scipy/scipy/blob/main/scipy/signal/_filter_design.py)
and [cutoff semantics](https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.butter.html)
are documented by SciPy; no source/dependency is included.

Preparation rejects invalid capacity/rate/cutoff or unrepresentable stage
stability. Replacement preserves all delay states without smoothing. Arbitrary
stages retain caller-owned headroom/product-overflow policy; nonfinite/denormal
handling, odd-order/band designs, automatic order selection and other families
are outside scope. Sample/reset/replacement use fixed storage.

## Evidence

All orders 2..32 and both passbands pass independent bilinear-warped analytic
response grids, cutoff/endpoints, rendered -3 dB RMS, partition identity, reset,
changed-coefficient history against independent processors, invalid settings
and heap counts. This does not prove every numerical corner or host deadline.
