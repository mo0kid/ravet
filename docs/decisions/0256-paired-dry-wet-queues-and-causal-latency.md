# ADR 0256: Paired dry/wet queues and causal latency

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

## Context

Prepared panning weights do not establish matching dry/wet ownership or
compensate a caller's additional wet-path latency. Independent dry push and
wet consume must preserve channel/source coordinates across unequal batches,
with no gain advancement on pushes or silently repaired queue errors.

## Decision

Expose immutable direct wet-coordinate gains using the full seven mathematical
curves/two normalization policies. Share private curve preparation with panning
in the new composed source only, retaining exact panning expression/branch
behavior. Do not route tiny wet fractions through the rounded affine pan
coordinate. Native f32 conversion retains requested metadata and actual gains.

Prepare a single-owned bounded native queue plus causal channel histories,
preflight aggregate32MiB arithmetic before fallible reservations and recheck
actual capacity. Push advances each channel delay once per accepted source
frame and stores already-delayed dry. Consume advances the shared gain pair
once per frame and mixes every corresponding channel, with complete prefix,
capacity and counter admission before any state/output mutation. Empty calls
retain state and backing suffixes remain untouched. Queued errors preserve
exact pending suffix and owner continuation rather than dropping/filling.

Use explicit immutable Integer/Linear/cubic Lagrange/first-order Thiran
reconstruction, zero initial history and no hidden minimum delay. Native32
rejects nonzero fractional values rounded to0/1, and Thiran rejects rounded
unstable endpoint a=1. Integer deliberately truncates. Changed latency requires
an empty queue, identical requested latency is idempotent. Immediate retapping
preserves histories, with caller-owned discontinuity/crossfade policy. Reset
clears queue/history/counter epoch and snaps gains without reallocating.

## Evidence and limits

Independent full direct curve and frozen panning-expression comparisons, all
native channel1..64/policy profiles, product-form Lagrange tap oracle, Thiran
closed impulse/DFT/DC moment with explicit geometric tails, actual native
parameter observations, history-preserving retaps, unequal batch partition and
physical wrap tests qualify the accepted profile. Whole-call late input/output,
count/max/full/empty/resource/native coefficient/queued latency failures preserve
owners and valid continuation. Near-u64 overflow is tested on both native owners
privately without adding a public skip. Consumption-only ramp tests include
bounded u32::MAX prefixes without claiming full execution. Actual native
three-channel Foundation→Graph consumers pair differently batched dry with a
real explicitly delayed wet FIR, changing full-family weights, lifecycle and
measured callback heap freedom. The f32 FIR bridge is caller explicit.

Callers own source pairing, actual wet latency, timestamps, tails and headroom.
Fractional FIR is not ideal delay; fixed Thiran's DC-delay/allpass facts do not
certify broadband delay or retuning transients. There is no implicit EOF/drain,
normalization repair, silence/drop recovery, limiter, physical/device/DAW,
SIMD or deadline claim. Nonfinite/overflow audio naturally propagates and can
poison delay history until reset. Construction/disposal are CONTROL; reset work
is prepared-capacity dependent.

Additional source review clarifies both native precisions reject fractional
Thiran when actual a rounds to1. Integer-mode fraction/coefficient getters are
observations of unused parameters, not executed delay. Delay reconstruction
uses native precision; pair interpolation uses f64; denormal handling is caller
policy. Independent maximum-fraction wrap and fractional2.5-group-delay actual
Graph/FIR cancellation fixtures complement the integer composed fixture.
