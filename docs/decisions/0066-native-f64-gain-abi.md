# 0066: Separate native f64 reference handle

## Context

Native gain format adapters need a reusable f64 DSP boundary while retaining
existing f32 handles, state bytes and serialized ownership guarantees.

## Decision

Add separate opaque RavetGain64 lifecycle/process symbols with double sample
pointers and existing float/u32 control values. Both precisions expand one Rust
ABI implementation for bounds, copied-input aliasing, validation/fault handling
and pointer obligations. Separate native processors preserve audio precision;
state encoding remains one shared implementation. No precision casts between
opaque handles are valid.

## Consequences and validation

Preparation allocates bounded scratch; process/reset/restore do not allocate.
Caller-owned ranges remain live/aligned, outputs disjoint and result storage
separate. Foreign pointer validity is an unsafe obligation, not inferred from
null checks. Panic containment does not establish exceptional deadlines.

Existing f32 ABI fixtures still pass. f64 fixtures cover exact samples, aliasing,
backing tails, bypass/state, rejection and prepared heap operations. Actual linked
C++ acceptance runs six mono/stereo/rate cycles through the public header/static
library. This adds ABI capability only; each native format needs separate mapping.
