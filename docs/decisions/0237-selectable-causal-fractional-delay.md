# 0237: One temporal output per input for selectable fractional delay

Status: Accepted implementation contract. Reviewed source is integrated;
public qualification is recorded separately in work status.

## Context

Existing DelayLine supplies independent stateless linear taps. Recursive allpass
interpolation would make repeated reads advance ambiguously. Consumers need an
explicit bounded owner for causal integer, linear, cubic and first-order allpass
profiles, with truthful boundary, edit and tail behavior.

## Decision

Add independently owned one-channel f64 FractionalDelay. Immutable policy and
maximum; initial requested delay zero. Fallible CONTROL reservation owns maximum+4
history values, at most32MiB, with checked capacity arithmetic before allocation.
Extra slots supply cubic support and add no output delay. Maximum0 is legal.
Processing/control edits/reset have no heap/locks/I/O; Drop is CONTROL. Reset is
O(prepared capacity), without a constant-time/deadline assertion.

Write current input, reconstruct exactly once and advance exactly once. Delay
finite in0..maximum, d=n+f. Integer truncates d; linear uses taps n,n+1. Cubic
Lagrange uses four causal nodes n..n+3 evaluated at f, explicitly one-sided rather
than centered/minimum-one-delay. At exact integers directly read the supported
sample without irrelevant0*NaN terms. Initial earlier history is zero.

Thiran first-order uses a=(1-f)/(1+f), v=x[t-n],
y=a*v+prior_v-a*prior_y. Fixed noninteger delay transfer is
z^-n(a+z^-1)/(1+a*z^-1); magnitude unity, group delay at DC n+f and at Nyquist
n+1/f. Pole -a yields a long alternating tail near f0. Reject positive f whose
computed a rounds to1 before any control/history edit. Integer f0 bypasses the
degenerate recursion and updates both histories to v. No hidden coefficient clamp.

Retuning is immediate/history preserving, without smoothing/crossfade; branch
changes retap the sequence feeding retained recursive history. Static LTI allpass
facts are not time-varying energy/stability or feedback promises. Nonfinite input
may poison history; consumers own headroom, denormals and reset.

Separate blocks require equal lengths and preflight before all edits. Empty
blocks are no-op. No arbitrary read/pop tap API, invented input padding/drain,
multichannel/rate conversion, host latency/tail mapping or callback deadline.
Existing linear delay, chorus/comb/reverb and all other signal behavior remains.

## Consequences and acceptance

This is a reusable selectable effect profile, not a reference-only variant or a
change to existing defaults. All reconstruction support is causal. Cubic may
overshoot and fractional FIR magnitude is not flat. Thiran fixed allpass tests
must account for finite impulse remainder: h0=a, hm=(1-a^2)(-a)^(m-1), m>=1;
after m0..L omitted energy is(1-a^2)a^(2L), not zero by arbitrary finite timeout.

Acceptance requires independent absolute-time tap/polynomial/recurrence/transfer
oracles, wrap/partition/reset/modulation/boundary/preflight/heap and actual stereo
Foundation/Graph consumer checks, plus existing Signal regressions, Clippy and
strict Rustdoc. No new external/runtime dependency. Offline qualification does
not imply acoustic/device/DAW/platform or native deadline support.
