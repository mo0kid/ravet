# 0158: Explicit VST3 block-start transport mapping

Status: implemented; normal/fast-math and macOS arm64 SDK format validation passed.

## Context

The reusable native processor accepts canonical transport, but SDK reference
processing previously passed an unknown/default snapshot. SDK optional context
flags do not guarantee every field, and some SDK observations cannot fit the
current native record without inventing metadata or changing its stable layout.

## Decision

Add optional external-SDK C++17 `nativeTransport(context, output)` and immutable
setup-time requirements for project music, cycle music, tempo, signature and
transport state. Null context maps to a canonical unknown snapshot. Non-null
context validates its always-valid finite positive sample rate and copies exact
i64 project samples. Playing and details become explicitly known, with recording
and active-loop state. SDK flags gate finite f64 musical position/positive tempo
and positive signature components that fit u16; no narrowing/truncation occurs.

Advertised cycle endpoints must be finite, and active endpoints ordered. Active
bounds map only with a known musical position, satisfying native v1 dependencies.
Inactive bounds (including zero extent) and positionless bounds are omitted;
active-loop state remains known. SDK bar start has no exact bar number, so no
native bar pair is guessed. Unavailable/unmapped values and unknown future SDK
flags are ignored. System/continuous time, SMPTE/frame rate, clock/chord, tempo
increment, sample loops, discontinuity and pre-roll have no mapping here.

All supported scalar validation precedes writes, preserving failed outputs. Shared
IEEE finite classification reads referenced memory as integer bits, preserving
foreign NaN/infinity rejection under fast-math; unavailable signed-zero fields are
canonical zero and used signed zero survives. Normal mapping uses no callbacks,
heap actions, locks, I/O, extrapolation, rounding or automatic reset.

Gain declares the immutable SDK requirements and maps context before complete
restore/queue visitor/DSP/publication. Canonical transport is passed through the
native client to its processor. SDK context lifetime/alignment remains caller-owned.
Gain ignores musical context, so this validates delivery/composition, not a
transport-driven synthesis feature. Further native record versions can represent
additional independent timeline metadata without altering v1.

## Validation and limits

Normal and fast-math fixtures cover1024 availability profiles, exact i64 extrema,
f64 signed zero, u16 bounds, null/partial/omitted fields, unavailable malformed
values, whole malformed rejection and10000 zero-C++new/delete mappings. SDK
contracts exercise requirements, malformed context before processing/recall and
valid context at both precisions. Rebuilt formats are recorded after gates.
No sample-accurate tempo curves, position-based discontinuity inference, unsupported
context fields, DAW timeline observation, device/deadline/platform or production
signing claim follows. Native codec evidence is independent of this SDK mapping.
