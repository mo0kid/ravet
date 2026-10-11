# 0239: FIR coefficient generation with explicit empirical and numerical policies

Status: Accepted implementation contract. Reviewed source is integrated;
public qualification is recorded separately in work status.

## Context

Existing direct and partitioned FIR runtimes consume supplied coefficients.
Consumers need reusable frequency/window/transition and weighted-band coefficient
preparation, without duplicating convolution or pretending empirical estimates
are verified continuous-band specifications.

## Decision

Add immutable owned symmetric f64 FirDesign, finite maximum65,536 taps/512KiB,
fallible CONTROL reservation/design/Drop. Reuse existing convolution owners.
Order/tail N-1 and linear-phase delay(N-1)/2 are explicit, including even Type II
half-sample delay. Actual response inspection belongs on CONTROL. Existing window,
convolver and fixed IIR oversampling behavior remains unchanged; no dependencies.

Windowed ideal band integrals support LP/BP Type I/II, HP/BS odd Type I only
because Nyquist-pass response is impossible for even symmetric Type II. Separately
validated Hz edges/rate, cosine windows or Kaiser beta0..32. Compute one side and
mirror exactly; normalize actual LP/BS DC, HP Nyquist or BP arithmetic center,
reject unrepresentable reference. Positive-series I0 is bounded128iterations over
beta0..32. Cutoffs describe ideal transition centers, not guaranteed finite -6dB
or IIR -3dB points. No coefficient/stability/gain clamps.

Empirical Kaiser order/beta uses requested attenuation8..120dB and pass/stop gap;
promote order to even/length odd and expose privately linked actual response,
original estimated order and selected beta. Do not enforce or claim continuous
specification satisfaction: the known first-lobe shortfall remains a tested
observation. No hidden stronger attenuation/order search or exhaustive minimum.

Shaped transition uses K=1..4 unit-area frequency rectangle convolutions, width
Δ/K each. K-fold density has degree K-1 and integrated edge degree K, full support
widthΔ. Infinite taps2fc*sinc(2fc*t)*sinc((Δ/K)*t)^K; finite truncation and actual DC
normalization are explicit. Transition remains inside DC/Nyquist. Finite response
ripple/leakage is not silently replaced by the analytic infinite target.

Weighted integrated least squares uses Type I odd3..511 taps/V<=256 and at most
32 sorted nonoverlapping linear desired bands, endpoints[-16,16], weight(0,1e6].
A(f)=sum ak*cos(2pi*k*f); central tap a0, symmetric paired taps ak/2. Analytic
cos-product and linear-desired*cos integrals form Gram/RHS; unregularized Cholesky
solves on CONTROL. Normalize weights by max; positive width/weight underflow
rejects. Pivot<=64eps*V*largest Gram diagonal rejects and normalized infinity
residual must be<=1024eps*V. This numerical policy is observable, not formal exact
arithmetic or hidden ridge/pseudoinverse. Preserve solved gain to retain objective;
unconstrained transitions may overshoot. Matrix<=512KiB plus vectors/output.

Windowed halfband fixes central0.5/noncenter even-offset0, length4m+3<=65535.
Scale only symmetric odd offsets for DC1/Nyquist0 and complementary centered
response within rounding. Keep equiripple/minimax halfband and certified continuous
spec designs explicitly separate; do not close them from a windowed variant.

## Acceptance and limits

Independent Fourier band integrals, integral I0/known constants, degree1..4
frequency quadrature and integrated LS quadrature stationarity/objective/hand
systems qualify math without production-coefficient reuse. Invalid/capacity/
ill-conditioned input and private allocation reclamation, maximum tap/matrix,
exact symmetry/halfband, actual direct/partitioned convolution and stereo
Foundation/Graph delay/tail/partition/reset/heap compose meaningful acceptance.
Full existing Signal tests/lint/strict docs remain required.

These are reusable coefficient preparations, not extra reference-only DSP owners.
No automatic host latency/tail mapping, transition crossfade, minimum-phase,
minimax/continuous bound certification, multichannel runtime or native deadline.
Physical/device/DAW/platform support cannot follow from offline evidence.
