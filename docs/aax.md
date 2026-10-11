# AAX references

The default local AAX reference uses an authored public-SDK data model and
algorithm around the public native Gain processor. It checks native preparation
and complete current state before publishing a processing-ready mono/stereo f32
owner. An explicit `stock` profile preserves the external Steinberg wrapper's
native-schema/control regression. Both keep SDK implementation outside this
repository. The default Gain reference remains main-only host-f32 with no MIDI
or AudioSuite. Explicit [internal precision](aax-precision.md) selects native64
DSP behind the same host buffers; an optional [native editor](aax-native-editor.md)
uses the originating selected-precision cache. Reusable auxiliary, timing and
generated-output mechanisms are documented separately. Explicit
[prepared host meters](aax-meters.md) append bounded telemetry-derived taps;
`--plugin gain-meters` supplies separate canonical Gain32/64 reference identities.
The explicit `--plugin performance` native profile supplies a distinct no-input
short-note instrument; see [Performance AAX](performance-aax.md).

```sh
CARGO_BUILD_JOBS=2 python3 scripts/bundle-aax.py \
  --vst3-sdk /path/to/vst3sdk --aax-sdk /path/to/AAX

# Separate regression profile; its stock initial mono lifecycle remains limited.
CARGO_BUILD_JOBS=2 python3 scripts/bundle-aax.py --profile stock \
  --vst3-sdk /path/to/vst3sdk --aax-sdk /path/to/AAX
```

The script builds with two jobs, runs native/controller/format contracts, adds
local development notices, ad hoc signs and loads the actual macOS module to
check AAX exports and SDK version. Performance uses
`target/aax/performance/release` and distinct public reference IDs. The default
Gain bundle remains under `target/aax/release`; stock uses
`target/aax/stock/release`. Those Gain alternatives contain the same public IDs,
so they are local serialized fixtures, not concurrent
production installs. No host directory is changed. The exercised external SDK
reports `0x20801345980`; only macOS arm64 is built/loaded.

Native IDs `RvG1`/`RvG2`, manufacturer `Ravt`, product `RvGn`, and effect IDs
`org.ravet.gain.mono`/`org.ravet.gain.stereo` remain the public reference identity.
Release requires collision review. The default prototype now uses complete native
chunk `RvS1`, including bypass, instead of the stock wrapper's combined VST3/AAX
chunk. This is a deliberate local prototype change. No released-session migration
or production state compatibility is established. No SDK precision/table export
implies a registered host format; this profile selects exact 32-bit companions.

## Checked native model and processing

The optional C++17 [processor owner](../crates/ravet-native/include/RavetAaxProcessor.hpp)
and [SDK model](../crates/ravet-native/include/RavetAaxEffectParameters.hpp) consume
matching native processor/audio-state/cached-schema/complete-codec companions.
The concrete [Gain model](../examples/gain-aax/GainNativeParameters.hpp) and
[descriptor registration](../examples/gain-aax/native-registration.cpp) use public
AAX APIs. Actual SDK Initialize queries effect identity, finite positive sample
rate and exact mono/stereo input/output stems. Original effect/rate/stem query errors return
unchanged; missing/invalid configuration never invents a rate or ready state.
The separate nativeStatus() observation retains stopped lifecycle codes; AAX uses
a different error domain. lastProcessStatus() is a lock-free atomic observation of
the latest AUDIO result, including original provider errors despite the SDK's void
algorithm callback. Neither observation is an ordered history or deadline receipt.

The explicit native references capture initialized public-SDK
[transport](aax-transport.md) before pending state/queue/DSP transfer. Exact raw
i64 ticks and rounded f64 PPQ, independent getter statuses and configured-loop
availability remain distinct; DETAILS does not supply per-flag availability.
The stock profile keeps its separate wrapper transport behavior.

The reusable [prepared audio buses](aax-bus-mapping.md) helper accepts explicit
main/aux layouts through a separate effect multibus context. Existing default
Gain/Performance contexts also protect callback/frame/owner metadata. The native
Gain validation command builds `ravet-bus-routing-native` and requires its archive
for actual initialized registered multibus contracts; Gain remains a main-only
plug-in. Direct native Gain CMake callers also supply `RAVET_BUS_ROUTING_LIBRARY`.
Stock Gain and Performance packaging keep their existing archive requirements.


CONTROL serializes all SDK callbacks, codec/staging and stopped lifecycle. One AUDIO
consumer exclusively owns DSP. Host callbacks, pointer-array/context storage,
frame-count and Owner-pointer fields must be live, correctly aligned, of declared
capacity and disjoint from sample buffers/owner storage except permitted sample
input/output overlap. SDK/native libraries, identity strings and borrowed objects
outlive all users. Hosts stop and join AUDIO before reset/reprepare/uninitialize or
destruction. A mutation guard rejects SDK callback reentry into retirement or other
mutations while host notifications borrow SDK objects. It does not make arbitrary
concurrent CONTROL callers safe.

The native profile maps one main planar f32 input/output bus of one or two matching
channels and at most1024 frames. Fixed preflight checks whole frame, metadata/sample
ranges, alignment/overflow, Owner-memory overlap and late output partial overlap
before queued intent/state/DSP changes. Output/input sample alias is permitted;
output channels cannot overlap. Known shape rejection preserves accepted queue and
native history, silences supported writable prefixes and preserves backing tails.
Malformed/missing outer SDK context fields do not create a safe writable output
contract. An invoked provider error fail-closes readiness and retains its exact
AUDIO result; arbitrary provider history rollback is absent. Fresh stopped native
preparation is required for recovery. Tables and foreign pointers remain trusted;
these checks are not binary isolation or arbitrary memory-validity proofs.

ResetFieldData checks field0/exact pointer storage/alignment and publishes only an
already-ready owner. It does not reset DSP implicitly. Explicit resetStopped and
reprepareStopped check native returns; successful operations restore the latest
accepted complete CONTROL intent and coalesce/discard its queued commands. Invalid
known configuration preserves an earlier profile before backend mutation; a failed
backend setup leaves callbacks fail-closed. Failed Initialize requires Uninitialize
before retry; repeated live Initialize rejects before proxy replacement.
Retirement invalidates readiness before native/SDK borrower cleanup.

## Controls and complete global state

The owner has a fixed single-producer/single-consumer ring with power-of-two slots
from2..1024 (default32; capacity slots-1). Complete scalar/capacity preflight reserves
before SDK acknowledgement mutation. SDK SetParameterNormalizedValue is a host
request; UpdateParameterNormalizedValue acknowledges canonical native intent.
Prepared scalar staging uses the immutable native cache; complete codecs are
CONTROL construction/save/restore work. Positive-frame AUDIO blocks consume accepted
block-start parameter events. Zero-frame blocks with pending commands reject before
state/DSP transfer. Queue overflow rejects without overwrite. Readiness-only failure
invalidation does not perform DSP lifecycle concurrently with AUDIO.

Complete restore validates and decodes an isolated native codec before SDK objects
or queue intent change. The newest queued complete restore supersedes older global
commands; following parameter commands retain source order. CONTROL save includes
acknowledged queued global values before render, excluding voices/history. Complete
fixed native state is persisted with exact outer manufacturer/product/native/chunk
identity, data size and version1. GetChunk accepts matching caller-populated identity
and capacity, supplies version/name/canonical data; restore and comparison require
the version too. All chunk storage must cover the declared header and data.
Unknown identity/version/shape or malformed payload rejects before SDK/native intent
mutation. SDK objects' void value updates can issue arbitrary foreign notifications;
completed foreign sideeffects and partial exceptional SDK updates have no rollback.
SDK registration/automation objects retain their public void behavior; delegate
statuses erased by those wrappers cannot be recovered as new checked successes.
An exceptional/failed SDK mutation cancels the reservation and fail-closes until
stopped retirement/fresh initialization restores accepted native intent.

The default Gain model requires writable global controls, including a unique native
bypass. Performance instead has Level-only control and no invented bypass. Separate
reusable adapters cover [auxiliary routing](aax-bus-mapping.md),
[acknowledged latency](aax-timing.md), [generated short MIDI](aax-generated-midi.md),
[native editor control](aax-native-editor.md) and [internal precision](aax-precision.md).
DSP-generated/read-only reflection, a factory program catalogue, a generic host tail
declaration and AudioSuite remain distinct scopes. Offline notifications only record mode
metadata; this native ABI has no processing-mode field and does not reprepare DSP or
claim mode-specific behavior.

The optional external-SDK C++17 [native parameter helper](../crates/ravet-native/include/RavetAaxParameters.hpp)
uses a shared immutable native cache and stages actual SDK objects. Full native
IDs map to exact `p` plus uppercase hexadecimal without leading zeroes; no prefix,
suffix, lowercase or `Byp` aliases are accepted. Scalar double intent must be
finite and in0..1, explicitly narrows to f32, and returns widened native canonical
f32 normalized values after physical range/step mapping. Read-only controls reject
updates and suppress SDK automation. Continuous SDK objects retain128 steps;
discrete objects declare intervals+1. Physical shortest f32 text and Boolean
On/Off parse completely, with bounded ASCII input and no implicit unit suffix.
Metadata names/units retain complete bounded UTF8 bytes. Caller SDK visual display
is separate. Construction/text/SDK registration and retirement are serialized
CONTROL operations; prepared scalar preflight makes no C++ heap allocation in the
exercised100000-cycle valid/late-fault test. Retain table/library and all borrowed
SDK objects through use. Default schema precision is32; explicit64 selects metadata
for its originating native table. The schema helper alone does not process audio;
explicit [native64 processing](aax-precision.md) uses the separately checked owner
and matching audio-state/codec tables behind host-f32 buffers.

SDK AddParameter may append its owning vector before a failing map allocation.
The helper checks public indexed adoption and retires adopted objects only through
public SDK removal; accepted prefixes clear on failure. Cloned display adapters
retain shared immutable cache ownership until the final borrower retires. Known
shape/ID faults precede insertion. Use an exclusively serialized empty manager
and preserve the exact staged SDK definitions; foreign notification effects have
no rollback. Foreign AAX_IString Set failures likewise have no atomic rollback
promise. [Decision0209](decisions/0209-aax-native-schema-control.md) records these
ownership and callback limits.

## Stock regression profile and evidence limits

The explicit stock profile retains [GainParameters](../examples/gain-aax/GainParameters.hpp)
and the [registration forwarder](../crates/ravet-native/include/RavetAaxFactoryRegistration.hpp).
It replaces stock SDK objects with the complete native set and preserves exact
IDs/physical text/read-only/Boolean roles. This consumer rejects the reserved
Steinberg UINT32_MAX parameter ID; the reusable native helper represents it. Its
factory substitution uses exact AddEffect identity, changes registration timing,
forwards original statuses and retains unknown callbacks. The stock aggregate keeps
its last-variant result and may mask an earlier refusal. Native descriptor registration
instead stops on the first exact refusal; neither rolls back prior host calls.

The stock ordinary EffectInit/ResetFieldData path renders stereo but rejects/silences
mono because setup precedes mono bus arrangement. Its actual binary fixture preserves
that historical gap, then proves positive mono/stereo1024 rendering after public
enter-offline/exit-offline notifications. W200's default native owner closes the public
reference's ordinary initial mono processing gap without claiming the stock wrapper
itself was repaired. Stock Uninitialize disables callbacks before SDK object retirement,
and fresh EffectInit can readopt parameters; this is a distinct parameter lifetime scope.

Actual native module fixtures capture registered descriptors/factories/algorithms,
run real SDK Initialize proxies, render ordinary first mono/stereo1024 blocks, and
exercise requested-versus-acknowledged controls, complete queue ordering, native
chunks, malformed state/IDs, silence/tails, retirement and fresh initialization.
A separately initialized matching synthetic owner through the actual registered void
algorithm proves original backend poison/status observation; it is not a fault injected
into the normal Gain provider. Normal/fast-math helper/model tests cover exact host
faults, matching native callback faults, independent smoothing/history oracle and
10000 C++ allocation-counted prepared valid/late-fault/control cycles. Existing Rust
native contracts separately establish their own allocation behavior. No arbitrary
provider/caller/host allocation or deadline guarantee follows.

The earlier normal public native and preserved stock bundle commands passed18/18
and14/14CMake contracts, respectively, plus actual package ACF/export/SDK-version
and registered algorithm fixtures. Later initialized transport acceptance passed
22/22nativeGain,4/4Performance and14/14stock with8Python tests. Prepared multibus
source adoption adds four nativeGain contracts; final26/26public nativeGain plus
actual module and Performance/stock regression receipts follow separately in
[work status](WORK_STATUS.md).
[Decision0213](decisions/0213-aax-checked-native-lifecycle.md) records the owned native
boundary; [Decision0209](decisions/0209-aax-native-schema-control.md) preserves the
stock control adoption. See [VST3](vst3.md) and [decision0023](decisions/0023-native-format-references.md).

Loading these local fixtures does not establish Pro Tools host acceptance,
production PACE signing, broad AAX/platform support or a real-time deadline.
Pro Tools developer host access or authorized production signing remains separate.
Installed wraptool is insufficient. No credentials/configuration are read, PACE
production signing invoked, artifacts published or host files installed. Avid SDK
terms and the owner's agreement require review before release; development notices
are not a redistribution licence grant.


## Generated short-MIDI output

Explicit output-enabled native instrument models use fixed-capacity staging and
registered output nodes; default references retain output capacity zero. See
[generated MIDI output](aax-generated-midi.md) for the representable subset, complete
preflight, consumed readiness, exact partial SDK delivery and actual initialized
Rust NoteRouting consumer. These receipts do not establish Pro Tools/PACE or a
packaged output-enabled plug-in.
