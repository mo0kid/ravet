# 0110: Explicit packed native audio port graph nodes

Status: implemented fixed f32/f64 native audio profile.

Keep original main-only factories unchanged. Add explicit packed-port factories
returning the same precision-specific node types and main control. Inspect ports
while inactive, validate each directional channel sum against the consumer graph
layout, then allocate nested port/channel scratch and native pointer capacity on
main before activation. Metadata remains bounded to nine ports per direction;
summed channels remain at most 64 per direction. Same 32/64 MiB scratch limits
exclude graph/plugin storage. Every f64 port must advertise native support.

Native enumeration/channel order becomes packed graph channel order. Do not infer
portable auxiliary pairing from directional native metadata: graph routes address
packed main indices, foundation auxiliary arrays remain empty. Inputs/outputs stay
separate and native port/channel boundaries are preserved by typed buffer builders.
Consumer selects layout/timing and observes one-shot lifecycle. No layout changes,
events/transport/editor/active parameter/device scheduling are added implicitly.

Linked input/output groups with distinct per-port processing test both precisions,
wide values, summed layout/auxiliary precision rejection before activation, heap
and exact owner retirement. The actual exported Sidechain reference passes f32/f64
main stereo plus detector input, alternating gate samples and dynamic wet/dry
routing/stop/main retirement. Main-only actual-binary regressions remain intact.
Evidence is scoped to macOS arm64 reference binaries, without broad compatibility,
platform/device or physical deadline claims.
