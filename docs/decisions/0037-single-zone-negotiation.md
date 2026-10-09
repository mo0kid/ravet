# 0037 — Explicit single-zone negotiation

Date: 9 October 2026. Status: implemented within stated limits.

The static voice allocator must not silently change its configured zone. Add a
separate opt-in receiver with a fixed chosen side and a reusable per-channel RPN
decoder. This keeps prior receiver behavior stable and protocol state bounded.
RPN6 coarse data enables/disables the selected zone and clears all voices and
controllers; report occupied slots so a DSP owner stops them. Reset ranges to
48/2, or 2/2 when disabled. RPN0 coarse data updates shared member or independent
master ranges and active snapshots. Released tails keep their old expression.

Zone interpretation and pitch-range defaults follow the MIDI Association MPE
specification sections 2.1/2.4, linked from the MIDI guide. Fractional ranges,
other-side zones, MIDI-CI and full receiver conformance are outside this API.
Zero-member fallback is conventional polyphony; caller owns all DSP behavior.
Selectors and apply/reset remain fixed-storage and allocation-free. Fixtures
cover both zones, invalid/cancelled selectors, range refresh, reset masks and
10,000 workloads with no Rust heap operations; hardware remains unvalidated.
