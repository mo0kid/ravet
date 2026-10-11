# 0251: Native peak/RMS ballistics with scaled energy and explicit precision

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

## Context

The existing f64 peak detector and finite-window meter statistics have different
history/timing contracts. A reusable attack/release RMS family needs explicit
energy versus amplitude semantics, native precisions, complete channel preparation
and callback-prefix preflight without changing existing dynamics defaults.

## Decision

Add validated immutable `BallisticsConfig` / explicit `BallisticsConfig32`, Peak
and Rms modes, fixed sample owners and independent 1..64-channel planar banks.
Rate 1..768000 Hz, times 0..60 seconds and block bounds 1..65536. Zero time is
explicit instantaneous response. Positive time coefficients use stable expm1;
reject alpha or actual native complement rounded to endpoints, and unresolved
RMS pure-release factor. Actual coefficients and requested metadata are separate
from exact ideal timing promises. No dependencies, allocation or legacy changes.

Store amplitude in both modes. Target = abs(input); attack when target > state,
otherwise release. Peak blends amplitude. RMS scales by the higher of state/target,
blends their normalized squared ratio, then rescales the square root. This is
algebraically a one-pole energy recurrence; full magnitudes are never squared.
Equal state/target and zero have exact branches. Avoid precomputed square-root
weights whose separate rounding can bias total mass at slow f32 timing. No hidden
clamp, normalization or coefficient repair. Independent high-precision Decimal
wide-exponent audit and native tests support the bounded equation.

Ideal peak constants apply to amplitude and RMS constants to energy. Actual native
sqrt/complement/history rounding can differ appreciably at admitted slow f32
extremes; retain measured observations and explicit endpoint refusals. Tests compare
tight independent driven-energy oracles, exact native repeated-zero factor and
separate derived initial/coefficient/gamma_n rounding bounds. Do not merely enlarge
an unexplained ideal-tail tolerance.

NaN is sticky NaN. Otherwise input/history infinity is sticky positive infinity.
Reset and validated finite nonnegative initial amplitude clear poison explicitly;
configuration replacement resets unsmoothed. Caller-only `snap_to_zero` clears
strictly subnormal amplitude states, never implicit block-boundary snapping.
These policies preserve scalar/partition identity. Normal finite ratio underflow
and floating recurrence limits remain explicit.

Bank preflights all counts/prefixes and prepared frame bounds before any output or
channel-history mutation. Independent histories share timing only. Unavailable
channel access and invalid initial level preserve states; empty prefixes consume
nothing and backing suffixes remain untouched. Fixed arrays store at most 64 native
amplitude states; sample/block/reset/config construction do not allocate, lock or
perform I/O. Native square-root cost and caller workload remain separate.

## Acceptance and limits

Both modes/precisions, meaningful rates/times and explicit endpoint/quantization
rejection; independent amplitude/energy steps, finite weighted-square convolution,
switched recurrence and sustained sinusoid energy; normalized large/small magnitudes
and exact equal near-maximum/minimum levels; actual very slow native release versus
ideal timing; all channel counts, separate/in-place prefixes and suffixes, late
invalid channels, mismatch/empty/partition/history/reset/replacement/poison/snap and
zero heap. Actual canonical native f32/f64 stereo Foundation→Graph gain consumers
verify committed source, callback preflight and lifecycle against independent
expected detectors/gain.

No finite-window RMS, finite recursive EOF tail, true-peak/loudness, linking,
lookahead/hysteresis, crossfade, finite-output sanitization, headroom repair,
processor-wide denormal mode, SIMD or native deadline. Offline consumers do not
establish physical device/DAW/platform/acoustic support. Existing detector/dynamics
profiles and broader named DSP criteria remain independent.
