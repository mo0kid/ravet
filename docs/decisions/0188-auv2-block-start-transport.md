# 0188: AUv2 block-start host transport preflight

Status: prototype; direct collector/projection, authoritative native processing
and normal self-contained reference-component contracts pass locally.

The stock wrapper narrows floating host sample coordinates and signature
numerators without scalar preflight and can pass a zero nonnull context when
callbacks are absent. A reusable Apple-SDK collector now validates supported
callback tuples before publishing a canonical native snapshot. Independent
successful callback availability remains explicit; absent/error tuples stay
unknown. TransportState2 is preferred when supplied, with no fallback after an
error. Successful incomplete callbacks reject atomically, with every requested
return field initialized to an invalid sentinel first. Sentinels use integer-bit
initialization and finite checks inspect IEEE representations under fast-math.

Samples are finite exact integral Float64 values in `[-2^63,+2^63)` before any
integer cast. Positive `+2^63` is rejected rather than comparing against a rounded
Float64 INT64_MAX. Signature numerator is finite integral 1..65535 and denominator
is 1..65535 before conversion. Beat is finite, tempo finite positive and transport
Booleans exactly 0/1. Active loop bounds are finite ordered beats, published only
with known musical position; inactive irrelevant bounds are ignored. Host Float64
precision is inherited, without rounding, extrapolation or automatic reset.

Native DETAILS groups multiple flags. The read-only snapshot therefore retains
callback kinds to distinguish legacy unknown recording from TransportState2 known
recording. Native signature requires DETAILS in the canonical ABI, so signature-only
snapshots also carry DETAILS with unknown/default flags. Native DETAILS has no
per-flag availability: zero pre-roll for every profile and zero recording for
legacy are unknown/default placeholders. Consumers
must interpret callbacks() when that distinction matters, rather than treating
every false DETAILS flag as host-known false. Actual changed/discontinuity and
cycling survive the direct snapshot;
pre-roll, sample loops and exact native bar-number pairs have no AU callback
mapping in this profile. The helper requests neither clock offset nor bar start.
Foreign callback/user-data lifetime and consistency across callbacks are host
obligations; only serialized AudioUnitRender collection is permitted.

A separate external VST3 projector requires actually known playing and samples
for a nonnull context, because that format implies availability of both. It
returns no context for incomplete transport, explicitly omitting partial musical
information from the bridge while retaining it in the direct native snapshot.
Representable music/tempo/signature/active beat-loop and recording fields survive;
discontinuity and exact callback availability cannot. The snapshot exposes only
read access, so integer projection consumes previously validated signature fields.
No system clock, clock/bar pair, sample-loop or pre-roll data is invented.

Performance AU replaces its inherited context update inside owned Render after
note/audio checks and before parameter transfer/DSP. Malformed successful host
callbacks reject and clear supported caller prefixes while retaining accepted
notes/controls and native history. Callback errors/absence remain valid unknown
transport. AUBase still owns render timestamp caching, including failed renders;
recovery tests use a new timestamp and do not promise retrying a cached timestamp.
The native performance DSP ignores transport; this proves scheduling/fault
boundaries rather than transport-driven musical behavior.

Existing native performance zero-frame processing validates complete,
signature-only, beat/tempo-only, legacy-only and unknown collected snapshots
through the authoritative Rust codec. Deliberately corrupted dependencies,
reserved fields and unavailable coordinates reject through the same native ABI.
Direct synthetic callback tests include absent/error/partial tuples, initialized
missing fields, State2 precedence and legacy recording, exact signed sample
boundaries, fractional/nonfinite samples/signatures, invalid Booleans and active
loop order, independent native availability and explicit SDK bridge omissions.
Four thousand accepted plus rejected cycles use zero counted ordinary C++ heap
actions, including optimized fast-math. Actual component callbacks prove queued
timed MIDI and Level survive a malformed sample and recover at exact -2^63, with
mono/stereo silence/tail/count/initialization checks and existing 200 voice cycles.
These local macOS arm64 checks claim no DAW timeline behavior, callback coherence,
all-host/backend heap behavior, real-time deadline, auval/sandbox/device, another
architecture or production distribution support. External SDK source remains
outside RAVET.
