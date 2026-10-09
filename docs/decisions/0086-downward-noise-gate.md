# 0086: Prepared downward-expansion noise gate

Status: implemented portable profile; offline numerical/heap evidence.

## Decision and scope

Reuse EnvelopeFollower's rectified attack/release dynamics with an explicit f64
sidechain. Prepare threshold/ratio/knee/floor and validated times before samples.
Below threshold, expansion uses slope ratio minus one for gain dB; a quadratic
soft knee joins the unity region with continuous slope. Clamp reduction to a
finite configurable floor rather than hard mute. Ratio one stays unity even at
zero detector level. Reset clears only detector history.

This complements compression without adding implicit lookahead, hold/hysteresis,
linked channels or separate gain smoothing. Per-sample log/pow remains explicit.
Nonfinite samples are not sanitized and can poison detector history until reset.
No host/device/real-time deadline claim follows.

## Evidence

Three groups compare independent static dB and knee landmarks, knee boundaries,
floor/ratio-one/sidechain behavior, exponential attack/release steps, reset,
invalid configuration and 10000 prepared sample/reset iterations without counted
Rust allocations. Initial formatting caught C++-style fractional literals; Rust
leading-zero literals correct that syntax before testing.
