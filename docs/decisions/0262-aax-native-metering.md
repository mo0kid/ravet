# 0262 — Explicit prepared AAX native meter taps

Status: Implemented; publicly adopted with the W234/W241/W249 AAX chain. Local
macOS arm64 package and workspace evidence only.

Native telemetry provides independent schema and snapshot ownership. AAX meter
fields and descriptions require an actual public SDK registration and host-f32
scalar storage. Neither a telemetry helper alone nor default Gain rendering
establishes host meter publication.

Append an opt-in bounded meter count and bindings to the existing native AAX
owner/model. Preserve Count0 default Contexts/IDs and the existing raw, generated,
timing and native64 contracts. Use public typed field registration, complete
native metadata copied on CONTROL, one unavoidable temporary cache before other
owners, exact originating core pointers, and the same admitted AUDIO token.
Whole foreign tap/metadata/audio preflight precedes getters/history; a full native
snapshot and all explicit projected words precede any tap publication.

Keep original native/SDK statuses and nonreturning exceptions separate. A
post-DSP projection/snapshot failure preserves prior taps and completed successful
audio but cannot restore native history. Preserve supported meter-only/raw SPSC
controls; stopped lifecycle remains caller stop/join with fail-closed live
admission. Timing candidates require explicit compatible telemetry, retain late
metadata refusal details, and adopt only after host ACK; retirement remains
CONTROL-owned. Do not infer dB, clipping, decay, output rollback, host tail or
memory-allocation extents from native metadata.

Supply a distinct opt-in Gain meter factory/bundle using canonical Gain32/64,
plus meaningful actual SDK instrument/raw/aux/timing composition fixtures and
maximum no-C++-heap tests. External SDK infrastructure stays external; the small
authored AAX-only bootstrap exposes no unrelated VST3 classes. Local ad-hoc
signing/SDK module execution does not qualify Pro Tools or production signing.

See [AAX meters](../aax-meters.md). Frozen dependency packages R4/W241 are explicit
composition inputs; fresh public/reference/shared gates remain required before
public acceptance. Historical failing qualification oracles remain separate
from the final corrected results.
