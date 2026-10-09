# 0087: Prepared low/high shelving designs

Status: implemented; portable numerical/heap evidence.

## Decision and scope

Add a separate ShelfKind and design_shelf constructor, preserving FilterKind's
existing Q semantics. Shelving slope is independently bounded to (0,1] for
monotonic profiles, gain -120..120 dB and frequency strictly inside Nyquist.
Use the low/high shelf and slope formulas in the
[W3C Audio EQ Cookbook](https://www.w3.org/TR/audio-eq-cookbook/), normalized
through the existing finite-coefficient/strict Jury stability constructor.
No new processing storage or hidden transition policy is introduced.

Gain replacement still preserves delay state without smoothing. Numerically
unrepresentable designs reject; consumers own headroom, modulation, denormal/
nonfinite handling and higher-order composition. No host/device deadline claim.

## Validation

Both shelf sides, three frequencies/slopes and five gains pass endpoint and
half-dB midpoint landmarks, reciprocal boost/cut and monotonic 512-bin grids.
Cascaded boost/cut impulses cancel across reset cycles within numerical tolerance;
block partitioning is identical and prepared loops have no counted allocation.
Invalid settings reject. This is fixture evidence, not exhaustive all-boundary
accuracy or extreme-gain/headroom validation.
