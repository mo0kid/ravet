# 0238: Bounded selected-order Butterworth sections and verified specifications

Status: Accepted implementation contract. Reviewed source is integrated;
public qualification is recorded separately in work status.

## Context

The existing const-generic Butterworth cascade has even low/high orders. Reusable
consumers also need odd orders, higher band transformations and bounded selection
from pass/stop constraints. These require explicit order meaning, coefficient
representability and state-replacement policies.

## Decision

Add independent f64 ButterworthDesign/ButterworthFilter owners with fixed16-slot
arrays. Low/high order1..32; band total digital order even2..32, prototype degree
half that total. Keep all existing cascade/filter behavior unchanged. Design is
CONTROL work without heap/dependencies; processing has no heap/lock/I/O. Strict
finite coefficients/Jury stability and finite positive normalization reject
unrepresentable designs; no clipping or stability margin repair.

Warp Hz frequencies w=tan(pi*f/rate). Prototype degreeM has left-half roots
exp(i*pi*(2j+1+M)/(2M)); exact odd real root-1 avoids sine-rounding ambiguity.
LP analog roots w*p, HP w/p. BP roots solve s²-B*p*s+W0²=0; BS uses B/p instead
of B*p, where B=H-L and W0=sqrt(LH) from separately warped band limits. Select
the larger-norm quadratic root and derive its partner from their product to
reduce cancellation. The odd prototype can transform to two real roots or a
conjugate pair; both form one real section. Bilinear z=(1+s)/(1-s) supplies stable
digital poles. LP/HP endpoint zeros and BP ±1/BS warped-center unit zeros provide
numerators. Normalize each section at LP/BS DC, HP Nyquist or BP warped center.
Odd LP/HP first-order sections use zero second-order terms.

Specification selection requires0<Rp<As<=300dB and explicit ordered pass/stop
edges. Ep²=expm1(Rp*ln10/10), Es² likewise. Minimum starting order is
ceil(ln(Es²/Ep²)/(2*ln(q))), q=Ws/Wp LP orWp/Ws HP. The admissible warped cutoff
interval is[Wp/Ep^(1/N),Ws/Es^(1/N)] LP or[Ws*Es^(1/N),Wp*Ep^(1/N)] HP.
Choose the geometric midpoint, round back to Hz, design actual sections and verify
pass amplitude>=10^(-Rp/20) and stop amplitude<=10^(-As/20) without tolerance
relaxation. If an unrepresentable midpoint or its rounded sections fail, retry
higher orders through32, otherwise fail. Return actual selected order/cutoff;
no smallest-f64-cutoff or exhaustive representable-order claim.

Replacing a selected-order filter design clears every history. Mapping old states
onto unrelated poles would be ambiguous; existing history-retaining cascade APIs
remain available with their existing contracts. No smoothing/crossfade, finite
IIR drain length, signal sanitation, denormal/headroom or deadline claim.

## Acceptance and consequences

Independent warped scalar prototype responses cover all orders/three rates/band
widths, endpoints/cutoffs/center. Independent positive-polynomial denominator
recurrence plus bilinear polynomial substitution checks odd/even rendered LP/HP
without production pole/SOS reuse. Wide real-root and narrow conjugate band cases
also have independent second-order impulse recurrence; rendered steady-tone gain
checks all response types. Strict specification/minimum/rounding/invalid tests,
all-order replacement/partition/reset/heap and actual stereo Foundation/Graph
composition are required alongside existing Signal tests/lint/strict Rustdoc.
No external/runtime dependency. Offline evidence does not establish acoustic,
device/DAW/platform support or native real-time deadlines. Other filter families
and DSP profiles remain independent capabilities.
