# 0235: Explicit low-cost streaming interpolation profiles

Status: Accepted implementation contract. Reviewed source is integrated;
public qualification is recorded separately in work status.

## Context

Existing windowed-sinc conversion filters and owns a prepared table/history.
Linear delay/table reads do not expose stream consumed/produced progress.
Consumers also need small stack-backed reconstruction profiles with explicit
quality, phase, source-history and backpressure behavior.

## Decision

Provide one independently owned f64 `StreamInterpolator` with immutable hold,
linear, Catmull–Rom, cubic Lagrange and quartic Lagrange variants. Keep exactly
five history samples and scalar phase, without heap storage or hidden I/O.
Prepare positive finite floating rates or exact positive-u32 integer rates with
input/output ratio 1/16..=16. Integer relative phase stays in (-O,I], using i64
for positive u32 rates; no accumulated duration counter can overflow or round.
Floating convenience retains its explicit long-duration count limitation.

Common source bracket at output J is k=ceil(JI/O), with backward fraction
f=k-JI/O and required consumed input count k+1. Hold selects x[floor(JI/O)] and
intentionally still requires that common bracket. Linear interpolates x[k] and
x[k-1], nominal delay zero but fractional bracket lookahead up to one input
sample. Catmull–Rom evaluates the Hermite cardinal cubic with half-difference
slopes through x[k-3..k] at coordinate 2-f on the four chronological nodes.
Cubic Lagrange evaluates that four-node interpolating polynomial at 2-f;
quartic Lagrange evaluates five chronological nodes at 2-f. Their nominal delays
are respectively one, one and two input samples. Initial history is zero.
Exact phase nodes use the direct supported sample to avoid introducing 0*NaN
from irrelevant history. No third-party coefficient or PCM identity is assumed.

Return exact progress and touch only produced output prefix. Capacity zero
consumes nothing; empty input drains only pending output. Additive finite-gain
preflight rejects before any state/output edit. Gain zero advances stream but
leaves output bitwise unchanged, avoiding artificial NaN from 0*nonfinite source.
Input/nonzero-gain arithmetic otherwise propagates nonfinite values; headroom,
denormals and resets are explicit consumer policy. No kind/rate live mutation.

For N>0 supplied input samples, integer available output count after pending drain
is 1+floor((N-1)O/I); N=0 yields no output. Caller-provided zeros count as input,
with no invented EOS/padding/trim/copy policy. Independent channels coordinate
progress externally; no same-frame processor adapter silently invents rate or
transport semantics.

## Consequences and acceptance

All five kernels are usable standalone reusable APIs. They provide no antialias
filtering: downsampling alias and cubic/quartic overshoot are intentional and
measured. Existing sinc, delay, wavetable and graph behavior stays intact. Graph
already supplies heterogeneous prepared processor composition; the new consumer
fixture composes source interpolators and FIR processing through actual Graph
without adding a redundant reference chain.

Acceptance requires independent direct-coordinate Lagrange product/Hermite
oracles, exact count/phase and long bounded recurrence, polynomial reproduction,
all-family partition/backpressure/reset/tail/add-gain/heap cases, measured low-tone
error and deliberate out-of-band alias, actual stereo prepared Foundation/Graph
composition, existing signal regressions, lint and warning-denied Rustdoc.
Offline evidence does not establish device/DAW/platform/SIMD/deadline support.
