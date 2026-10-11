# Prepared native AAX audio buses

This optional public-SDK helper prepares explicit planar f32 main/auxiliary audio
layouts for the owned native processor. It does not register or discover host
routing. The local validation fixture authors SDK descriptors, initializes their
registered factories using public proxies, and invokes their registered algorithms
against the portable Rust bus-routing processor. Public Gain and Performance keep
their existing main-only profiles. Pro Tools, signing, installation and audio
latency/deadline validation remain separate.

## Layout and ownership

`RavetAaxAudio.hpp` accepts one main plus at most eight auxiliary buses and at most
64 total channels in either direction, with 1–1024 prepared frames. This is the
helper's total-channel limit; the underlying native ABI permits up to 64 channels
per bus. Each bus preserves native source order, including input-only and
output-only buses. Output pointer slices follow source order, skipping buses with
no outputs. Input slices are explicit static indices or one dynamic sidechain
slice selected by the SDK field. Main input starts at zero. The sidechain index is
an index into the declared input pointer-array capacity, not its length. Negative,
missing, overlapping or out-of-capacity slices reject; disconnected/inactive
sidechain behavior is not inferred.

All tables, callback libraries, processors, SDK proxy objects and foreign ranges
remain live through every borrower. CONTROL serializes lifecycle, SDK parameter
intent and complete state work. The host stops and joins AUDIO before configuring,
preparing, resetting, uninitializing or copying complete diagnostics. AUDIO owns
one prepared processor and the borrowed sample ranges exclusively during a call.
The mapper owns fixed pointer metadata, and copying a mapped view retargets its
native bus arrays to the destination's own storage.

Raw AAX pointer arrays expose no byte capacity. Callers supply live, aligned arrays
of the exact prepared input/output capacities and live sample prefixes for the
supported frames. Whole metadata, sample extents, address overflow and output
alias/partial overlap checks precede view publication or sample access. Inputs may
alias outputs across buses; the authoritative native adapter copies all inputs
before output writes. Outputs cannot overlap each other or mapper/view/owner,
input/output pointer metadata, MIDI metadata or supplied callback protected ranges.
Rejected mapping preserves the prior mapped view's bytes. Supported silence is a
separate operation over safe declared output prefixes, clamped to prepared frames;
it protects metadata and storage beyond the union of those prefixes. For two
partially overlapping output bindings, a location inside either supported prefix
may be cleared even if it is beyond the other binding's prefix.

A zero-frame call may omit both audio pointer arrays. No sample pointer or dynamic
sidechain selector is consumed in that case. Non-omitted zero-frame calls retain
metadata and index checks without reading sample pointers. Queued controls or
notes still cannot be applied at zero frames under the existing owner contract.

## Owned processor and SDK composition

`NativeAaxProcessor::prepareLayoutStopped(layout, sampleRate)` accepts the complete
explicit layout. Known shape/rate/staged-intent rejection precedes backend work and
retains previous readiness and configuration. Once authoritative native prepare or
complete intent application runs, an error retains its original native status and
fails closed; processor history is not rolled back. Successful stopped prepare
resets transient DSP history, restores acknowledged complete parameter intent and
clears the prior queued commands. `layoutStopped()` returns a copy. The existing
`prepare(channels, rate, frames)` still selects the original main-only profile.

`processLayout(inputs, outputs, frames, sidechainFirst, ...)` maps every prepared
bus before MIDI, combined event capacity, transport collection, complete-state
application, queue dequeue or DSP. The fixed prepared path has no C++ heap work.
This claim does not cover arbitrary SDK callbacks, Rust consumer hooks or an audio
deadline. AUDIO's exact latest status is separately retained atomically for CONTROL
observation. Backend failures fail closed under the existing lifecycle contract.

The default final `Multibus=false` argument of `NativeAaxEffectParameters` preserves
the existing effect/instrument Context types and layout. Explicit `Multibus=true`
selects effect fields owner0, frames1, inputs2, outputs3, sidechain index4. Combining
that Context with `Instrument=true` is deliberately rejected at compile time;
a MIDI instrument auxiliary context requires separately designed registration.
`PreparedProfile` pairs a native layout with exact requested main SDK stems.
The SDK model supports None/mono/stereo main stems only; it does not infer stem
formats from arbitrary channel counts. Initialize compares actual effect identity,
stems and positive sample rate before preparing the authoritative native table.

`configureProfileStopped(profile)` is checked and CONTROL guarded. It changes the
selected native layout only; it does not alter host descriptors or authorize new
metadata capacities. Descriptor registration and the caller's context must already
match the selected profile. Known profile or SDK stem rejection preserves the old
profile/readiness. Authoritative setup failure makes the owner unusable; the model
keeps the last successful profile for fresh Uninitialize/Initialize recovery.

The registered fixture declares sidechain field4 using `AddSideChainIn`, and its
output-only mono/stereo monitor using `AddAuxOutputStem` on output field3. Descriptor
calls follow native source order. Original SDK registration errors are forwarded
before callback/effect publication; prior foreign calls have no rollback. Every default effect, MIDI instrument and explicit multibus callback protects
frame, owner-pointer and outer/context metadata before consuming frame values;
wholeOwner frame alias rejects before dereference or MIDI getter. Explicit multibus
also protects sidechain metadata. All these ranges are protected against
sample writes. They must be valid live typed objects and point to the exact owner;
this contract cannot detect arbitrary invalid addresses or authenticate ownership.

## Portable routing reference and evidence

`ravet-bus-routing-native` supplies paired precision-specific processor, schema,
cache, complete audio-state, codec and transient telemetry tables. Its portable
processor accepts mono/stereo matching main input/output and exactly two auxiliary
buses: one mono input-only detector, one mono/stereo output-only monitor, in either
source order. Main output is input × Gain; monitor output is detector × Gain,
duplicated for stereo. Bypass copies both at unity. Gain1 is transparent; sample
values pass through unsanitized. Timed physical controls obey whole-block validation.
The reference implements f32/f64. Default AAX processing uses its f32 native
table; the explicitly selected [internal precision adapter](aax-precision.md)
uses the originating f64 table behind the unchanged host-f32 bus map.

Gain ID1 is 0–4, default1; Bypass ID2 is discrete 0/1, default0. Complete global
state is the native 32-byte RAVETBUS/version1 shape. Exact processed-frame telemetry
is a separate saturating u64 count, clears on prepare/reset, and survives complete
parameter recall. It is not a clock, persisted state or automatic host publication.
The validation fixture forwards actual Rust calls through coordinated originating
tables, retains an opaque handle, and queries the public telemetry API under
exclusive stopped ownership; it does not cast private processor/SDK layouts.

Local validation passes five Rust routing/precision/state/layout/export tests,
warning-denied Rustdoc and Clippy, and fourteen focused CMake normal/fast-math
contracts. These include the initialized eight-layout SDK routing fixture,
unchanged Gain/Performance owners and Gain model, registered public default
factories with common metadata guards and paired smoothing/voice-phase oracles,
and 10,000 maximum-size/late-fault mapper cycles with zero counted C++ heap work.

The normal native Gain package passes 26 CTest contracts, the AAX export/version
probe and its actual registered module tests, including default callback metadata
faults and independent retained state/history. Performance passes four CTest
contracts and its actual registered module/voice-phase checks. Stock Gain passes
14 CTest contracts and its actual registered stock schema/control/algorithm probe
using an explicit independent build cache. Stock initial mono lifecycle limits
remain separate from the owned native model. Eight Python packaging tests, focused
public Rust tests/lint/Rustdoc and strict documentation checks pass. These are local
SDK/native/package receipts; they do not establish Pro Tools host acceptance.

The package command remains `python3 scripts/bundle-aax.py --vst3-sdk <external-vst3-sdk> --aax-sdk <external-aax-sdk>`; native Gain additionally builds the public bus-routing
archive for mandatory helper/actual multibus validation. It remains main-only as a
plug-in reference. Stock Gain and Performance do not build/adopt that archive.
Direct native Gain CMake configuration must supply `RAVET_BUS_ROUTING_LIBRARY`;
missing actual routing archive is fatal rather than skipping those contracts.
