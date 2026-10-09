# 0004 — Prepared DSP primitives and private consumer boundary

Status: delay storage, feedback diffusion, and fixed 2x oversampling implemented;
public APIs provisional. Proprietary consumer products remain external.

## Context and decision

More substantial effects require reusable storage and sample-rate conversion
without introducing product code into the framework. Add three small independent
primitives to `ravet-signal`: `DelayLine`, `DelayDiffuser`, and `HalfBand2x`.
The portable crate has no host, device, GUI, or external runtime dependency.

Allocate delay storage during preparation. Processing advances caller-owned
state one sample at a time. Reset clears storage in place. Fractional delay
uses double-precision linear interpolation. A nonfinite delay reads zero;
finite delay clamps to the prepared storage range. The caller remains
responsible for stable filter coefficients and valid audio values.

Whole-sample `read_integer` clamps an integer distance to the same storage
range and uses integer cursor arithmetic without interpolation. The diffuser
uses this path for its prepared delay. Fractional reads split the clamped
distance into integer and fractional parts, wrap the integer cursor difference,
and interpolate its adjacent samples. Arbitrary nonzero storage lengths remain
supported. Repeated wrap tests compare integer and
fractional reads, including oversized integer distances.

The diffuser implements `y = delayed - g*x; write = x + g*delayed`. It is named
as a feedback diffuser rather than advertised as a canonical unity-magnitude
allpass. Its test checks the specified impulse energy and reset recurrence.
On AArch64 and FMA-enabled targets, each multiply-add uses one rounding; other
targets retain separate operations. Double-precision rounding can differ by
target, while the recurrence and prepared storage remain the same.

The 2x profile uses f32 polyphase allpass interpolation/decimation, with fixed
coefficient data (up transition width 0.05, -90 dB stopband; down width 0.06,
-75 dB). The implementation evaluates the standard first-order allpass transfer
function. It has no coefficient designer or external framework source dependency.
It adds frequency-dependent delay and supplies neither dry alignment nor host latency
compensation. No per-block tiny-value snapping is applied, so block partitioning
does not change stream state. No native denormal or deadline claim is made.

## Ownership and evidence

Only these reusable primitives and their generic numerical tests live here.
Proprietary products, assets, comparison fixtures, licences, and wrappers belong
in separate private projects. See [publication policy](../publication.md) for
the additional history review required before making the repository public.

Generic tests cover delay wrap/interpolation/reset, feedback-diffuser impulse
energy, oversampling DC preservation, high-rate Nyquist rejection, and reset.
Consumer-specific comparisons and allocator workloads are private evidence;
they do not establish a framework platform or host support matrix.
