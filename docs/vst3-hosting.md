# Explicit trusted VST3 catalogues

The default-disabled `ravet-host/vst3-catalogue` feature supplies a Rust C ABI
and main-thread owner for a caller-linked native provider. The optional C++17
provider uses an external caller-owned VST3 SDK. Cargo does not locate/download
that SDK or link a native provider. This stage loads one explicitly trusted module
and caches its complete factory/class metadata. Component/controller instances and
audio are supplied by the separate [scoped instance API](vst3-instances.md).
Stopped component/controller bytes use the separate [state companion](vst3-state.md).
Owned controller parameters and prepared timed input use the separate
[parameter companion](vst3-parameters.md).
Main-thread NSView editors use the separate [editor companion](#hosted-editors-macos-nsview). Editors for Graph-bank children and controller edit gestures are described in [Graph-bank child editors and edit gestures](#graph-bank-child-editors-and-edit-gestures).

`Provider::from_raw` is unsafe: the supplied table and executable implementation
must remain loaded and implement the documented pointer, ownership and thread
contracts. Its lifetime does not pin a separately loaded dynamic library. Unsafe
`Provider::load` explicitly trusts the selected module and dependencies as
compliant native code. Native initialization has unrestricted process access;
crashes, termination, arbitrary foreign allocation and reentry are not isolated.
The provider requires the platform main thread, with same-owner-thread checks on
queries and retirement. Rust `Provider` and `Library` cannot move/share across
threads. Preparation, load and retirement belong outside audio callbacks.

A successful load publishes 1..256 unique nonzero opaque 16-byte class IDs,
positive cardinalities and bounded complete metadata. Factory vendor and class
category/name are required; extended vendor can fall back to the factory vendor
with explicit provenance. Highest available factory interface chooses Unicode,
extended narrow or base metadata. A failed extended getter rejects the whole
catalogue. Fixed native strings require termination and valid scalar UTF8 or
paired UTF16; conversion never clips. Raw subcategories, SDK version, flags and
optional fields are preserved. Cached Rust getters execute no plug-in calls.
Path input is 1..4096 UTF8 bytes without NUL; no system scan occurs.

The native host application outlives factory references and module termination.
A live factory identity has one owner, including path aliases: a second open
rejects before context mutation. Up to256 identities are reserved. Its bookkeeping
mutex is never held across foreign calls. Factory3 context installation and
clearing retain exact native results. The macOS SDK returns NotImplemented after
calling its registered callbacks; this optional result is accepted and reported,
without a successful delivery claim. Other context installation failures reject.
Failed candidate cleanup clears best-effort; it cannot roll back foreign effects.
A pointer-valid interface returned with an unsuccessful query is retained for
release before its status is checked. Caught native C++ exceptions map to SDK
InternalError; exceptional clearing still consumes once.

`Library::retire` reports consuming successful or failed-native context clearing.
Both retire the handle once. Other provider rejection returns the original owner
in `RetireFailure` for explicit recovery. Drop retires once and discards the
result; it does not retry. A noncompliant native provider is outside the unsafe
contract. The independent ABI-v2 [instance owner](vst3-instances.md) establishes explicit
non-discardable retention before creation; the v1 catalogue supplies no instance API.

Build the optional native provider from an ordinary local checkout:

```sh
cmake -S crates/ravet-host/native/vst3 -B target/vst3-host \
  -DRAVET_VST3_SDK=/absolute/path/to/vst3-sdk -DCMAKE_BUILD_TYPE=Release
cmake --build target/vst3-host --target ravet-vst3-host \
  ravet-vst3-catalogue-tests ravet-vst3-context-tests -j 2
ctest --test-dir target/vst3-host --output-on-failure
```

The static target exports `ravet_vst3_provider_v1` and the separate v2 instance
table, plus `ravet_vst3_state_provider_v1` and
`ravet_vst3_parameter_provider_v1`; catalogue consumers explicitly link its
SDK dependencies and frameworks and borrow the returned table as Rust
`RawProvider`. CMake copies the external complete SDK licence into its build
folder; retain that notice in linked distributions. The native recipe is currently
macOS only. The portable Rust ABI can be compiler-checked elsewhere, without
native loader/runtime support evidence.

To exercise explicitly selected local modules, configure
`RAVET_VST3_TEST_MODULES` with a quoted semicolon-separated absolute-path list,
build `ravet-vst3-provider-tests`, then run CTest. With no modules supplied the
actual module test is absent; its executable rejects an empty argument list.
Contract tests cover raw metadata boundaries/faults, Rust cache/owner/retirement
and non-Send/non-Sync declarations. Public receipts load the actual local
Gain and Performance modules repeatedly and from a Rust main-thread executable.
Public CMake integration receipts are recorded in [work status](WORK_STATUS.md).
No third-party compatibility, DAW, native editor, sandbox, device, process memory
bound or real-time deadline claim follows. An authored pointer-valid factory and
Module::create substitute additionally checks nine context optional/failure/
exception cases, exact reference retirement and host lifetime during final factory
release and module exit. This uses public SDK interfaces; actual SDK module loading
is exercised separately. All three public CTest executables and a relinked Rust
main-thread owner/provider composition probe pass locally after these fixes.

See [decision0210](decisions/0210-trusted-vst3-catalogues.md) and
[CLAP hosting](plugin-hosting.md).

## Hosted native 32/64 aux buses and owned Graph banks

The default-disabled `ravet-host/vst3-hosted-audio` feature (it enables
`vst3-catalogue`) adds `vst3::instances::hosted_audio`. It is an additive,
exact-origin companion: the caller-linked native archive exports
`ravet_vst3_hosted_audio_provider_v1`, whose 72-byte table (64-bit macOS)
contains pointers to the exact instance ABI-v2 and rich processing-v1 tables.
`hosted_audio::Provider::from_raw` checks size, version, every entry and both
origins; preparation additionally checks pointer identity against the
Instance's Library and its refreshed processing table. Existing v1/v2, state,
parameter and processing layouts, result domains and f32 entry points are
unchanged. Cargo still links no SDK; see the native build section above.

### Catalogue and explicit profile

On a stopped child, call `refresh_parameters`, then `refresh_processing`, then
`refresh_hosted_audio`. The returned `AudioCatalogue` copies every advertised
audio bus in native index order (0..16 per direction): direction, main/aux type,
`defaultActive` flag, channel count, speaker arrangement bit mask and UTF-8
name. At most one main bus per direction is admitted. Native refresh
invalidates any earlier readiness, so every refresh requires preparation again.

`HostedAudioProfile::new(precision, rate, maximum_frames, inputs, outputs)`
takes an explicit `BusRequest::active(arrangement)` or
`BusRequest::inactive(arrangement)` for EVERY advertised bus; no
`defaultActive` inference or fallback precision is applied. Rate is finite,
positive and at most 768000 Hz; frames 1..=65536; at most 64 active packed
channels per direction and at least one active channel overall. Mono is the
SDK centre speaker bit (`1 << 19`), not bit 0.

`prepare_hosted32` / `prepare_hosted64` refuse a profile of the other precision
locally. Local refusal (shape, missing bus request, stale catalogue) preserves
the prior prepared profile. Once the native call begins, Rust readiness is
cleared first. The native provider negotiates `canProcessSampleSize`, passes all
advertised arrangements (inactive included) to `setBusArrangements`, reinspects
counts/types/arrangements, sets up processing, activates the requested buses
and only then publishes; the latency/tail getters follow. Native refusals keep
their original stage (`ControlReport::stage`, 100..=122) and returned tresult,
for example a `kResultFalse` from `canProcessSampleSize` at stage 107.

`PackedAudioMap` exposes each bus's native index, arrangement, channel count and
packed range. Active channels pack in increasing native index order separately
per direction; inactive buses keep their index (no range) and remain present in
native `ProcessData` with null sample pointers.

### Scoped processing

`Instance::with_audio` lends the same Send/non-Sync/non-Clone HRTB capability.
`Audio::process_hosted::<f32 | f64>` takes packed input/output slices, the
unchanged W233 `Input` (points, events, optional context) and an exclusively
borrowed `OutputStorage`; the returned `Output` borrows that storage. Precision,
counts, frames, points, events and context are checked before native calls.
f64 samples are never narrowed. Legacy main-f32 entries refuse a hosted
capability before any native call. A native fault silences valid output prefixes
and retires hosted processing for that scope; stopped re-preparation or a fresh
child is required.

### Owned Graph bank

`hosted_audio::graph::Vst3GraphBankBuilder<S>` consumes a MAIN
`Library<'static>` (static caller-linked tables and executable) and the exact
paired parameter/processing/hosted providers. Children retain the Library
through a private `Rc`; no borrowed lifetime is erased. `node` /
`node_with_setup` add up to 128 children; setup runs on MAIN after creation and
before refresh/preparation (for example stopped state recall). `build` prepares
every child and captures latency/tail FIRST, then starts every child and begins
every native AUDIO lease LAST. Any start/begin failure ends earlier leases in
reverse before stopping and retiring; no node escapes.

`Vst3GraphNode<S>` implements `Processor<S>` with the packed layout, cached
latency/tail and no foreign lifecycle calls: prepare admits only the exact rate,
layout and a block no larger than the prepared one. It is Send, not Sync, and
owns only the native AUDIO handle, static tables, copied profile and prepared
buffers. Sidechain/aux buses are ordinary packed node channels, so Graph's
main-only outer configuration is unchanged.

Dropping a node moves its whole bundle into a dedicated one-slot return ring
whose consumer always stays in the process-lifetime MAIN registry (64 banks,
128 children, nonwrapping generations; bound to the platform main thread on
Apple targets). `Vst3GraphBankControl` is MAIN-only and disposable: dropping it
marks the bank orphaned while the module, children and leases stay retained.
`reclaim_vst3_graph_banks(Some(id) | None)` gathers returned bundles without
native calls, then for an orphaned fully returned bank ends ALL leases before any
stop or retirement, then retires children and the Library. A forgotten node,
never-joined worker, refused return slot or failed native end keeps the bank
retained with a diagnostic; nothing is guessed to have ended.

#### Rich lanes

`LaneOptions` (1..=4 complete owned batches per node, processing ABI limits, a
64 MiB per-bank aggregate budget) selects queued mode. On MAIN,
`control.submit(child, frames, fill)` fills typed `InputEvents`, `InputPoints`
and `Context` and assigns the next exact sequence; the node consumes exactly one
matching head batch per block and never skips or overwrites one. A missing
batch, sequence/owner or frame mismatch refuses before native mutation and faults
the node with a copied diagnostic. `control.receive` re-validates the batch's
complete rich output on MAIN and recycles it. No Vec/String/Box is constructed or
dropped on AUDIO. Framework events reach a node only through the explicit
translation below; a node without one refuses non-empty framework events
before native execution.

#### Framework event translation

`builder.translate(child, FrameworkTranslation)` opts one child in to
translating ordinary RAVET events and block-start transport
(`hosted_audio::graph::framework`, [decision 0273](decisions/0273-hosted-vst3-graph-framework-events.md)).
Shape and capacity are checked when you call `translate`. `build` checks
parameter routes and the note bus against the refreshed catalogue after every
child is prepared and before any child starts or any lease begins. A refusal
is `BankError::Translation { child, error }` and unwinds like any build error.
Each `TranslationError` names the failing case and route index (duplicate
graph or native ID, reserved native ID, invalid scale, note bus, capacity,
unknown or read-only parameter). Routes are checked against a catalogue
snapshot taken at `build`; a later plug-in restart that changes parameter
flags is not revalidated.

| Framework input | VST3 input | Rule |
|---|---|---|
| `ParameterValue` | `IParameterChanges` point | Declared `ParameterRoute` (graph ID, native ID, `Normalized` or declared `Linear { minimum, maximum }`) |
| `NoteOn` / `NoteOff` | `NoteOnEvent` / `NoteOffEvent` | `noteId -1` under `Unidentified`; refused under `RequireIdentified` |
| `NoteWithId` | `NoteOnEvent` / `NoteOffEvent` | ID kept, 0..=`i32::MAX` |
| `NoteExpression` (not pressure) | `NoteExpressionValueEvent` | `expressions` declared; ID-only target; volume `/4`, tuning `/240 + 0.5`, others unchanged |
| `NoteExpression::Pressure` | `PolyPressureEvent` | `poly_pressure` declared; concrete channel and key, optional ID (else -1) |
| `Transport` | `ProcessContext` | `transport` declared; flags exactly for the fields present |

- **Routes.** No ID equality is inferred. Graph IDs and native IDs are unique,
  with at most 2048 routes. A route must name a writable parameter that is not
  hidden. Values outside the declared scale refuse. The plug-in still owns
  stepped quantization and any nonlinear mapping; declare a `Normalized` route
  for those.
- **Points.** Each value becomes one point at its exact sample offset. Points
  are never sorted or merged.
  - A second value for the same native ID at the same offset follows
    `same_offset`. `SameOffsetPolicy::Refuse` (the default) refuses the block.
    `LastWins` replaces the earlier value and counts it as ignored.
  - The native queue admits at most 64 distinct IDs per block
    (`MAXIMUM_PARAMETER_IDS`), reported as its own refusal.
  - Admission is bounded per point. The conflict check scans only the trailing
    equal-offset run (at most 64 points), and the distinct-ID set is kept
    incrementally. A full 2048-point block costs at most about 2048 x 128
    comparisons.
- **Notes.** The note bus must be selected in `event_inputs`. Channels must be
  within its channel count. Velocity is copied unchanged. Tuning, length,
  `ppqPosition` and flags are zero; nothing is derived from transport. The
  translator is stateless: a note-off without an earlier note-on is passed
  through, and the plug-in owns voice state. Once expressions or pressure are
  declared, a malformed target or value refuses even under `Ignore`; only
  undeclared capabilities are ignored.
- **Transport.** All-absent transport passes no context. Otherwise `playing`
  and `sample_position` must both be known, because the SDK has no availability
  flag for either. The sample position is copied exactly. Present tempo, beats,
  recording, active loop, signature, bar start and beat loop set `kTempoValid`,
  `kProjectTimeMusicValid`, `kRecording`, `kCycleActive`, `kTimeSigValid`,
  `kBarPositionValid` and `kCycleValid`. Discontinuity, pre-roll, tempo
  increment, bar number and sample loops are validated but not represented.
  `framework::transport_context` exposes this mapping. With `transport` false,
  transport is not passed and is not counted as ignored.
- **Policies.** `unmapped_parameters` and `unsupported` are each `Refuse` or
  `Ignore`. `unsupported` covers MIDI, UMP, modulation, gestures, scoped values
  and release, choke, completion, notes without a route and undeclared
  expressions. Ignored events are counted: read them on MAIN with
  `control.ignored(child)`.
- **Mixed blocks.** In queued mode, `MergePolicy::Refuse` refuses a block where
  both the framework and the lane batch carry events or points.
  `FrameworkFirst` and `LaneFirst` merge by offset, with that source first at
  equal offsets; cross-source point conflicts refuse. A context from both
  sources always refuses. A context from only one source is used.
- **Capacity.** `events` and `points` are each 0..=2048 per block. Merged
  storage, needed only by the merging policies, must also fit 2048 alongside
  the lane batch. All of it is allocated on MAIN and counted in the 64 MiB
  per-bank budget. AUDIO translation allocates, locks and frees nothing.

Every refusal, including capacity overflow, happens before the plug-in runs.
The plug-in is not called and its history does not advance.

- **Outputs.** The block's outputs are silenced and the node returns
  `ProcessStatus::Silence`, not a fault.
- **No latch.** Neither the node nor an enclosing `ravet_graph::Graph`
  latches. The Graph reports `GraphStatus::Continue` and the next valid block
  runs.
- **Reason.** The reason is a `framework::Refusal`, with a distinct variant for
  each case: invalid event, unmapped parameter, unsupported kind, same offset,
  event or point capacity, the 64-ID limit, transport, mixed input, two
  contexts, merge conflict and merged capacity.
- **Reporting.**
  - `control.refused(child)` counts refused blocks.
  - `control.drain_refusals(child, f)` passes up to 16 unread records to `f`,
    oldest first. Further refusals are counted but their details are dropped.
  - `diagnostic::FRAMEWORK_REFUSED` or `diagnostic::MIXED_REFUSED` is set.
  - In queued mode, the matching lane batch is consumed and `receive` reports
    it as `Error::Framework(refusal)`, so sequences stay aligned.

A Graph status does not carry the refusal. Callers that must react to
refusals poll these on MAIN. Native faults, admission refusals and framework
events sent to a node without a translation still fault and latch, and need a
rebuild. Decision 0273 explains why refusals return `Silence`.

Tests check every mapping and refusal with the mock provider, including exact
sample offsets and 10,000-cycle zero-allocation runs. An actual caller-linked
consumer loads RAVET Gain and Performance VST3 bundles built from the same tree.
A Gain node driven by framework values 1.0 at sample 0 and 3.0 at sample 11
(declared linear 0..4 route) matches the smoothing reference exactly, changing
first at sample 11. A Performance node driven by `NoteWithId`, volume and pan
expressions, poly pressure, Level values and transport matches a closed-form
voice (maximum error about 1.1e-10 in f32 and 7.5e-18 in f64). A broadcast
Graph chain, Performance into Gain at 0.5, matches the same reference and
reports the expected ignored counts. A refusing Gain node inside a Graph
silences only the refused block. The Graph reports `Continue` for every block,
output equals input before and after the refusal, and a later gain change is
applied. The mock provider checks the same over seven Graph blocks with two
refusals. 10,000 variable Graph blocks with
translated events perform zero Rust allocations on AUDIO. The reference
Performance voice does not apply pressure or transport, so those reach the
actual modules without an audible check; the mock provider checks their wire
form.

#### Checked reset

Generic hosted VST3 has no guaranteed SDK history reset. Node `try_reset` returns
`ProcessError::RebuildRequired` without foreign calls and the node stays
faulted; legacy `reset` records the same state. `Graph::try_reset` reports the
original node and error and remains faulted. Rebuild: stop/join workers, drop
the nodes, reclaim on MAIN, and build a fresh bank.

### Limits

Explicit unsafe table/code/module trust remains the caller's obligation. Rust
allocation counting covers Rust only: foreign SDK/C++ heap, crash isolation and
real-time deadlines are not established. Evidence is local macOS arm64 with the
authored Gain/Performance references and authored native fixtures; no DAW,
third-party plug-in or other-platform support follows. Auxiliary buses and all nine rich input
event kinds also have actual-module evidence against the RAVET reference VST3
modules; see [vst3-reference-modules.md](vst3-reference-modules.md). Framework event translation
is described above. Editors for scoped instances and for Graph-bank children,
and controller edit gestures, are described below.

## Hosted editors (macOS NSView)

The default-disabled `ravet-host/vst3-editor` feature (it enables
`vst3-catalogue`) adds `vst3::instances::editor`. The caller-linked native
archive exports `ravet_vst3_editor_provider_v1`, whose 88-byte table names the
exact instance ABI-v2 table as its origin. `editor::Provider::from_raw` checks
the size, the version, all nine entries and a non-null origin.
`Instance::open_editor` also checks pointer identity with the instance's own
table before any native call. Statuses 16..19 (unavailable, unsupported,
reentry, empty) belong to this companion only; existing layouts and domains are
unchanged.

### Lifecycle

`open_editor(provider, Platform::NsView)` runs on the platform main thread. It
calls `createView("editor")` and requires `isPlatformTypeSupported("NSView")`.
It then installs a host `IPlugFrame`, queries `canResize`, and admits a
`getSize` of 1..=16384 per dimension. A missing view is `Error::Unavailable`
(stage 201) and another platform is `Error::Unsupported` (stage 202). A creation
failure clears the frame and releases the view before returning.

Each child holds at most one editor. The guard exclusively borrows the
`Instance`, which borrows its `Library`, and exposes only `&Instance`. Native
child destruction returns `LIFECYCLE` while an editor lives, and module close
returns `CHILDREN`. A view released through `close` or drop is therefore
released before handler clearing, controller and component `terminate`, and
module exit.

Because the guard holds `&mut Instance`, the scoped Rust calls that also need it
(`with_audio`, `prepare`, `start`, `stop`, state and parameter refresh) require
the editor to be closed first. An open editor with running audio on the same
instance is demonstrated only through the raw native tables. This is an
accepted limitation (decision 0271). For an editor that stays open while
audio runs, use a Graph-bank child editor; see
[Graph-bank child editors and edit gestures](#graph-bank-child-editors-and-edit-gestures).

`mem::forget` on the guard leaves the native editor alive. Dropping the
`Instance` then leaks the native child, because the refused destroy is ignored,
and dropping the `Library` leaks the module. This leak is memory-safe and never
an out-of-order release. A drop-time destroy refused before teardown starts
keeps the native editor alive and increments `editor::refused_drops()`.

`unsafe Editor::attach(parent)` borrows a caller-owned `NSView`. The view must
stay alive until a successful `detach` or until the guard is closed or dropped.
The plug-in may add subviews. A returned `attached` refusal leaves the editor
detached. A throw is treated as attached and faulted, so teardown still calls
`removed`. A returned `removed` refusal keeps the editor attached and
retryable. Every owner reply, refusals included, reports the authoritative
attached state, and `Editor::attached()` follows it. A refused second attach or
an operation after a fault never reads as detached. `close` and drop run
`removed` (if attached), then `setFrame(nullptr)`, then release. Thread and
reentry refusals happen before teardown starts and return the guard in
`CloseFailure`. Once teardown starts it always consumes the editor and reports
the first foreign failure as `Closed::FailedNative`.

### Sizing, frame requests and scale

`resize(size)` calls `canResize` (anything other than `kResultTrue` is
`Unsupported`), then `checkSizeConstraint`, then `onSize`. Only `kResultTrue`
from `checkSizeConstraint`, which equals `kResultOk`, accepts the adjusted
rectangle, and that rectangle must be bounded. `kResultFalse`,
`kNotImplemented` and any other result are returned refusals with no
unconstrained fallback, so a resizable view without constraint support cannot be
host-resized. After `onSize` the size is re-queried with `getSize` and that
value is recorded. A differing or failed re-query is counted in
`size_mismatches`.

The native frame handles `resizeView` on the main thread:

- During host `attached` or `setContentScaleFactor` the request is acknowledged
  and honoured later. It is applied once after the call if the view is
  attached, otherwise at the next successful attach.
- A superseded request, a request refused by the plug-in's own `onSize`, and a
  request lost to a fault or teardown are counted in `deferred_discarded`.
- During any other host call into the view, or a frame-initiated `onSize`, the
  request is refused as reentry and counted.
- When idle and attached, `onSize` runs in the same call stack. Idle requests
  to a detached view are refused.
- Off-main calls, a foreign view, an out-of-range rect, calls after teardown
  and calls to a faulted owner are refused.

Applied plug-in sizes go to a one-slot mailbox. Read it with
`take_resize_request` and resize your own container; the native owner never
resizes the caller's view. Detach, teardown and a successful host resize clear
the mailbox. `state()` copies flags and saturating frame counters.
`set_content_scale(0.25..=8.0)` uses the optional
`IPlugViewContentScaleSupport` and is normally `Unsupported` on macOS.

### Reentry and threads

Editor entries hold the module's busy flag, as does an idle frame-driven
`onSize`. Nested editor entries from view callbacks return `REENTRY`, and nested
instance CONTROL entries return `LIFECYCLE`. The raw native editor entries do
not take the live-core CONTROL admission, so an AUDIO capability can stay
active. The Rust guard cannot be combined with Rust-side audio on the same
instance; see the lifecycle section above.
`Provider` and `Editor` are neither Send nor Sync. No callback enters Rust, and
no view, frame or controller pointer is exposed.

### Native validation

Configure as above, optionally with
`-DRAVET_VST3_EDITOR_TEST_MODULES="editor:/abs/Gain.vst3;none:/abs/Performance.vst3"`.
Then run CTest. The registered tests are:

- `ravet-vst3-editor-abi-tests` (C11 layout)
- `ravet-vst3-editor-owner-{normal,fast}-tests` (injected SDK view: faults,
  deferred and refused frame calls, constraint policy, retained frames, ordered
  teardown, 1000 cycles)
- `ravet-vst3-editor-provider-{normal,fast}-tests` (substituted loader: origin,
  argument, alias and thread refusals, ordering, reentry, active AUDIO)
- `ravet-vst3-editor-window-tests` (actual modules in a hidden offscreen
  `NSWindow`, three cycles each, one with audio on a worker)

`ravet-vst3-editor-visible-probe` is a manual-only, `EXCLUDE_FROM_ALL` target
that is never registered with CTest.

### Limits

Only macOS `NSView` is supported, with local arm64 evidence. That evidence is
authored fixtures plus the RAVET Gain editor built with `RAVET_GAIN_EDITOR=ON`;
Performance and default Gain have no editor and report unavailable. There is no
DAW, third-party editor, visual-rendering or accessibility claim. Keyboard,
wheel and focus forwarding are not provided. Edit gestures are adopted only
for Graph-bank children that enable them; every other child's handler still
returns `kNotImplemented`. See
[decision 0271](decisions/0271-hosted-vst3-editor-ownership.md).

## Graph-bank child editors and edit gestures

Two additions let a Graph-bank child keep processing on its AUDIO worker while
its editor is open on MAIN, and deliver the editor's parameter edits to the
processor and the host ([decision 0276](decisions/0276-graph-bank-editors-and-edit-gestures.md)).
Both are macOS-only prototypes with local arm64 evidence.

### Edit gestures

The caller-linked native archive exports `ravet_vst3_edit_gesture_provider_v1`,
an additive 40-byte table whose origin is the exact instance ABI-v2 table.
`edits::Provider::from_raw` checks its size, version, entries and origin.
`Vst3GraphBankBuilder::edit_gestures(child, provider, EditOptions)` checks
pointer identity with the bank's Library, an event-queue capacity of `1..=4096`
and a value capacity of `1..=256`. Existing tables, layouts and result domains
are unchanged. A child that does not opt in keeps the original component
handler: `beginEdit`, `performEdit` and `endEdit` return `kNotImplemented`, and
`restartComponent` coalesces flags as before.

`build` enables capture after the child is prepared and before any child starts
or any lease begins. Enabling snapshots the writable parameter ids (not
read-only, not hidden) of the catalogue refreshed at `build`. An enable
refusal is `BankError::Edit` and unwinds like any build failure. From then on:

- **Capture.** On the platform main thread the handler appends `beginEdit`,
  `performEdit` and `endEdit` calls to a fixed ring of 256 records with a
  gap-free sequence.
  - An id outside the snapshot, or a value that is not finite or outside
    `0..=1`, returns `kInvalidArgument`.
  - A full ring returns `kResultFalse`; nothing accepted is overwritten.
  - Off-main calls and calls after the child retires return `kResultFalse`.
  - Every refusal is counted.
  - The handler calls no foreign or Rust code, so edits issued from inside
    view, parameter or teardown callbacks are safe.
- **Gesture pairing.** At most 64 ids may be open (begun, not ended) at once,
  and each open gesture reserves one ring slot.
  - The `endEdit` of an accepted `beginEdit` is never refused for capacity,
    even when the ring fills mid-gesture.
  - A `beginEdit` needs two free slots (itself and its end).
  - A second `beginEdit` of an open id, and an `endEdit` without an open
    `beginEdit`, return `kResultFalse` and are counted as `unpaired`. They are
    never recorded.
  - Published begins and ends are balanced per id, except in these cases:
    - Closing or dropping the editor with gestures open records a synthesised
      `End` for each (`EditEvent::synthesized`); a later `endEdit` for that id
      is refused as unpaired.
    - Gestures still open when the child retires are not closed.
    - A panic in a `drain_edits` callback, or a pump `Abi` error, can drop a
      begin or an end from the published stream.
  - While a gesture stays open, the plug-in's next `endEdit` for that id
    closes it. At most 64 can be open, so the cost is bounded at 64 reserved
    slots.
  - Id `0xFFFFFFFF` (`kNoParamId`) is refused as unknown and never queued.
- **`restartComponent`.** Only `kParamValuesChanged`, `kLatencyChanged` and
  `kParamTitlesChanged` are accepted, in any combination. A call carrying any
  other defined flag is refused whole with `kResultFalse`; zero or undefined
  bits return `kInvalidArgument`. Accepted and refused restarts are both
  recorded.
  - `kResultOk` means "recorded for the host", not "applied": the bank does not
    reconfigure a running child.
  - An accepted latency change is published with `rebuild_required`, and the
    node keeps the latency cached at `build` (and reported for delay
    compensation) until the bank is rebuilt.
  - Value and title changes are published only. Translation routes and the
    writable-id snapshot are not refreshed, so ids the plug-in adds later stay
    `kInvalidArgument` until a rebuild.
- **Pump.** `control.pump_edits(child)` on MAIN moves records out of the
  native ring only while both the AUDIO value ring (`EditOptions::values`) and
  the host event queue (`EditOptions::events`) have room.
  - No record is taken from the native ring unless it can be both applied
    and published.
  - When the host stops draining, records wait natively, and once the native
    ring is full the plug-in's next edit is refused and counted.
  - Call `pump_edits` regularly; values reach AUDIO only through it. It makes
    no plug-in call (only RAVET's native drain entry).
  - Once the child's node has returned to MAIN, `pump_edits` refuses with
    `edits::Error::Abandoned` and moves nothing.
  - An `Abi` error (a malformed record from the trusted provider) has already
    consumed that native batch; treat it as fatal for the child's edits.
- **AUDIO delivery.** Each `performEdit` value becomes one parameter point at
  sample offset 0 of the next block that the node successfully dispatches to
  the plug-in. It goes into the node's existing prepared parameter queue, ahead
  of that block's framework or lane points.
  - Values for the same id that arrive before that block coalesce; the last
    one wins and the rest are counted as `coalesced`. They would all land on
    the same sample.
  - **The edit wins at offset 0.** If the block itself carries a point for the
    same id at offset 0, the edit value replaces it, and the replaced block
    point is counted as `overridden`. The block's later points for that id
    still apply. With automation that writes the same id at offset 0 in every
    block, every edit is still delivered, and the automation resumes from the
    next block. This intentionally differs from the framework
    `SameOffsetPolicy` and `MergePolicy`: edits are merged after them and
    independently of their settings.
  - Values beyond the native 64-distinct-id or 2048-point limit are deferred
    to the next block (`deferred_blocks`).
  - Values leave the pending set only when the native dispatch succeeds. A
    refused (silent) block or a failed dispatch keeps them pending; a failed
    dispatch also faults the node, which then needs a rebuild.
  - The pending set holds at most 256 distinct ids. Coalescing is a linear
    scan, so the worst case is `values x values` id comparisons per block
    (65,536 at the maximum).
  - Nothing is allocated, locked or freed on AUDIO.
- **Publication.** `control.drain_edits(child, f)` passes each pumped
  `EditEvent` (sequence, `EditKind::{Begin, Perform, End, Restart}`) to `f` in
  capture order.
  - When the child also has a framework translation, an edit of a routed id
    carries the matching framework event: `ParameterGesture` begin or end, or a
    physical `ParameterValue` through the route's declared scale.
  - That event is for the host's own model (automation recording, UI mirrors).
    Do not feed it back into the same node: the value was already applied on
    AUDIO.
  - Events leave the queue before `f` runs; a panic in `f` loses the rest of
    that batch.
- **State.** `control.edit_state(child)` copies the native counters (pending,
  open, accepted, overflowed, invalid, unknown, unpaired, off-thread, retired,
  restarts) and the node counters (pumped, applied, values pending, coalesced,
  overridden, deferred blocks).

### Graph-bank child editors

With `vst3-editor` and `vst3-hosted-audio`,
`control.open_editor(child, editor::Provider, Platform::NsView)` returns a
`Vst3GraphEditor`. It uses the same native editor companion as scoped
editors. Those entries take the module busy flag but not the live-core CONTROL
admission, so the bank's AUDIO leases stay live. The guard borrows neither the
control nor an `Instance`, and is neither `Send` nor `Sync`. It offers the
scoped editor's operations: unsafe `attach`, `detach`, `size`, `resize`,
`set_content_scale`, `take_resize_request`, `state` and `close`.

**Thread safety is the plug-in's responsibility.** With an open editor, MAIN
calls into the controller and view (`createView`, `attached`, `onSize`, edit
callbacks) while AUDIO calls `process()` on the same child. VST3 permits this
and requires the plug-in to make it safe. This matters most for a
single-component plug-in, whose controller and processor are one object.
RAVET's ordering rules prevent lifecycle races (no retirement while the view
lives), but they cannot make a plug-in's own state thread-safe. Only RAVET's
Gain, a split component/controller plug-in, has been exercised. No
single-component or third-party plug-in has been tested.

Ordering is enforced by state, not by a borrow:

- Each child has at most one editor (`BankError::EditorOpen`).
- Native child destruction returns `LIFECYCLE` while the view lives.
- **Reclamation policy: defer.** While any editor of a bank is open,
  `reclaim_vst3_graph_banks` and `control.retire()` do not release that bank.
  - They end no lease, stop or retire no child, and count the bank in
    `ReclaimReport::editors`. The count is made only once every node has
    returned; until then the bank is retained as outstanding.
  - Reclamation never closes a caller's editor itself. Close or drop the guard
    on MAIN, then reclaim again.
  - The view is therefore always released before the child's controller,
    component and module retire.
- A drop-time destroy refused before teardown starts keeps the native editor
  and the bank retained, and increments `editor::refused_drops()`.
  `mem::forget` on the guard keeps the bank retained for the process lifetime.
  Both are memory-safe leaks of bounded registry capacity.

`attach` keeps the scoped editor's unsafe obligation: the caller's `NSView`
must stay alive until a successful `detach` or until the guard is closed or
dropped. No callback enters Rust, and no view, frame or controller pointer
escapes.

### Validation

- `ravet-vst3-edit-gesture-abi-tests` (C11 layout).
- `ravet-vst3-edit-gesture-provider-{normal,fast}-tests` (substituted loader).
  These cover:
  - the unchanged handler without enable;
  - argument, alias and thread refusals;
  - ordered capture with id, value and thread validation;
  - the restart flag policy;
  - ring overflow that refuses, with contiguous sequences over 1000 cycles;
  - begin/end pairing with a full ring mid-gesture, duplicate and orphan
    refusal, the 64-gesture limit, synthesised ends at editor close and
    `kNoParamId` refusal;
  - gestures issued from inside view and parameter callbacks;
  - drain while an AUDIO capability is live;
  - refusal after handler retirement.
- Rust mock tests (`hosted_audio::graph::edit_tests`). These cover:
  - delivery at offset 0 of the first dispatched block after the pump, and
    not before;
  - coalescing, the edit winning at offset 0, and framework identity;
  - 200 blocks of continuous offset-0 automation with every edit delivered;
  - deferral beyond 64 ids and across refused blocks;
  - values kept pending across a failed dispatch, on the plain and lane
    paths;
  - a block with 63 distinct ids plus new edit ids including id 0, where one
    edit applies and the other is deferred (the 64-id limit holds);
  - `pump_edits` refused after the node returns;
  - backpressure that never drops;
  - restart records and malformed records;
  - declaration and enable refusals;
  - 10,000 blocks with edits and zero Rust allocations on AUDIO;
  - bank editors with deferred reclamation, refused close and drop, and
    ordered teardown.
- `ravet-vst3-editor-bank-window-tests`: configure with
  `-DRAVET_VST3_EDITOR_BANK_LIBRARY=/abs/libvst3_editor_bank.a` and
  `-DRAVET_VST3_EDITOR_BANK_MODULE=/abs/ravet_gain_vst3.vst3`. The library is
  the `ravet-host` example built with
  `cargo build --release -p ravet-host --features vst3-hosted-audio,vst3-editor --example vst3-editor-bank`,
  and the module is a Gain built with `RAVET_GAIN_EDITOR=ON`. The test runs
  the actual RAVET Gain editor, attached to a bank child in a hidden offscreen
  `NSWindow` that is never ordered front, while two Gain children process on
  an AUDIO worker. It checks:
  - programmatic edits injected into the Gain editor's own script-message
    bridge, so the plug-in's controller issues the gestures itself;
  - that an injected but unpumped edit does not change the audio;
  - that the first block after `pump_edits` equals, bit for bit, a reference
    child given the same value as a framework event at offset 0;
  - five more edits while the worker processes continuously;
  - deferred reclamation while the editor is open;
  - clean teardown, three times;
  - every AUDIO `process` call measured with `allocation-counter` (Rust heap
    only).
- ThreadSanitizer (local, normal build only). The native edit-gesture,
  editor and hosted-audio provider fixtures, and the existing editor window
  test with the actual Gain editor and concurrent AUDIO, were rebuilt with
  `-fsanitize=thread` and reported no warnings. Only RAVET's own C++ is
  instrumented; the Gain plug-in, the VST3 SDK hosting library and Rust code
  are not.
  - The Graph-bank editor window test could not be linked under
    ThreadSanitizer: ld64 refuses a fourth personality routine (C++,
    Objective-C and Rust plus the sanitizer runtime's). It has no sanitizer
    evidence.
  - This is not a proof that no race exists.

Not covered: single-component or third-party plug-ins, a visible focused
editor, WebKit JavaScript or input-event edits, multi-bank load, and Rust-side
race detection (no sanitizer or model checker for the Rust rings).

### Limits

macOS `NSView` only, local arm64 evidence. The only real editor exercised is
RAVET's own Gain editor, in a hidden window that is never ordered front. Its
edits are injected in-process into its script-message handler, not produced
by WebKit JavaScript evaluation, mouse or keyboard input. There is no DAW,
third-party editor, visual-rendering, accessibility or real-time deadline
claim. Allocation evidence covers the Rust heap only. Edit values are applied
at block granularity (offset 0). The bank does not restart or re-prepare a
running child for latency, I/O or routing changes. A forgotten editor guard
keeps its bank retained.
