# 0243: Bounded elliptic poles, zeros and specification selection

Status: accepted contract; reviewed implementation integrated. Public qualification
is recorded separately in [work status](../WORK_STATUS.md).

## Context

An elliptic family requires genuine pass and stop ripple, finite zeros, parity
gains and complete-integral degree selection. A Chebyshev wrapper or a changed
cutoff label cannot provide that behavior.

## Decision

Add fixed16-section f64 EllipticDesign/EllipticFilter with LP/HP order1..32 and
band total even2..32/prototype half. Finite positive rate, ordered Hz pass edges
strictly inside Nyquist and finite0<Rp<As<=300dB/Rp<=120dB. CONTROL allocation-free
bounded design and AUDIO sample/block/reset no heap/lock/I/O. Strict numerical
representability and Jury stability reject without clamps. Independent channel
owner, replacement resets all history, existing API/defaults unchanged, no deps.

Use explicit modulus pairs(k,kprime) and completeK via at most32 AGM steps.
Real Jacobi functions use descending amplitude from AGM histories; dn is computed
withhypot(cn,kprime*sn), with exact endpoints, avoiding division instability nearK.
Primary equations are [NIST22.20](https://dlmf.nist.gov/22.20),
[NIST22.2](https://dlmf.nist.gov/22.2), [NIST22.6](https://dlmf.nist.gov/22.6)
and [addition formulas](https://dlmf.nist.gov/22.8). No framework implementation
or inaccessible source is an implementation input.

Ep²=expm1(Rp*ln10/10),Es²=expm1(As*ln10/10),k1=Ep/Es.
The degree solve enforces Kprime(k)/K(k)=Kprime(k1)/(N*K(k1)) through128
log(k/kprime) bisections, retaining the nearest actual candidate including both
endpoints. Bracket: normalMIN_POSITIVE to next-down(1), complements from the same
stable product. Small complements rounding to1 retain the separately meaningful
small modulus for the complementary integral. Positive representable Ep² and
As<=300 imply k1>=approximately2e-177; N>=1 gives k>=k1, so the normal lower bound
is conservative. Relative residual must be<=1e-12; unrepresentable near-one
spacing rejects. This is not a continuous fractional modulus concealed behind a
rounded public transition ratio.

Find r withsc(r,k1prime)=1/Ep in0..Kprime(k1) through128 bounded bisections, with
relative residual<=1e-12. v0=K(k)*r/(N*K(k1)); evaluate sv/cv/dv at(v0,kprime).
For j odd1..N-1 if evenN or even2..N-1 if oddN, evaluate s/c/d atj*K/N.
Pole re=-c*d*sv*cv/(1-d²sv²), im=s*dv/(1-d²sv²); zero pair+-i/(k*s).
Odd degree adds exactrealpole-sv/cv and an infinite zero. N1 directly uses the
closed pole-1/Ep and k=k1, preserving valid extremely small ripple/nome profiles
without an unresolved inverse or an alternate-family fallback.

Frequency maps/prewarp and band quadratics preserve real sections and actual
finite zero transforms. Normalize each section at the mapped pass reference;
apply even prototype10^(-Rp/20) once, odd1. Far-stop parity even10^(-As/20),odd0.
Stable larger band root/product-derived partner avoid cancellation; unit-circle
zero numerators do not undergo stable-pole validation.

Spec starting order isceil(K(kedge)*Kprime(k1)/(Kprime(kedge)*K(k1))) with
kedge=Wp/Ws LP orWs/Wp HP. AtcandidateN actualmodulus gives warped passedge
intervalLP[Wp,Ws*k],HP[Ws/k,Wp]. Select geometricmidpoint, round toHz, design
actual sections and strictly verify both supplied edge amplitudes. Any failed or
unrepresentable candidate retries through32; no parameter relaxation. Actual
order/edge/stopratio expose selection. No global-f64-minimum or continuous
postround ripple certification is inferred from nominal mathematics.

## Acceptance and limits

Independent convergent complete-integral power series ([NIST19.12](https://dlmf.nist.gov/19.12))
and log-nome theta Fourier series ([NIST20.2](https://dlmf.nist.gov/20.2)) supply
modulus and rational amplitude, independent of production degree/AGM/poles.
Explicit series tail bounds qualify the oracle; log form preserves q1 underflow.
Every order/four response families, pass critical points, stop edges/finite zeros
and parity must be checked alongside independent N1 closed time/N2 spectral
polynomial factors, exact real-band transforms, steady tones/finite impulse,
strict spec/minimum, partition/reset/preflight/heap and actual prepared stereo
Foundation/Graph composition. All old Signal regressions/lint/Rustdoc required.

Not all extreme input combinations are representable; failures are explicit.
Finite impulse windows do not prove an infinite IIR tail is absent. Signal
headroom/nonfinite/denormal/transition policy remains the consumer's. No physical
host/device/platform or deadline support follows from offline fixtures. FIR
minimax/equiripple and configured halfband/allpass designers remain independent.
