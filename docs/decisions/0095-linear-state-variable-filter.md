# 0095: Prepared linear trapezoidal state-variable filter

Status: implemented scoped one-channel f64 profile.

## Decision

Solve the two linear trapezoidal integrator equations, preparing grouped scalar
coefficients and retaining two histories. Return low/band/high outputs together
so consumers need not maintain three separate filters. Raw band output has gain
Q at cutoff. Use cutoff/rate 1e-6..=0.499999 and Q 0.1..=100 as explicit numerical
limits; no implicit clamp or frequency/Q smoothing. Replacement retains histories;
reset clears histories and retains coefficients. Processing is fixed storage.

Mathematical reference: [Andrew Simper, linear trapezoidal SVF derivation](https://cytomic.com/files/dsp/SvfLinearTrapOptimised2.pdf).
The implementation expresses the solved equations; no reference source file,
notebook, asset or dependency is incorporated. Nonlinear/saturated profiles,
denormals, arbitrary time-varying stability and native deadlines are separate.
Nonfinite audio propagates and may poison state; extreme finite audio may overflow.

## Evidence

Three groups compare all three impulse outputs against independent existing
biquads across cutoffs/Q, check the weighted reconstruction identity, exercise
live coefficient changes against an ungrouped simultaneous nodal solve, verify
reset/no heap work and cover constructor/end-point/nonfinite recovery policy.
These finite fixtures do not prove all floating-point corners or modulation.
