# VST3 gain reference

The public reference now builds a VST3 processor and separate edit controller
using an explicitly supplied external Steinberg SDK. Its DSP and state codec
are the existing Rust `ravet-gain` implementation, reached through the narrow
`ravet-gain-native` ABI. This public wrapper is a mono/stereo native f32/f64 gain/bypass
prototype, not a completed generic format framework.

## Build and check

Install CMake and a native C++ toolchain. On macOS use Xcode. Supply a complete
SDK installation explicitly; it is not downloaded or vendored automatically.
The exercised SDK is 3.8.1; other versions are not established as compatible.

```sh
CARGO_BUILD_JOBS=2 python3 scripts/bundle-vst3.py --sdk /path/to/vst3sdk
# Optional macOS embedded editor and actual-bundle editor probe:
CARGO_BUILD_JOBS=2 python3 scripts/bundle-vst3.py --sdk /path/to/vst3sdk --native-editor
# Optional macOS factory list/selector (can combine with --native-editor):
CARGO_BUILD_JOBS=2 python3 scripts/bundle-vst3.py --sdk /path/to/vst3sdk --factory-programs
# Separate public performance instrument (generic host UI):
CARGO_BUILD_JOBS=2 python3 scripts/bundle-vst3.py --sdk /path/to/vst3sdk --plugin performance
# A debug build is also available:
CARGO_BUILD_JOBS=2 python3 scripts/bundle-vst3.py --sdk /path/to/vst3sdk --profile debug
```

The script builds the Rust static library and CMake plug-in/validator/contracts
with two build jobs. It runs CTest and Steinberg's validator, includes the supplied
SDK licence and RAVET GPL notice, and ad hoc signs the macOS bundle. Artifacts
stay under `target/vst3`; SDK link/install helpers are disabled. Nothing is
published, installed in a normal host directory or commercially signed.
Gain remains the default profile. `--plugin performance` selects the separate
native performance archive, module and instrument contracts; its isolated build
root is `target/vst3/performance/{debug,release}`. `--native-editor` and
`--factory-programs` apply to Gain only and reject with Performance before build
mutation. The selected bundle alone receives notices/signing and dynamic SDK
validation; unrelated outputs are ignored. Shared mechanics live in
`scripts/vst3_bundle.py`. See
[decision0176](decisions/0176-public-vst3-profile-bundling.md).

Only macOS arm64 has been executed; Windows/Linux build branches need native
validation and complete target-specific Rust static-link system libraries.

## Parameters, processing and state

VST3 IDs 1/2 are gain and bypass. Gain uses physical amplitude 0..4 and normalized
0..1; unity is 0.25 normalized. Bypass is stepped and advertises the host bypass
flag. Separate processor/controller classes follow the SDK architecture. The
host may provide a generic parameter editor; an optional macOS Cocoa `IPlugView`
is implemented below.

Prepare accepts matched mono/stereo, native f32/f64 and 1..65,536 maximum frames. At most
1,024 total queued points are inspected, including unknown IDs. All queue counts,
payloads, normalized values, offsets and audio pointers are validated before DSP
or pending restore mutation. Unknown parameter IDs are ignored only after
validation. Stable bounded insertion merges queues in frame order, retaining
queue traversal order at equal offsets. Cost is O(events squared); this is not
a worst-case callback deadline claim. No sort allocation is performed.

Raw global parameter queues now use the reusable native SDK walker, which copies
each point once and validates all unknown-ID payloads before any prepared visitor
effect. Both sample precisions use control-created immutable native range/role
caches for mapping, with no schema-provider calls on audio. Finite bit checks
retain validation under fast-math; normal/fast-math SDK-linked contracts exercise
malformed prefixes, unknown values, capacities, timing and cached C++ heap counts.
This delivers explicit parameter points; continuous host curve interpolation and
other SDK event kinds remain separate work. See
[prepared native parameter input](native-processing.md).

Input scratch copying completes before host output writes, supporting separate,
in-place and mixed input/output aliases. Output channel ranges must not overlap.
Zero-frame calls permit parameter flush without audio buses. Gain smoothing and
sample-timed bypass follow the portable processor. Reset preserves requested
state. Invalid blocks silence active host outputs, retaining backing tails;
unwinding DSP panic disables processing until control-thread setup is repeated.
Nonfinite audio is not sanitized. Instruments, auxiliary buses, note/event
ports and emitted events remain pending. The explicit block-start transport
profile below forwards representable fields; further context metadata remains
unmapped.

State is the shared 32-byte complete gain/bypass record; 20-byte legacy gain
records migrate with bypass off. IBStream I/O is bounded and handles short
reads/writes. Decode completes before publication. Processor state uses one
lock-free 64-bit atomic word and a 31-bit restore generation. Audio applies a
new restore only after host block validation; conditional reflection preserves
concurrent restores. Fewer than 2^31 restores may occur across pending callbacks.
The controller receives the complete component state separately, updating both
normalized controls. Controller state also stores complete current controls:
REAPER may suspend processing when host bypass is enabled, leaving subsequent
controller edits undelivered to processor queues. Restoring controller state
publishes the validated complete record through the SDK connection point;
restores before connection remain pending until connection. These UI/control
messages allocate outside processing; ordinary sample automation retains its
parameter-queue timing. No stream I/O or message allocation occurs in process.
The AU SDK identifies itself with `IVst3ToAUWrapper` and owns a separate normalized
parameter snapshot. Its controller can lag AU calls until UI timers run; the
embedded adapter retains component-based preset recall there instead of letting
a stale controller record override DSP state. AU recall still preserves current
host bypass, as its actual-bundle regression verifies.

## Native pointer boundary and evidence

`include/RavetGain.h` documents caller-owned pointer ranges and exclusive handle
ownership. Prepare/destroy belong after callbacks stop. Native foreign pointers
must be valid/aligned and cover declared capacities; null checks cannot establish
foreign memory safety. Request-result pointers cannot overlap sample buffers.
No control-thread caller may inspect a live DSP handle. Pure encode/decode run on
control threads; state publication belongs to each format wrapper.

Rust ABI tests cover aliases, zero-frame events, complete/legacy codecs, rejection
and retained tails; 10,000 iterations of processing, reset, restore and capacity
rejection report no Rust allocation/deallocation. C++ contracts check exact wet/
bypassed samples, mono/stereo, separate/in-place storage, complete state/controller
recall (including suspended controller edits), zero-frame flush and whole-stream rejection. Steinberg's validator loads
the actual bundle and passed 47 tests on macOS arm64. Native allocations, DAW
behavior, production signing and deadlines require separate evidence. REAPER
7.82 on macOS arm64 discovers the VST3 and verifies synthetic wet/bypassed
stereo audio, gain/bypass session recall, normalized automation and its generic
parameter window. This does not validate a custom editor or other hosts/platforms.
See [work status](WORK_STATUS.md) and
[decision 0021](decisions/0021-vst3-gain-native.md).

## Shared native f64 gain ABI

The public `RavetGain.h` also declares a separate `RavetGain64` handle with
`ravet_gain64_create/destroy/prepare/reset/restore/process`. Use matching lifecycle
functions for each handle precision. Native sample pointers are double; parameter
values/result gain remain float and bypass remains u32. The shared 32-byte state
codec and legacy decode are unchanged. The opaque handles cannot be cast between
precisions. Preparation bounds mono/stereo, 1..65536 frames and 1024 events; f64
copied-input/output scratch allocates at most 2 MiB (1 MiB for f32). Native
process/reset/restore use prepared storage through the reusable
[native processing adapter](native-processing.md).

Both ABI precisions expand one shared validation/aliasing/fault implementation.
All inputs copy before output writes, including cross-channel aliases; output
ranges must remain disjoint and result pointers must not overlap sample buffers.
Live/aligned pointer ranges are caller obligations. Zero frames permit null sample
pointers. Rejections silence supplied valid output ranges and preserve requested
results; unwind containment does not promise exceptional real-time behavior.

Build/run explicit C++ ABI acceptance without a format SDK:

```sh
CARGO_BUILD_JOBS=2 cargo build -p ravet-gain-native --release --locked
cmake -S tools/native-validation -B target/native-contracts -DRAVET_GAIN_LIBRARY="$PWD/target/release/libravet_gain_native.a"
cmake --build target/native-contracts --parallel 2
ctest --test-dir target/native-contracts --output-on-failure
```

Local macOS arm64 C++ acceptance passes 6006 normal/zero-frame blocks across
six mono/stereo 44.1/48/96 kHz cycles, retained sub-f32 precision, cross-channel
aliasing, backing tails and rejected-output silence. Rust ABI tests additionally
check extreme/subnormal samples, bypass/state recall, invalid preparation and
10,000 zero-heap prepared f64 workloads. This is ABI evidence. VST3 mapping is separately exercised below; AU/AAX/LV2
sample-size mapping needs its own adapter/host acceptance. See
[decision 0066](decisions/0066-native-f64-gain-abi.md).

## Native VST3 f64 processing

`canProcessSampleSize` advertises both sample sizes. Setup prepares the matching
Rust handle and restores the shared complete control record. Each process call
must match setup precision; mismatch rejects and silences supplied valid output
ranges before state mutation. Audio never passes through a f32 conversion in
the f64 path; parameters/smoothing retain their existing f32 resolution. State
restore/reflection and generation rules apply equally to both precisions.
Stopping resets both handles; setup can switch precision while preserving
requested gain/bypass state. Success reports per-channel silence from actual
output samples (negative zero is silent; nonfinite values are not).

SDK contract fixtures pass 6006 normal/zero-frame f64 blocks across six mono/
stereo/rate cycles, exact sub-f32 values, cross-channel aliases, event bypass,
malformed event/state preservation, backing tails, f32 switching and mismatched-
precision rejection. Separate native tests check both precision silence flags.
The actual dynamic Steinberg 3.8.1 validator passes 47 tests/zero failures,
including its 64-bit suite; the earlier unsupported 64-bit path is now exercised.
REAPER's selected native precision has not been observed for this update.
AU/AAX builds that compile this processor still need separately rebuilt adapter/
host validation; this does not establish their f64 paths. See
[decision 0067](decisions/0067-vst3-native-f64.md).

## Optional macOS embedded editor

Build with `--native-editor` to include the Rust-owned WebKit editor C ABI and
public gain UI codecs. The default bundle remains DSP-only. CMake's
`RAVET_GAIN_EDITOR=ON` requires the corresponding Rust static-library feature.
Only macOS/Cocoa NSView embedding is advertised; no floating window or other-OS
editor support is inferred. The host owns its AppKit/run loop/window and serialized
UI lifecycle. The view retains its controller and native callback entry retains
the view while edits/snapshots run.

The compiled local page uses reusable slider/number and bypass controls with
physical gain values. Rust validates bounded edit JSON against stable gain IDs,
clamps/snaps physical values and emits normalized host units; Rust also encodes
controller snapshots and edit status. SDK code owns begin/perform/end edits and
controller writes. Accepted edits reach the host through ordinary normalized
parameter automation, with no direct audio-thread/DSP mutation. Rejected perform
edits restore the controller value and display an error. A rejected end retains
host gesture ownership, retries during coalesced UI snapshots and rejects removal
until that end succeeds; forced destruction can only attempt another end.

Host automation refreshes unfocused controls without sending new edits. Initial
size is 480×480 logical pixels; explicit resize accepts 360×300 through 1200×900,
with size-constraint clamping. Removal destroys the native child independently of
processing; the same IPlugView can reattach with current controller values.
The page includes four input/output meters when the host forwards processor
output parameter changes. The editor option registers twelve additional hidden,
read-only, nonautomatable parameters (IDs 10..21): peak/RMS/diagnostic triples
for input L/R, output L/R. Levels normalize 0..4 and saturate for UI telemetry;
DSP audio/f64 meters retain their precision. Diagnostic values flag nonfinite
samples. The audio owner copies native meter records and emits one value per
parameter at the last sample of a successful nonempty block. Missing/rejecting
host queues drop telemetry without failing audio. Empty/rejected blocks and
stopped processing retain prior displayed values. Hosts deliver parameters
individually, so the UI does not claim an atomic complete-block meter snapshot.
These transient values are absent from RAVET's gain/bypass state record.

The controller feeds the Rust meter snapshot codec and local AccessibleMeter
controls; no live DSP pointer crosses to the UI. Accessibility attributes/text
are emitted, while actual assistive-technology use remains unvalidated.

The bundle script runs SDK processor/state contracts and the 47-test validator,
then loads the actual exported module into a synthetic Cocoa host for three
editor cycles and same-view reattachments. Its programmatic checks cover gain/
bypass edits, held multiple keys, nonfeedback host refresh, rejected value
rollback, rejected-end retry/removal retention, size bounds and actual exported
processor-to-host-to-controller meter rendering/diagnostic text. Native screenshot
inspection confirms readable controls/status. This is actual-bundle native editor
evidence; REAPER interaction, focus/assistive-tech and other-platform checks
remain unvalidated. See [decision 0077](decisions/0077-vst3-gain-editor.md).

The shared Rust gain library also exposes an immutable complete program catalogue
through `RavetGainPrograms.h` (Unity/Silence/Bypass). Portable and actual native
public-header state/audio checks pass in both precisions. The optional `--factory-programs` build now maps this catalogue to VST3
unit/list/selector metadata and complete host control recall, as described below.
The default wrapper still exposes gain/bypass controls only. See
[parameter programmes](parameters.md) and
[decision 0083](decisions/0083-native-gain-programs.md).

## Optional VST3 factory programs

`--factory-programs` explicitly enables one root unit and a three-entry program
list over Rust's complete Unity/Silence/Bypass catalogue. Stable parameter ID 3
is its normalized list selector (indices 0/1/2 map to 0/0.5/1), marked list/
program change and not automatable. Gain/bypass IDs and the persisted 32-byte
complete control record remain unchanged. Program identity is not persisted or
inferred after custom edits: the selector is the last selection request.

Controller selection validates finite normalized 0..1 and snaps nearest index
with ties upwards. It updates both local controls then requests host
`kParamValuesChanged`; the host must read/deliver complete gain/bypass values to
the processor. Without a handler only local controller values change. Rejected
notification restores all three local values; reentrant parameter/state writes
are rejected while notification runs. Host side effects are not transactional.
No private direct-controller/audio mutation is added.

Processor ID-3 queue points expand to both physical controls at the same frame,
preserving existing smoothing/bypass rules and queue tie order. The 1024 raw
point limit remains, with a separate 1024 expanded-control limit: at most 512
program points alone. Expanded overflow rejects/silences the whole block before
state/DSP mutation. Zero-frame requests apply complete controls without time.

The actual exported bundle passes one-root/list/names/selector checks, rejected/
reentrant notification handling, host read/delivery, complete state and timed
f32/f64 audio, zero-frame and 512/513-point bounds across six activations/36
blocks. SDK validator passes all 47 tests; optional Cocoa editor regression
passes three cycles. Default-build regression is checked separately. This is
macOS arm64 external SDK 3.8.1 evidence, not DAW program-menu acceptance. The
page itself still shows gain/bypass/status; AAX/CLAP/LV2 program menu mappings
remain independent work. See
[decision 0084](decisions/0084-vst3-factory-programs.md).

Optional AUv2 factory mapping is described in [the AudioUnit guide](audio-unit.md).

## Serialized native meter snapshots

The always-available reference header `RavetGainMeters.h` adds
`ravet_gain_meters`/`ravet_gain64_meters`. A caller exclusively owning the DSP
handle copies its last successful nonempty block's input/output peak, RMS,
sample count and nonfinite flag. Each reading is 32 bytes and the complete
stereo record 128 bytes, with reserved words zero. Mono's unused channel is zero.
f32 levels widen to double; f64 levels retain native precision without saturation.
Prepare/reset clear readings; empty/rejected blocks retain the prior snapshot.
The functions do not allocate/reset and reject null pointers without output
mutation. Foreign buffers must meet the header's live/aligned/distinct rules.

These are serialized DSP reads, not concurrent UI access. A host adapter must
copy readings on the processing owner and deliver them through a separate safe
handoff; merely exporting these functions does not enable VST3/AU/AAX/LV2 editor
meters. Nonfinite samples count as silence for metering diagnostics while audio
remains unsanitized. See [decision 0098](decisions/0098-native-gain-meter-snapshots.md).

## Reusable controller schema registration

The Gain controller now registers its global Gain/Bypass schema through
`ravet::vst3::registerParameters` from the optional external-SDK header
`RavetVst3Parameters.hpp`. The same helper preflights unrelated bounded schemas,
UTF-16 host text limits, stable-ID conflicts and roles, and maps control values
through native linear/log/discrete callbacks. It does not generalize Gain's
factory, processing, state exchange, editor or programme code. See
[native processing](native-processing.md) and
[decision 0148](decisions/0148-vst3-native-schema-registration.md).

The Gain processor now consumes the
[reusable native C++ client](native-processing.md#c-native-client) for f32/f64
lifecycle/audio, direct complete base-state recall/snapshot and separate telemetry.
Preparation marks state pending; the first valid callback applies the current
published restore, including zero-frame flush. Tagged global events and snapshots
retain the existing controller generation/CAS protocol, smoothing, meter IDs,
program expansion and editor delivery. Callback profile matching handles distinct
addresses for equivalent Rust-promoted tables. This adoption does not yet add
generic SDK factories/multibus/performance/generated output or replace
consumer controller codecs/scheduling. See
[decision 0157](decisions/0157-native-cpp-client.md).

The processor now requests and forwards
[explicit block-start transport](native-processing.md#vst3-block-start-transport)
through the generic owner. Supported SDK scalars validate before pending recall,
parameter visitation, DSP or state publication; malformed context silences the
supported output. Sample/musical positions, tempo, signature and transport state
retain exact available values. Inactive/positionless cycle bounds and fields that
cannot fit native v1 (including a bar start without a bar number) remain omitted.
No discontinuity, missing availability, tempo curve or DAW timeline behavior is
inferred. Normal/fast-math mapping and actual SDK composition have separate checks.

## Explicit VST3 performance input

Optional C++17 `RavetVst3PerformanceInput.hpp` consumes an external SDK event
list for one explicit bus. `nativePerformanceInput` translates one event into
one or two canonical native records; rejected/ignored results preserve the
destination and count. `visitValidatedPerformanceInput` reads each event once,
validates the complete raw list, and stages at most1024 expanded native records
before calling a visitor. Offsets are globally ordered; ties and expansion order
remain intact. Zero-frame offsets must be zero. Scratch needs56KiB plus bounded
local records; host/visitor callbacks retain their own thread obligations.

On/Off preserves f32 velocity without MIDI quantization and treats velocity-zero
On as On. SDK note ID-1 means absent; every other int32 identity, including the
SDK plug-in negative range, retains its bits in portable u32. Finite cents within
+/-12000 expand into semitone tuning after On or before Off when nonzero. Optional
note length is a hint only; no Off is inferred. SDK ppq/flags are unrepresented.
Poly pressure becomes channel/key/optional-ID scoped physical pressure. Standard
expression values are normalized0..1: volume maps to linear value*4 (including
zero mute), tuning to240*(value-.5) semitones; pan/vibrato/expression/brightness
retain f64. Expression events require a concrete ID; no voice state is inferred.

`UnknownExpressionPolicy::Reject` is default policy by explicit argument; Ignore
filters unknown kinds only after validating ID and normalized value. Other
selected types (SysEx, text, integer expressions, chord/scale, legacy MIDI output)
reject. Unselected payloads filter after common bus/timing/order checks. A visitor
refusal may retain earlier visitor effects: use private prepared scratch and
defer DSP/state/output publication until success. Gain has no performance input
bus and does not adopt this helper; a consumer must declare compatible buses and
expression metadata. No SDK instrument export, voice application, output delivery
or DAW evidence follows. See [decision0160](decisions/0160-vst3-performance-input.md).

## Explicit VST3 performance output

Optional C++17 `RavetVst3PerformanceOutput.hpp` maps canonical native records to
one explicit SDK event bus. `nativePerformanceOutput` leaves failed destinations
unchanged. Concrete On/Off and scoped Off with concrete channel/key retain exact
widened f32 velocity. ID-only standard expressions map physical volume/4 and
tuning/240+.5 to SDK normalized units; other supported scalars retain f64.
Tuning conversion may round through its floating unit arithmetic. Volume rejects
subnormal rounding/underflow using integer IEEE significand checks, including
under fast-math; tiny positive volume never silently becomes mute. Pressure
requires concrete channel/key, optional ID and exact widened f32 value.

Concrete SDK note IDs preserve int32 bits except UINT32_MAX, which would become
SDK's absent-1 sentinel and therefore rejects. Standard expressions require
ID-only scope because SDK has no channel/key fields for them. Wildcard/scalar
precision loss, note choke/end, MIDI/UMP/SysEx, parameters and text/int expressions
reject; no release is substituted for completion. SDK ppq/flags/length/tuning
metadata default to zero rather than being inferred from transport or history.

`deliverNativePerformanceOutput` preflights the entire bounded ordered native
stream before any `addEvent`. At most1024 SDK events occupy stack scratch. A host
can mutate its SDK arguments without changing native source storage. Host refusal
returns exact delivered/undelivered counts, retains rendered DSP and accepted
host prefix, and discards the suffix without replay/retry/rollback. Null host
accepts an empty stream only. Gain has no performance output and does not adopt
this helper; consumer bus declarations, metadata/voice policy and host validation
remain explicit. See [decision0161](decisions/0161-vst3-performance-output.md).

## Prepared VST3 native audio bus mapping

Optional C++17 `RavetVst3Audio.hpp::NativeAudioBusMap` validates/caches complete
native configuration on stopped control. Logical main/auxiliary buses retain
native order; zero-channel directions occupy no SDK bus, and input/output
indices compact independently. Fixed limits are1..9 nonempty buses,0..64 channels
per direction,576 total directional channels and1..65536 maximum frames. Failed
configuration retains the previous profile; consumer DSP preparation/resource
limits remain separate. Query input/output bus counts before SDK registration.

`map` validates the complete SDK audio shape at the selected native precision,
aligned sample arrays/pointers, address overflow and disjoint output prefixes
before publishing any native bus metadata. Inputs may alias outputs across any
bus. Bounded output-range sorting allocates nothing; no samples or silence flags
are changed by mapping. Caller owns live foreign metadata/arrays/samples and
keeps writable samples separate from metadata/events/private processor storage.
Flagged silence still requires valid pointers and never overrides sample data.

Zero-frame callbacks either omit both SDK directions entirely or supply exact
prepared directional counts/channel metadata; sample arrays are not read then.
Partial omitted directions reject. `silenceSupportedOutputs` clears only valid
aligned prefixes in exact prepared output shapes and supported frame
ranges at either known SDK precision (including mismatched processing precision),
skips null/misaligned/overflowing sample pointers, preserves tails and
sets bits for channels actually cleared. Unsupported metadata stays untouched;
foreign range ownership remains necessary even on rejection.

Gain now uses this profile before transport/queue staging/pending restore, and
uses bounded fault cleanup instead of directly walking rejected pointers. It
retains mono/stereo effect bus declarations; the helper's generic instrument and
auxiliary mapping is offline fixture evidence, not an exported SDK instrument or
speaker-layout/DAW claim. See [decision0162](decisions/0162-vst3-native-audio-buses.md).


## Public performance instrument

The separate [performance reference](../examples/performance-vst3/README.md)
uses the public native 16-voice sine archive and the shared external SDK build.
It declares no audio input, mono/stereo output and one 16-channel performance
input at native f32/f64 precision. The controller registers Level and five
audible standard note expressions. Host generic controls are available; this
reference declares no custom native editor or generated-event bus.

Complete canonical level recall joins the same private timed event stream as
automation and performance input. Late semantic faults leave voice history and
pending recall intact. Equal offsets apply recall, global points and performance
events in that order. The shared 1024-event capacity includes recall and optional
note-cents expansion. Overflow rejects with supported output silence. Internal
voice completion still retires voices; VST3 has no native NoteEnd equivalent, so
this consumer explicitly suppresses completion notifications.

The focused actual-archive C++ contracts and dynamic SDK validator pass locally.
A disposable REAPER 7.82/macOS arm64 fixture also passes synthetic 48 kHz MIDI
rendering, saved Level recall and automation, with independent PCM checks.
Other host/device/platform/editor/deadline claims require separate evidence. See
[decision 0166](decisions/0166-vst3-performance-instrument.md).
