# ADR 0255: Prepared panning curves and pair ramps

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

## Context

Graph gain routing and a scalar smoother do not provide a prepared panning
law family or guarantee that two gain transitions advance once per frame.
Panning a mono source into two outputs and balancing two existing channels
are distinct source policies that should remain explicit.

## Decision

Prepare seven mathematical laws with caller-selected unity endpoints or
unity center. Keep the actual exponents in names and document center loss
and endpoint boost. Evaluate a mirrored near/far pair, including explicit
endpoint/center values, so opposite pans swap actual coefficients exactly.
Store private immutable metadata alongside actual native f64 coefficients;
explicit conversion creates native f32 coefficients, retaining requested
metadata without promising ideal rounded power.

Each reusable stereo panner owns one pair transition. A changed target
starts from current rounded gains, first frame advances 1/N and frame N
assigns the target; identical actual pairs update metadata without restarting.
Use explicit f64 endpoint interpolation for both variants, native coefficients
and sample arithmetic. No per-frame transcendental functions, allocation,
locks or I/O. Every complete prefix is preflighted before gain or output
mutation; empty calls retain progress and inactive suffixes are untouched.
Define mono duplication and stereo balance separately, without crossfeeding.

## Evidence and limits

Independent signed-coordinate elementary/trigonometric/power expressions
cover 10,001 positions for all seven laws, two normalizations and both
precisions, including center/endpoint metadata, exact mirroring and observed
power. Orthogonal/coherent fixtures distinguish level policies. Frame-index
transcriptions of the declared interpolation policy cover exact short durations, retargeting,
identical pairs and bounded u32::MAX prefixes with exact remaining counts;
no skip API or full u32::MAX-duration execution claim is added. Partition,
late right-channel/max/count failures, empty calls, native rounding,
nonfinite multiplication and boosted overflow preserve the declared policy.
Actual native mono/stereo Foundation→Graph→FIR consumers qualify prepare,
reset, deactivate/reprepare, variable blocks and measured callback heap
freedom. F32-to-f64 FIR bridging is caller explicit.

Ramps need not follow a static pan curve or preserve power. Arbitrary
correlation/loudness, limiter/spatialization, SIMD/native deadlines and paired
wet-latency-compensated dry/wet ownership remain outside this decision.
