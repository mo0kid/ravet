# 0258: Native nonlinear multimode ladder

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

Use an explicit continuous four-RC-stage model with one feedback-summing tanh
saturator, discretized by prewarped trapezoidal integration. This is not a claim
of distributed transistor, OTA or diode circuit exactness. Six modal taps provide
LP12/LP24/HP12/HP24/BP12/BP24 without normalization or hidden oversampling; lower
modal orders retain all four resonant feedback poles. Both original physical
cutoff and divided normalized endpoint qualify before native representability.
Unavailable rounding rejects without clamp/nudge. Public defaults remain absent.

Reduce instantaneous feedback to a strictly monotone scalar equation with native
lambda=k*G^4. Amplitude-scaled bisection is bounded to 64/32 iterations and
67/35 solver tanh calls plus one staged diagnostic. Select adjacent endpoints by
rounded native residual; do not claim high-precision neighbouring-float residual
optimality. Scalar affine versus staged actual recurrence diagnostics are distinct.
Native platform math and native rounding/state policy may differ across targets.

State is the trapezoidal integrator memory s=y+v, not voltage y. set_design retains
it; replace_design/reset explicitly clear it. Bypass freezes state/observations and
returns dry for every mode. Nonfinite source/history behavior is visible; zero
feedback and positive-feedback/rounded-zero-lambda branches are separate. NaN
poisons until reset, signed infinity/drive overflow saturates with finite history.
Denormal snapping is caller explicit. Low-cutoff native32 DC rounding floors and
persistent native32 subnormal tails are measured/documented, not silently repaired.
The ideal k=4 endpoint remains marginal; finite selected tails do not establish
arbitrary-modulation stability or universal headroom/accuracy.

Fixed 1..64-channel banks and whole-call preflight retain prefix/suffix and
observation invariants. Independent 450-digit zero-state/history/cancellation/knee
root equations retain strict native-relative budgets when c/lambda match. All 2016
recorded history equations match this backend exactly. A separately qualified
monotone-equation bound abs(delta_c)+abs(delta_lambda)*abs(reference_root), with
upward rounded nonzero operations and native-relative budget scaled to the bounded
actual-root magnitude, accommodates potential native-math equation drift. Sixty
synthetic alternate equations admit independently computed displaced roots and
reject deliberate extra solver error, without claiming observed platform drift.
Four-dimensional nodal Newton, polynomial time and bilinear/analog
response oracles qualify both precisions/all modes. Every bank refusal preserves
all histories/observations, and 64 channels compare with independent filters.
Actual frame/planar/in-place Graph→FIR callbacks use separate mode/design/bypass/
replace schedules, repeated lifecycle and NaN/reset, with measured heap zero.
Native32 explicitly bridges to existing f64 FIR. Numerical evidence is captured
with nocapture, without treating green logs as universal DSP/platform support.

Continuous RK4 and coherent zero-resonance aliasing fixtures qualify the selected
model. The ideal bandlimited Fourier reference intentionally removes folded
harmonics; it is neither a framework oversampler test nor an implicit-feedback
alias claim. Original frozen W245 evidence remains historical; revision extends
qualification without changing coefficient, solve or stage recurrence expressions.
