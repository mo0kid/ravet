# Owned raw short-MIDI AAX effects and auxiliary buses

The opted-in `RawShortMidi` AAX processor/model profile accepts complete canonical
short MIDI alongside main audio, prepared auxiliary buses and optional dynamic
latency. The ordinary effect, note-only instrument, generated instrument and
multibus default contexts keep their field layout and policy. Raw generated
output is an explicitly rejected template combination: this profile is input-only.
The no-input raw instrument remains available through its existing context.

## Prepared processing and registration

`NativeAaxProcessor::processRawLayout` and its admitted form accept planar host-f32
inputs/outputs, a live stream, optional sidechain input index and known protected
metadata. `HasMidiInput` selects the prepared combined MIDI/control capacity;
legacy non-MIDI effects retain their control capacity. Calling legacy
`processLayout` on a raw profile refuses rather than silently dropping MIDI.

Distinct raw effect/multibus contexts append a notes field to their respective
legacy prefix. The registered callback acquires one authoritative lock-free AUDIO
admission token for raw, generated or dynamic-timing opted-in profiles. Nested or
concurrent callbacks return Busy before a MIDI getter, storage, native processing,
diagnostic or host-audio mutation. A failed admission cannot clear another live
callback's token. The prior convenience admission APIs use that same profile flag.

Known Context, context-pointer list, owner-pointer, frame, optional sidechain,
input/output pointer arrays and `sizeof(AAX_IMIDINode)` base spans are admitted
before the virtual MIDI getter. A node may not overlap those spans or its owner.
The live stream header and complete packet span are checked against both
directional pointer-array metadata before copying. The prepared mapper rejects
sample/pointer metadata aliases with the node, stream, packets, callback metadata
and owner. Actual typed object liveness and array/sample capacity remain caller
obligations; the adapter never guesses a derived node's private extent. A known
fault silences only independently admitted writable prefixes, preserving protected
metadata and backing tails. Both raw main/aux directions support valid shared
input/output buffers through whole source capture.

The local RawRouting reference registers six exact public IDs under
`org.ravet.rawrouting`: mono/stereo main, two stereo detector/monitor source orders,
and mono/stereo timed delay. Registration uses checked public `AddMIDINode`, one
LocalInput node, channel mask 0xffff and the exact selected context field. Auxiliary
profiles register one input-only detector and one output-only monitor in the
prepared source order. This demonstrates reusable SDK mapping and initialized
callbacks, not Pro Tools routing. The external SDK bootstrap uses an authored
zero-class VST3 factory; its unrelated stock VST3-backed parameter creator is
unsupported. No SDK source is copied into RAVET.

## CONTROL publication and MIDI order

One CONTROL intent producer and one AUDIO consumer use the existing SPSC queue.
For raw profiles, ordinary acknowledged scalar updates and validated complete
state can be published while AUDIO is inside a getter or native DSP. A host
`SetParameterNormalizedValue` success means request acceptance only: the matching
acknowledged model update determines DSP intent. Parameter-manager access retains
its SDK caller/thread requirements. CONTROL reads its own intent/staging and
atomic queue indices; it does not inspect AUDIO diagnostics or event scratch.

Stopped lifecycle, profile, telemetry and timing-candidate operations require the
caller to stop/join AUDIO and serialize CONTROL. An admission-bit check detects an
already entered callback; it does not provide atomic exclusion against a newly
starting callback. These operations are not a general cross-thread control lock.

Whole block preflight validates every packet, effective trailing control and both
audio directions before complete state application, dequeue, transport collection
or native DSP. A last accepted complete restore supersedes earlier intent; its
trailing controls retain original order. At equal offset 0, effective queued
controls precede original MIDI order. Total controls plus MIDI must fit the full
prepared capacity. Late malformed packets, excess combined capacity, metadata,
layout and transport refusal preserve pending intent and native history.

All defined complete one-, two- and three-byte channel/system messages use the
existing canonical policy, including standalone F7 as EndOfExclusive only. F7
never infers a SysEx stream; SysEx payloads, undefined statuses, malformed lengths,
non-seven-bit data, nonboolean immediate flags and invalid order/offsets are refused.
The SDK compatibility contract identifies timestamp 0 as immediate while the flag
is currently unset; boolean 1 remains accepted with authoritative ordered timestamp,
including valid offset 0/zero-frame use. The note-only defaults retain their narrower policy. Raw offset 0 events and
queued controls are valid for zero frames, with no audio/FIFO frame advancement.
Native kind14 contains exact wire/channel bytes but no MIDI source-port identity;
this profile does not merge unidentified nodes or invent port provenance.

## Native consumers, precision and state

Public `RawBusRouting<f32/f64>` and `RawDelayRouting<f32/f64, DELAY>` compose the
existing routing/FIFO kernels. Bus routing accepts mono/stereo main alone or the
prepared detector/monitor pair in either source order. Gain/Bypass affect main and
monitor samples. Fixed delay retains its declared FIFO length and Level/Bypass
behavior. Raw bytes are observed without implicit MIDI-to-parameter binding.
Legacy BusRouting and Delay policies remain unchanged.

Each reference family exports exact selected 32/64 core, schema, cache, audio-state,
complete-state and optional telemetry tables. Complete schema/companion origin,
shape, required functions and counts are checked before resource creators. A
callback-equivalent table copy is not the selected origin. Acquisitions and failed
candidate ownership are retired on CONTROL. Bus complete state is `RAVETRBS`,
delay is `RAVETRDL`, version1, with two globals; MIDI/FIFO observations are transient.
Ten telemetry IDs report frames, packet totals, last ordered wire checksum/class
mask/count, acknowledged Gain/Level and Bypass, supplied native transport flags,
and mixed-event checksum/count. See `RavetRawRouting.h` for exact checksum bytes.
Snapshots require stopped/joined AUDIO and full caller capacity.

The selected64 adapter retains the host-f32 contract, exact native64 origin, whole
source capture and deterministic conversion policy. Structural/input preflight
precedes native history; data-dependent output narrowing occurs after native DSP.
A narrowing fault validates all outputs before copying any, silences safe prefixes
and poisons readiness, without claiming history rollback. Nonfinite/subnormal and
rounding behavior follow the owned precision adapter's existing policy. The last
actually returned native code remains separate from the latest attempt's local
preflight, exception or conversion error; diagnostics require stopped observation.
Early admitted Context metadata refusals replace old success flags with fresh
Preflight, including native64 precision flags. The native last-return numeric
starts at 1 with no has-ever-returned bit, survives lifecycle calls and can be
ambiguous with a returned 1. Per-block stopped diagnostics are not lifetime history.
The maximum event profile uses fixed stack scratch; the raw instrument's measured
Clang owner+collector subtotal is about 169 KiB before opaque host/native frames,
not a portable bound for all layouts/builds. Small audio-thread stacks are unqualified.

## Dynamic timing and validation

Raw timing candidates explicitly supply their own exact selected companions and
optional telemetry. Delay8 continues its old DSP/FIFO while Delay16 is pending;
only a matching latency ACK plus successful getter reconciliation authorizes
publication. AUDIO adopts the prepared zero-history candidate with coherent
current/queued globals, then CONTROL retires the old native owners. Host refusal,
compensation disabled, candidate acquisition/setup failure and wrong ACK remain
explicit. A mismatch is sticky faulted readiness, not an implicit fallback.
No generic host tail API, signing or Pro Tools timing behavior is inferred.

Isolated actual canonical Rust tests cover both precisions, all 123 defined short
statuses, exact ordered/mixed checksums, zero-frame behavior, empty main auxiliary
views, independent f64 resolution, legacy refusal and late state/history faults.
Fresh production CMake32/64 bundles pass five maintained tests each: four
normal/fast-math native and registered module validators plus the normal/fast
raw-generated compile-time refusal gate. Runtime checks cover SDK initialization/factories, all six contexts, both auxiliary
orders, metadata/getter0 faults, valid in-place audio, nested/concurrent admission,
mid-DSP SPSC publication, backend refusal/throw and Delay8->16 ACK adoption. Maximum
1024-frame/packet loops count zero ordinary C++ heap allocations/retirements; this
is bounded callback evidence, not a measured deadline or every allocator audit.
Packaging verifies notices, required exports/version and local ad-hoc signatures.
The focused isolated Rust suite passes 21 tests plus warning-denied Clippy/Rustdoc;
15 bundling Python tests cover profile selection and signature refusal ordering.

These are local macOS/external-SDK fixtures. ACF proprietary host registration,
Pro Tools/signing, other platforms, MIDI2/SysEx/source ports, generated raw output,
AudioSuite and arbitrary consumer/UI contracts remain separate. Exact public
promotion and the shared workspace/default reference gates are still required;
the isolated receipts do not qualify stale public archives.
