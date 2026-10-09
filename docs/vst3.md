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
# A debug build is also available:
CARGO_BUILD_JOBS=2 python3 scripts/bundle-vst3.py --sdk /path/to/vst3sdk --profile debug
```

The script builds the Rust static library and CMake plug-in/validator/contracts
with two build jobs. It runs CTest and Steinberg's validator, includes the supplied
SDK licence and RAVET GPL notice, and ad hoc signs the macOS bundle. Artifacts
stay under `target/vst3`; SDK link/install helpers are disabled. Nothing is
published, installed in a normal host directory or commercially signed.
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

Input scratch copying completes before host output writes, supporting separate,
in-place and mixed input/output aliases. Output channel ranges must not overlap.
Zero-frame calls permit parameter flush without audio buses. Gain smoothing and
sample-timed bypass follow the portable processor. Reset preserves requested
state. Invalid blocks silence active host outputs, retaining backing tails;
unwinding DSP panic disables processing until control-thread setup is repeated.
Nonfinite audio is not sanitized. Instruments, auxiliary buses, note/event
ports, emitted events and full host transport mapping remain pending.

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
scratch allocates at most 1 MiB. Native process/reset/restore use prepared storage.

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
