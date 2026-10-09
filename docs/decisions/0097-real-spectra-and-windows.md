# 0097: Prepared real spectra and explicit cosine windows

Status: implemented scoped real half-spectrum interface/window families.

## Decision

Reuse the existing complex transform with owned complex scratch for real
forward/inverse APIs. Return N/2+1 bins, reconstruct the remaining bins by
conjugation, normalize inverse by N. Reject lengths and nonzero/nonfinite
imaginary DC/Nyquist before output mutation; forward explicitly zeros endpoint
imaginary values. Other nonfinite data propagates. N=1 has only DC.

Prepare separately owned cosine windows, explicit symmetric/periodic convention
and actual sampled coherent gain/energy/noise bandwidth. Keep FFT/window policies
separate and perform no peak/amplitude correction automatically. N=1 is unity;
exact zero coefficient sum has no equivalent noise bandwidth. Tiny rounded
negative taps remain. Size bounds 1..1048576 apply to both prepared mechanisms;
FFT additionally requires powers of two. Backend remains full complex FFT, not
an optimized real-only implementation. No overlap/add/SIMD/deadline evidence.

Window definitions reference the official [SciPy window documentation](https://docs.scipy.org/doc/scipy/reference/signal.windows.html).
No SciPy source or dependency is incorporated.

## Evidence

Three groups compare half spectra against direct DFTs, inverse/Parseval and
independent DC/interior/Nyquist reconstruction, check window landmarks/conventions,
analytic periodic gain/noise bandwidth and corrected coherent-bin amplitude,
atomic rejection, tiny/singleton/nonfinite policies and prepared heap-free loops.
The first compilation exposed a mutable index/immutable size borrow; storing
the mirrored index separately resolves it. Native timing remains unvalidated.
