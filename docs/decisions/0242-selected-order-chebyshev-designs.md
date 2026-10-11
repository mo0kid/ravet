# 0242: Bounded Chebyshev I/II poles, zeros and edge selection

Status: Accepted implementation contract. Reviewed source is integrated;
public qualification is recorded separately in work status.

## Context

Higher-order Chebyshev I and II have different ripple edges, finite zeros and
parity gains. A reusable family cannot reuse Butterworth labels/endpoint zeros or
normalize even Type I pass gain to1 without changing its ripple convention.

## Decision

Add independent fixed16-section f64 ChebyshevDesign/ChebyshevFilter. LP/HP order
1..32; total band order even2..32/prototype half that total. Type I positive finite
ripple<=120dB, Type II attenuation<=300dB; rate positive finite/ordered Hz edges
strictly inside Nyquist. CONTROL allocation-free design; sample/block/reset no
heap/lock/I/O. Strict finite/intermediate/gain/pole/Jury representability rejects,
without clamps. Selected-design replacement clears every history. Existing filter
APIs/defaults and frozen Butterworth implementation stay unchanged; no dependency.

T_N Chebyshev polynomial gives Type I squared magnitude
1/(1+ep²*T_N(Omega)²), ep²=expm1(Rp*ln10/10). Spectral poles use
mu=asinh(1/ep)/N, theta=(2j+1)pi/(2N),
p=-sinh(mu)sin(theta)+i*cosh(mu)cos(theta), plus exactreal-sinh(mu) for oddN.
No finite Type I prototype zeros. Preserve prototype DC1 odd or10^(-Rp/20) even.

Type II squared magnitude T_N(1/Omega)²/(T_N(1/Omega)²+es²),
es²=expm1(As*ln10/10). Use reciprocal roots of the corresponding spectral factor
with mu=asinh(es)/N. Finite prototype zero pairs +-i*sec(theta), odd remaining
zero at infinity. Prototype DC1; far-stop limit10^(-As/20) even,0 odd.

Prewarp separately withtan(pi*f/rate); LPw*p/HPw/p and BP/BS quadratic transforms
as mathematically specified. Transform finite zeros by the same frequency map,
without stable-pole validation on imaginary-axis zeros. Purely imaginary band
zero quadratics use a robust larger root and product-derived partner; real
unit-circle numerators preserve actual stop notches. Odd infinite zero maps to
LP-1/HP+1/BP+-1/BSwarped-centerpair. Pair sections deterministically, normalize
at prototype pass-reference mapping and apply nonunity Type I parity gain once.

Spec methods derive Type I rippleRp or Type II attenuationAs with0<Rp<As<=300,
Rp<=120. Starting orderceil(acosh(sqrt(Es²/Ep²))/acosh(q)), q=Ws/WpLP orWp/WsHP.
AtcandidateN qN=cosh(acosh(sqrt(Es²/Ep²))/N). Type I warpededge interval
LP[Wp,Ws/qN],HP[Ws*qN,Wp]; Type II LP[Wp*qN,Ws],HP[Ws,Wp/qN]. Choose geometric
midpoint, round toHz/design actual sections, verify supplied pass>=10^(-Rp/20)
and stop<=10^(-As/20) strictly. Failed/unrepresentable midpoint retries through32
only. Returnactualkind/edge/order; no parameter/order repairs or global-f64-edge
minimum/continuous-certified postround-extrema claim.

## Acceptance and consequences

Allorder/types/responses versus independent Chebyshev recurrence; ripple critical
points, transformed finite zeros and endpoint parity distinguish genuine family
behavior. Independent first-order closed-form and T2 spectral polynomial/bilinear
recurrence check time without production poles/SOS; exact real prototype two-real/
conjugate band transforms have an independent time recurrence. Rendered tone and
finite impulse DTFT/final-window observations, strict spec/minimum/boundary,
replacement/reset/heap/preflight and actual stereo Foundation/Graph consumers
are required with full Signal regressions/lint/strict docs.

A finite tiny observed tail window qualifies those fixed test profiles, not a
proof of general finite IIR flush completion. Nonfinite audio, denormals/headroom,
transition smoothing and native deadlines remain consumer policy. Offline evidence
does not establish physical/host/platform support. Elliptic, halfband/equiripple
and other DSP profiles remain separate capabilities.
