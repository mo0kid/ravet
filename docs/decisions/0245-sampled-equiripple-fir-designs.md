# 0245: Bounded sampled minimax FIR exchange and constrained halfband

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

## Context

Windowed, empirical Kaiser and integrated least-squares coefficients have distinct
objectives. A reusable equiripple family needs genuine weighted exchange, explicit
convergence/error observations and actual LP/HP/BP/BS/halfband consumers. Finite
grid exchange cannot certify the entire continuous interval.

## Decision

Add EquirippleFirDesign and separate EquirippleError. Symmetric TypeI odd3..511,
TypeII even2..512; finite positive rate and explicit separated pass/stop bands.
Generic1..32 FirBands permit finite signed linear target[-16,16]/weights(0,1e6].
TypeII nonzero desiredNyquist rejects; forced target0 is observed/excluded from
support. Halfband N4m+3,3..511 uses a reduced basis and equal reflected weights.
CONTROL fallible generation/drop; existing prepared direct/partitioned FIR owners
are AUDIO consumers. No dependencies/default/history or existing error-enum change.
The public FirBand documentation covers coefficient designers; only a private
validated-owned symmetric constructor is added to FirDesign.

[Official Remez semantics](https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.remez.html)
identify weighted maximum-error exchange and explicit dense-grid controls. Our
implementation derives its interpolation/alternation mechanics independently;
no external framework implementation or assumed numerical package is an input.

Grid max32768 observedpoints including forcedNyquist/reflectedhalfband; endpoints
plus uniform interiors with step<=1/[2*(taps+1)*density]. Optionsdensity8..64,
maxiterations1..128,relativegap1e-12..1e-3 (defaults16/64/1e-8). Checked count preflight,
fallible reused scratch and fixed support arrays have aggregate declared numeric
capacity<=4MiB; allocator overhead is not a process-memory promise. Declared numeric capacities are checked before any reserve, then actual capacities
are checked again. Numerically indistinguishable coordinates/transformed targets/
weights reject without nudging. Nyquist must be exactly available: positive
rate/2 divides back to .5 and doubles back to rate. Normalized original endpoints
must be finite0.. .5 and preserve positive widths/gaps/DC/Nyquist identities before
allocation; TypeII forced metadata requires the exact original Nyquist endpoint.

For Vvariables use V+1supportpoints x_i and logscaledalpha_i=1/prod(x_i-x_j).
Weights normalized by originalmax; TypeI Q1/xcosomega, TypeII Qcos(omega/2)/xcosomega.
WorkingtargetD/Q,weightW*Q. Delta=sum(alpha_i*d_i)/sum(alpha_i*sigma_i/w_i);
y_i=d_i-sigma_i*delta/w_i. Weighted alternation and degree consistency give the
sampled interpolation candidate. Barycentric evaluation is scaled and exact at
support; finite/zero/underflow failures explicit. Select sampled largest-error
sign runs then deterministic maximin threshold/alternating support/window ties.
All loops/resources bounded; no hidden adaptivegrid,regularization or tap growth.

Convergence needs full-grid primal<=|delta|*(1+requestedgap)+explicitnumericallowance,
then actual rounded coefficient grid maximum and supported alternating residuals
satisfy the same bounds. Support stall alone is never success. Exhaustion/stall
returns NonConvergence with actualiterations and no partial result. Exact target
within numericallowance may have no alternating support. FinalDCTTypeI/II recovers
symmetric coefficients without normalization. Allowance128eps*(V+1)*max(1,max|D|,
sum|coeff|) in normalized units; exceeding1e-6*max(1,max|D|) rejects as unrepresentable.
Immutable actual error/ripple/extrema/iteration/point/capacity/weightscale receipts
prevent an editable metadata claim. Restored tiny original-unit products can
underflow; normalized values preserve information and original error sign.

Halfband basis A=.5+cosomega*P(cos2omega),V=(N+1)/4. Lowband target.5/cosomega and
weightcosomega solve the same exchange; reflected stop follows complement. DCTIV
odd cosine coefficients populate only symmetric odd offsets, exactcenter.5 and
evenoffset0. Quartercycle.5, A(pi-omega)=1-A(omega). Do not normalize DC or project
an unconstrained generic solution: equal pass/stop ripple and free-basis optimum
are the desired constrained behavior. Rounded lost transition gaps reject.
Despite structural complement, actual rounded coefficients are evaluated at both
halves of every declared grid. Reflected support metadata independently evaluates
the rounded stop frequencies and is not inferred by negating low errors.

## Acceptance and limits

Independent small-variable linear-program vertex enumeration directly solves
cosine-basis constraints on the actual grid, not production barycentric exchange.
Medium andnonconstant511/512 cases use independent85-digit Decimal Gaussian dual
annihilation/nonnegative weights/full-grid actualcoefficient evaluation, with
qualified Taylor cosine/residuals. Meaningful long ripple and primal/dual gap must
be observed, rather than length-only constant/zero targets. Denser halfband
observations, exact3tap closed optimum, exactstructure/complement andallnamedresponse
profiles complement generic signed/linear/weightscale/forcedNyquist/preflight/
capacity/unrepresentability/nonconvergence tests. ActualstereoFoundation/Graph
convolver andpartitionedlatency/fulltail/reset/heap fixtures required.

No postround continuous certification, automatic spec/tap sizing, odd-symmetric
Hilbert/differentiator, minimum phase or globalfloatingminimum is promised.
Headroom/nonfinite/denormals/smoothing/deadlines remain consumer policy. Offline
numerical fixtures do not establish DAW/device/platform/acoustic support.
