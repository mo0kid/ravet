# 0261: Exact LV2 native core and shared schema admission

Status: Accepted; reusable source adopted with scoped local qualification.
Fresh integrated public reference and programme acceptance remain separate.

## Decision

Use the canonical exact processor pointer exposed by the cached parameter schema
and require state and cache companions to expose that exact shared parameter
schema. Remove callback-equivalence matching, which admitted copied core tables
and separately copied schema metadata. W236 canonical runtime table acquisition
makes these pointer comparisons reliable across code generation boundaries.

Query each metadata accessor once on CONTROL before any cache, publication or
DSP creator. A violating metadata-query exception refuses the helper before
acquisition; native ABI calls otherwise retain their nonthrowing contract.
Constructor signature, f32/default ports, state/publication/control precedence,
restore serialization, save concurrency and processing lifecycle stay unchanged.
This intentionally changes acceptance of copied core/schema inputs.

Do not claim an identity oracle for top-level cache/state table addresses: no
existing accessor declares those addresses. Coherent top-level copies preserving
the actual selected core/shared schema remain accepted. An additive bundle is
unnecessary for the identities the existing ABI can authoritatively establish.

## Evidence and limits

A maintained native fixture uses callback counters and real Gain native handles
to prove creator0 refusal for byte-identical copied cores, cache/state copied
schemas, null/throwing metadata and unchanged invalid-helper callback storage.
Positive coherent/genuine exported origins process and retire exactly once.
Both bundle profiles include this gate, propagate refusal and retain actual
Gain/Performance module audio/state/MIDI/transport regression requirements.

Isolated maintained CMake7/7 and actual self-contained Gain3/Performance5
validator bundles pass; bundler Python4 includes mandatory origin-gate refusal
propagation. Exact qualification receipts accompany W248; isolated archives do not establish
fresh public qualification. No new precision/bus/event/UI profile, host routing,
installation, SDK vendoring, DAW/other-platform or deadline claim is made.

See [the LV2 guide](../lv2.md).
