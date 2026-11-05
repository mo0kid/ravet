# 0294: Canonicalize Float64 residue in AU transport sample positions

**Date:** 5 November 2026  
**Status:** Implemented in `RavetAuTransport.hpp` (prototype). Amends the
exact-integral sample rule of [decision 0188](0188-auv2-block-start-transport.md).
Evidence: macOS arm64 REAPER 7.82 and REAPER 7.82/OSX64 under Rosetta 2,
synthetic and actual-component contracts; no further DAW claim.

## Context

Decision 0188 accepted a host's `TransportState`/`TransportState2` sample
position only when it was an exact integral Float64. The generic AUv2 adapter
treats a rejected transport collection as a malformed block: it outputs silence,
returns an error and does not call the product, so the product's state does not
advance for that block.

REAPER 7.82 derives the position from seconds × rate in Float64. For whole block
starts it reports values such as `3584.0000000000005` and `13823.999999999998`,
one unit in the last place from the integer. In a 48 kHz/512-frame offline
render, 147 of about 1,200 blocks were rejected. Every rejected block left the
delay line unadvanced, so the delay's echoes arrived 512 frames later per
rejected block between impulse and echo: +6,848 instead of +4,800 (fixed
100 ms), and +12,672/+14,208 instead of +9,600 (synced eighth at 150 BPM).
Dry signal positions stayed exact because the impulses fell in accepted blocks.
Logic Pro reports exact integers and was unaffected. CLAP, VST3 and LV2 renders
in REAPER were exact; those adapters do not use this AU collector.

## Decision

`nativeTransport` rounds a finite sample position to the nearest integer when
it lies within `min(2^-10, max(|n|, 2^20) × 2^-40)` samples of that integer
`n`. Any position further from an integer, such as `0.5`, still rejects the
collection. The tolerance is at least 2^-20 samples, which covers a few
thousand ulps near zero. It grows with magnitude at about 2^12 ulps and never
exceeds 1/1024 sample. Values of 2^52 and above are always integral and are
unchanged. The `[-2^63, +2^63)` range check applies to the rounded value.

The rejection policy for genuinely malformed tuples is unchanged.

## Consequences

- The generic AUv2 adapter and the performance AU (the only consumers) now
  process REAPER's blocks. The VST3 projection receives the canonical integer.
- A host that deliberately reports sub-sample positions finer than the
  tolerance has them rounded. Positions further from an integer still fault
  the block, as before.
- Tests: `tools/au-validation/transport.cpp` adds the observed REAPER values, a
  near-zero residue, a residue beyond tolerance and a large value beyond the
  absolute cap (normal and `-O3 -ffast-math`). The delay's
  `tools/au-plugin-validation/delay.mm` replays REAPER's 48 kHz/512-frame
  pattern, and its fixed and synced echoes must be exact. The generic
  `contracts.mm` host callback reports 456 plus one ulp for every product and
  mock. The mock checks that it receives exactly 456.
