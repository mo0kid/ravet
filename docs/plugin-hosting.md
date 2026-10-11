# Explicit CLAP discovery and trusted loading

For the separate optional external-SDK VST3 catalogue owner, see
[trusted VST3 catalogues](vst3-hosting.md). Instance/audio hosting below is CLAP.

The optional `ravet-host` crate supplies reusable explicit candidate discovery,
bounded owned plugin catalogues and loaded-entry lifetime access through pinned
Clack 0.2.0. It stays outside the portable umbrella. These are hosting mechanisms,
not a complete DAW, native plugin sandbox or third-party compatibility guarantee.

`discover_clap` scans only supplied directories: at most 32 directories, 4096
total entries and 1..1024 accepted candidates. It does not recurse, search system
paths or load code. It skips entries reported as symlinks, deduplicates canonical
paths and sorts results. macOS accepts `.clap` bundles/directories or regular
files; other targets accept regular files. Extensions match ASCII case-insensitively.
Missing/non-directory/symlink roots and I/O/capacity errors reject the complete
result. This is a candidate list; contents are validated only during selected
loading. Filesystem discovery is not an atomic snapshot or trust decision.

`ClapLibrary::load` is an explicit unsafe native-code boundary. The caller must
trust the currently selected file and its CLAP pointer/lifetime/thread compliance.
Loading/initialization and metadata inspection execute native code with full
process access. There is no timeout, crash/process isolation, signing validation
or protection from a noncompliant plugin. Do not load every discovered path just
to display a file list. `from_entry` also accepts an already-valid Clack entry,
including safely linked test plugins.

Catalogues accept 1..1024 plugins with unique nonempty IDs <=256 bytes, names and
optional vendor/version <=4096 bytes, and <=128 features of <=256 bytes. Owned
CString fields preserve native bytes; consumers explicitly choose display decoding.
Missing/oversized/duplicate metadata rejects. `entry()` exposes the valid Clack
entry for consumer-owned host callbacks, instances, audio/state/editor extensions
and lifecycle. Clack retains loaded code through instances independently of this
wrapper; dropping the catalogue does not invalidate a live instance.

```sh
cargo run -p ravet-host --example inspect --locked -- --discover target/clap/release
cargo run -p ravet-host --example inspect --locked -- --load 'target/clap/release/RAVET Gain.clap'
```

`--discover` executes no candidate. `--load` is for explicitly selected trusted
compliant libraries only. Its minimal offline example inspects the catalogue,
selects a plugin ID (optional last argument), instantiates with coalesced core
request handoff, saves/restores state through 1 MiB bounded storage when supported,
then activates, processes three f32 blocks on a separate thread and deactivates.
It accepts zero/one input bus and exactly one output bus with at most 64 channels;
no notes, output events, device or GUI is driven. It services at most one
main-thread callback at explicit pre/post-render points and reports remaining
requests while following its fixed offline lifecycle; it is not a persistent host
scheduler.
Do not infer full plugin compatibility from that example's narrow host contract.

The upstream unmodified Clack gain example at tag `v0.2`/commit
`27ca28391b57a61bad4bb5099288a7bf9247350d` was built under ignored target storage.
On local macOS arm64, discovery/load/catalogue, four-byte state roundtrip and three
stereo f32 blocks passed (0.25 output peak for 0.25 input). RAVET Gain/Sine bundles
also passed 32-/16-byte state and stereo/effect/instrument lifecycle checks. No
upstream source/binary is redistributed in this repository. Richer callback scheduling/transport/events, other formats and process isolation
remain separate work. See [decision 0046](decisions/0046-explicit-clap-hosting.md).

## Guarded native editor lifecycle

`PluginEditor` exclusively borrows a live Clack instance on its plugin/OS main
thread. `create` negotiates explicit API/floating configuration and owns successful
GUI creation. Embedded default and negotiated sizes must be 1..16384 per dimension.
A missing extension/unsupported API rejects before native creation; invalid initial
size after creation is paired with immediate destroy. Embedded `show` requires a
successful parent attachment. `resize` checks resize support and asks the plugin
to adjust before applying a bounded size. Successful show/hide calls are idempotent
and track state. Drop hides a shown GUI and always calls destroy, including scope
exit or early return. It borrows the instance, so the library remains live throughout.

`set_parent` and floating `set_transient` are explicit unsafe native operations:
the caller's compatible parent must remain alive through guard destruction, on
the correct OS UI thread. A Rust Window handle alone does not prove native-object
lifetime. Parent window creation/ownership, event-loop pumping, host GUI resize/show/
hide requests, focus/scaling and callback scheduling remain consumer responsibilities.
The temporary `plugin_handle` allows explicit state/parameter operations without
replacing/moving the borrowed instance; `main_thread_callback` services only an
explicitly scheduled plugin request. The guard does not activate/deactivate audio.

On macOS, the native fixture owns a Cocoa parent and verifies child creation and
removal across three create/attach/show/resize-or-reject/hide/drop cycles:

```sh
cargo run -p ravet-editor-smoke --example hosted --locked -- 'target/clap/release/RAVET Gain.clap'
```

RAVET Gain and the unmodified upstream Clack EGUI gain at the pinned v0.2 commit
both pass. The upstream GUI bundle was built only under target; its baseview/egui
GPU dependencies are not RAVET dependencies. These tests establish local native
child/lifecycle and callback acceptance, not visual hide/focus/accessibility or
all-third-party editor compatibility. Floating GUIs and other OS/API combinations
remain unvalidated. See [decision 0048](decisions/0048-guarded-plugin-editors.md).

## Deferred core host requests

`RequestHost` uses `HostRequests` for CLAP core restart, processing and
main-thread callback requests. Prepare one mailbox and clone it into the instance's
shared handler. Callback publication uses a single atomic byte without allocation,
locks or I/O. Repeated identical requests coalesce; these are pending actions,
not a lossless command history. `take` atomically drains all kinds, and `defer`
returns actions to the mailbox when the consumer cannot yet act.

`service_callback` consumes only a pending main-thread callback and invokes it
on the corresponding instance's main thread. It preserves restart/process flags
and services at most one callback. A reentrant request remains pending until
the next consumer tick. Restart/processing transitions, wakeups, audio ownership,
extension callbacks and application scheduling remain explicit consumer work.
The mailbox must belong to the supplied instance. No native event loop is created.

A linked synthetic plugin requests all kinds during actual CLAP initialization,
then requests another callback from `on_main_thread`. The fixture verifies bounded
deferred service and retained lifecycle actions. Concurrent publication checks
coalescing from three threads. This closes core request handoff, not the complete
host lifecycle or GUI/parameter extension contracts.

## Hosted parameter metadata and inactive edits

`HostedParameters` exclusively borrows a live instance and inspects an explicit
maximum of 1..4096 parameters (zero actual parameters is valid). It owns stable
IDs, flags, raw name/module bytes and finite plain minimum/maximum/defaults.
Invalid IDs, duplicates, failed information queries, invalid ranges/defaults or
excess count reject the whole catalogue. No opaque parameter cookie is retained.
`refresh` replaces metadata only after the new catalogue fully validates;
consumers call it when state/rescan policy requires. The guard prevents cached
extensions from outliving or switching instances.

`value` reads a finite in-range plain value by stable ID. `set_inactive` validates
ID, read-only status, bounds and integral stepped values before dispatching one
global zero-time parameter event. It requires an inactive handle; active edits
belong to audio-thread event processing. The caller supplies the outgoing queue.
Dispatch is not acknowledgement; query the resulting value explicitly. There is
no normalization, guessing of slider skew, automatic rescan or persistence
fallback. Separate opt-in `ParameterHost` handlers receive extension requests. These are control-thread APIs,
not audio processing operations.

Linked Gain tests exercise exact value recall, invalid-edit preservation and
active rejection. A synthetic CLAP parameter provider tests duplicate IDs,
nonfinite/reversed ranges, bad defaults, stepped defaults, missing metadata,
capacity, failed-refresh preservation, read-only edits and invalid returned values.
The offline inspection example prints bounded hosted plain values when available.

## Deferred parameter extension callbacks

`ParameterHost` explicitly advertises CLAP host parameters. Prepare one
`ParameterSharedRequests` and one `ParameterMainRequests` per instance, then pass
clones into its shared and main-thread factories. Shared `core()` preserves core
restart/process/callback handoff; `take_flush()` drains a coalesced atomic request.
Schedule either processing or the appropriate inactive/active flush when ownership
permits. Return an unserved request with `defer_flush()`. Never run process and
flush concurrently. Neither mailbox automatically invokes plugin code.

Main-thread `take()` returns coalesced rescan flags and up to 64 exact stable-ID
clear actions. Repeated IDs merge flags in first-observed order. Clear requests
invalidate consumer references such as automation/modulation; they are not value
edits. Rescan `ALL` needs a deactivated instance before refreshing the catalogue.
The main mailbox stays on its owner thread and cannot be sent/shared across
threads. Callback publication/drain uses prepared storage without heap operations.

More than 64 distinct IDs before draining sets `overflow` and retains the exact
prefix. Treat this as a host fault requiring explicit reference reconciliation or
instance rejection. A partial snapshot cannot prove that all references were
cleared; do not expand lost IDs into clearing unrelated user automation. There is
no automatic recovery, persistence or active automation scheduler. Clear/rescan
and flush drains are separate snapshots, rather than a transaction.

A linked native fixture calls the actual advertised extension during initialization
and its deferred main-thread callback. Tests check exact IDs/flags, coalescing,
bounded overflow, concurrent flush publication, retained core requests and 10,000
prepared callback/drain workloads with zero measured heap operations. See
[decision 0062](decisions/0062-host-parameter-requests.md).

The offline inspection example uses these handlers, rejects clear overflow and
reports exact rescan/clear snapshots. It stores no automation/modulation
references. Initialization flush requests are served by its subsequent process
calls; post-render changes remain explicit diagnostics when the example stops.

## Hosted audio configuration and timing

`HostedAudioConfiguration::inspect` owns an inactive instance's directional port
metadata. Missing audio-port extension means no ports. Each direction accepts
0..9 ports with 1..64 channels per port, unique IDs and optional main port at index
zero. Names remain bytes (at most 256); optional port types remain opaque owned
C strings (at most 256 bytes). IDs may match across directions. Advertised
in-place pairs must resolve to an opposite port of equal channel count. Preferred
64-bit ports must also advertise 64-bit support. These are deliberate validation
limits; larger or invalid declarations reject the whole snapshot.

`refresh` replaces the old snapshot only after full inspection succeeds. Both
inspection and refresh reject active instances. They allocate/call plugin code
on the control thread. Port enumeration preserves independent direction/order;
it does not infer portable main/aux pairs or negotiate a new plugin layout.
Consumers still choose precision and honor common-sample-size flags.

`hosted_latency` queries the main-thread latency extension, including after
activation when latency may change. `hosted_tail` requires a live audio-owner
Clack processor handle. Missing extensions return `None`, which differs from a
reported zero. Tail preserves finite sample counts or infinity (native raw values
at/above i32::MAX); the framework does not allocate in that query. Plugin callback
heap/deadline behavior remains the plugin's own contract. Separate opt-in `ProcessingHost` handlers receive timing/port change callbacks;
automatic compensation updates remain consumer work.

Inspect actual trusted bundle metadata without attempting an offline render:

```sh
cargo run -p ravet-host --example inspect --locked -- --metadata 'target/clap/release/RAVET Sidechain.clap'
```

Linked reference tests check directional stereo/mono sidechain and no-input
instrument ports. A synthetic provider checks malformed/oversized declarations,
failed refresh preservation, active rejection, exact large latency, finite/infinite
tail and 10,000 prepared tail queries without heap operations. The actual built
sidechain bundle also passes the metadata command. See
[decision 0063](decisions/0063-hosted-audio-configuration.md).

## Deferred audio configuration notifications

`ProcessingHost` adds audio-port, latency and tail extensions to parameter-aware
handlers. Prepare a shared parameter mailbox, a main parameter mailbox and an
`AudioConfigurationRequests` mailbox for each instance. Construct
`ProcessingMainRequests` from main parameter/audio mailbox clones for the main
factory; pass an audio mailbox clone to activation's audio-handler factory. Keep
consumer clones outside callbacks for subsequent drains. Core request service
works with either handler type.

`AudioConfigurationRequests::take()` coalesces port rescan flags, latency changes
and tail changes into one atomic snapshot. `defer()` republishes unserved kinds.
Native main-thread port/latency and audio-thread tail callbacks only publish;
there is no plugin reentry, allocation, lock or automatic lifecycle transition.
The host advertises all defined port rescan flags. Non-name changes require
consumer deactivation; this framework's full snapshot API requires deactivation
even for names. Re-query latency after activation and tail on its audio owner.
Consumers must rebuild/retire prepared routes when configuration/timing changes.

The offline example now uses these combined handlers, reports initial/activation/
post-render changes, queries latency after activation and tail before stopping on
the audio worker. It does not restart or rewire itself. Linked fixtures call the
actual advertised extensions on their main/activation/audio paths; tests check
coalescing, deactivation flags, deferred preservation, unknown flag masking and
10,000 prepared zero-heap workloads. See
[decision 0064](decisions/0064-host-audio-change-requests.md).

## Bounded native audio-owner handoff

`hosted_audio_handoff` accepts a tagged Clack stopped processor already activated
by its main-thread `PluginInstance` owner and allocates 1..4-slot pending/retired
queues. Invalid capacity returns initial ownership; full/abandoned offers retain
the rejected token. Keep each matching main-thread instance alive until its
stopped audio token returns for explicit `instance.deactivate`. Consumer tags,
configuration/latency compatibility and reference reconciliation remain explicit.

Move `HostedAudioReady` to the audio owner and call `start`. Initial failure returns
the complete ready state. `HostedAudioRunning` cannot move across threads: its
private started token stays behind audio-owner `process`, `reset`, replacement and
stop operations. A compile-fail example enforces the running engine's non-Send
contract. Prepared native buffer/event/transport validity and ordinary native
fault/silence/sleep policy remain the consumer's responsibility.

At a caller-selected valid block boundary, `apply_pending` accepts at most one
FIFO candidate. Full retirement space prevents dequeue/start/stop. It starts the
candidate before stopping the old processor, then returns the old stopped token
to control. A failed candidate start retires that stopped token and preserves the
old processor/history. Missing control preserves current processing and pending
ownership. There is no crossfade, old-tail drain, migration or automatic restart
request service. Ordinary swaps/process/reset allocate or destroy no framework
storage; native callbacks keep their own obligations and deadline evidence.

Finish on the audio owner outside callbacks. The returned ready engine is
transferable back to main; stop control offers, drain `take_pending` for every
unstarted proposal, then extract `into_current`. Each token must deactivate its
matching instance on main. Retired tokens remain on `HostedAudioControl`. Dropping
a running engine instead stops its current processor on its owner, but explicit
finish preserves tokens for orderly deactivation. Endpoint/token destruction is
outside callbacks. Keep main instances alive through all queued/running/retired
ownership; the handoff does not establish a new native-code trust boundary.

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example audio-handoff --locked -- \
  'target/clap/release/RAVET Gain.clap' org.ravet.gain
# Explicit trusted stereo f32 effect, selected ID, expected default linear gain:
# audio-handoff <trusted CLAP library> <plugin ID> [expected default gain]
```

The example uses no audio device or window and expects one stereo input/output
port. It renders 2000 blocks/32000 frames across two instances, compares samples,
checks measured allocations and returns tokens for main-thread deactivation.
Linked synthetic tests check actual native start/process/reset/stop/deactivate/
destroy thread IDs, initial/candidate failure recovery, backpressure, full/gone
offers, abandoned control, pending shutdown and retained audio. Actual-binary
scope belongs in [work status](WORK_STATUS.md). Graph port mapping, format/device
scheduling, physical deadlines and general third-party compatibility remain
separate work. See [decision 0106](decisions/0106-native-audio-owner-handoff.md).

The handoff adds a direct dependency on already-pinned rtrb 0.4.0, whose reviewed
manifest declares MIT OR Apache-2.0. No new external lockfile package is introduced;
retain its applicable notices when distributing binaries.

## Prepared hosted f32/f64 graph nodes

`prepare_hosted_graph_node` (f32) and `prepare_hosted_graph_node64` (f64) consume a caller-created trusted Clack instance on
main, fixed `HostedGraphNodeOptions` and an audio-handler factory. Inspection
requires zero/one main audio port per direction matching the declared layout;
1..64 channels per nonempty direction, 1..65536 frames, no auxiliary buses or
input/output events. Unsupported configurations/metadata retain instance ownership
before activation. Scratch/native port arrays and one return slot allocate on
main before activation. The optional host dependency adds only internal foundation
and development graph packages; no new external lockfile package is introduced.

The returned `HostedGraphNode` or `HostedGraphNode64` implements the matching
f32/f64 `Processor` for a `GraphNode`. The f64 factory rejects with `Precision`
before activation unless every mapped port advertises 64-bit support. It does not
convert samples. Existing RAVET Gain CLAP is f32; the f64 validation fixture
is a separate public memoryless stereo x2 plug-in.
Its prepare checks the already activated fixed contract; it does not activate on
audio. Use [explicit graph sessions](audio-graphs.md#explicit-audio-owner-sessions).
Audio start/process/reset/stop run on the session owner, with native start rejected
on main and runtime owner checks. Input copies to prepared mutable native scratch;
output scratch clears before native calls and copies back afterward. Empty blocks
skip native processing. Native events remain separate work; packed auxiliary ports use their separate
factories below, and block-start transport has explicit opt-in below. Native tail/latency are consumer-declared graph timing, not guessed from
missing extensions or queried on incorrect threads. Node scratch is at most
32 MiB for f32 or 64 MiB for f64, outside the graph's own cap; aggregate/plugin memory remains consumer-budgeted.

Native success statuses are retained in independent atomic diagnostics; this node
does not automatically sleep, drain tails or restart. Runtime errors silence output
and return `BackendFailure`, acknowledging native histories may have advanced.
Graph fault/reset policy remains explicit. Node-local steady time advances per
native attempt, independent of playback sample position. Signed-clock overflow
makes steady time unavailable until reset. Reset while stopped defers the native
reset until audio start; reset while running executes on its owner. Normal rendering
introduces no framework heap action, locks or I/O; native callbacks retain obligations.

Retain `HostedGraphNodeControl` on main until the node has stopped on audio and
its graph deactivates/drops on control. Node deactivation returns its stopped token;
`reclaim` executes matching main-thread deactivation at most once. Control cannot
move across threads. Early control destruction follows Clack's protective lifetime
leak policy; orderly stopped retirement/reclaim is required. Explicit bounded
`service_callback` uses the installed `HostRequests` mailbox, leaving restart/process
scheduling to the application. After reclaim, `into_instance` recovers the inactive
instance for state/parameter editing or a fresh activation/node. Nodes are one-shot
after deactivation; graph reprepare does not implicitly reactivate them.

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-graph --locked -- \
  'target/clap/release/RAVET Gain.clap' org.ravet.gain
# Caller-selected trusted zero-latency stereo gain probe:
# hosted-graph [--f64] <trusted CLAP library> <plugin ID> [expected default gain]
```

The no-device/no-window example declares the known gain probe's zero latency/no
tail explicitly. It verifies native wet/dry routing across 2000 blocks/32000 frames,
dynamic owner swap/stop, measured allocations and main graph/instance retirement.
Synthetic linked callbacks verify native thread IDs, shape/phase rejection,
backend silence/nonrollback, empty/malformed blocks, sleeping-output initialization,
reset/steady clocks, main callback service and stopped token/instance recovery.
Actual-binary evidence and its limits belong in [work status](WORK_STATUS.md).
No broad third-party compatibility, editor/parameter automation, other format/device
scheduling or physical deadline claim. See
[decision 0108](decisions/0108-hosted-main-audio-graph-node.md).

The native f64 fixture validates wide finite values (including 1e200 and sub-f32
precision) through the actual loaded ABI, wet/dry routes and live graph swap:

```sh
CARGO_BUILD_JOBS=2 cargo build --release -p ravet-host --example graph-fixture --locked
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-graph --locked -- \
  --f64 target/release/examples/libgraph_fixture.dylib org.ravet.host-graph-fixture 2
```

The displayed fixture filename is macOS-specific; use the local Cargo cdylib
artifact on other systems. Only macOS arm64 has actual-binary evidence here.
The fixture has no parameters/state/editor/input-output events or product assets;
its optional known synthetic transport profile is described below.
See [decision 0109](decisions/0109-hosted-native-f64-graph-node.md).

## Packed native audio ports in graphs

`prepare_hosted_graph_ports_node` and `prepare_hosted_graph_ports_node64` select
f32/f64 and pack each direction independently in native enumeration order, then
channel order within each port. Inspect `HostedAudioConfiguration` while inactive
to choose the graph layout and explicit route indices. The layout's input/output
counts must equal the respective native sums; up to nine ports per direction and
64 total channels per direction. Main flags do not determine portable pairing.
All f64 ports, including auxiliary ports, must advertise native 64-bit support.

Graph routes address packed main channel indices; foundation auxiliary buses
remain empty. For the reference sidechain, inputs 0/1 map to native main port 0,
input 2 to the mono detector port 1, and outputs 0/1 to native output port 0.
Prepared per-port buffers preserve native boundaries and use separate input/output
storage. These factories do not negotiate layouts or infer sidechain policy. The
original main-only factories keep their zero/one-main-port restriction.

Bounds, node-owned scratch limits, declared timing, event/transport exclusion,
one-shot owner sessions and main stopped retirement match the main-only profile.
Use the explicit known zero-latency/no-tail sidechain validation profile:

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-graph --locked -- \
  --sidechain 'target/clap/release/RAVET Sidechain.clap' org.ravet.sidechain
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-graph --locked -- \
  --f64 --sidechain 'target/clap/release/RAVET Sidechain.clap' org.ravet.sidechain
```

This profile fixes the reference detector threshold at 0.1, exercises alternating
0.05/0.2 detector samples and verifies two graph instances/swaps/retirement without
a device or window. It is not a general sidechain discovery/host compatibility
claim. See [decision 0110](decisions/0110-hosted-packed-audio-ports.md).

## Opt-in hosted block-start transport

`HostedGraphNode::set_transport_enabled(true)` and the f64 counterpart enable
mapping on main while the native token is stopped, before transferring it to an
audio owner. Default nodes continue to omit native transport. The standalone
`hosted_transport` helper implements the same checked allocation-free conversion.
All absent optional fields produce no native context. If any field is known,
`playing` must be known: CLAP's transport flags cannot express unknown playing.
This policy preserves absence without silently assuming stopped.

Known positive finite tempo, beat position and sample position map to flagged
block-start fields. CLAP uses signed 31-fractional-bit beat/second timelines;
rounding outside that range rejects with `ProcessError::InvalidTransport`, clears
outputs and occurs before native calls/history/steady-time changes. Sample position
converts through f64 seconds, so floating and fixed-point quantization apply; the
whole i64 sample range does not promise exact roundtrip. Negative positions are
allowed. Discontinuity has no native flag and does not reset DSP.

Optional `Transport.details` forwards recording/active-loop/pre-roll flags,
positive signature components, finite BPM-per-sample tempo increment and current
bar start/zero-based number. Increment requires known tempo; bar requires known
beats. Active loops require ordered bounds for every known timeline; supplied
inactive bounds reject. Fixed-point rounding must preserve strict endpoint order.
Unknown details preserve legacy false flags/zero metadata; CLAP cannot distinguish
unknown from false for these status flags. A status-only active loop with neither
timeline is allowed. Negative bars/bounds and non-power-of-two denominators are
accepted. No extrapolation, automatic looping, resetting or mid-block transport
updates are added. See [decision 0127](decisions/0127-extended-block-transport.md).
New metadata has offline conversion evidence; the loaded fixture profile below
continues to cover the earlier playing/position/tempo fields.

The public fixture's `--transport` profile validates exact negative seconds/beats,
positive tempo and flags, then uses alternating playing/stopped blocks for x2/
silent native output. Ordinary no-context fixture behavior remains x2.

```sh
CARGO_BUILD_JOBS=2 cargo build --release -p ravet-host --example graph-fixture --locked
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-graph --locked -- \
  --f64 --transport target/release/examples/libgraph_fixture.dylib org.ravet.host-graph-fixture 2
```

`--transport` is restricted to the known public fixture profile; it is not a
musical host application or device transport. Typed nodes support opt-in with
other explicit instances under their transport contracts. See
[decision 0111](decisions/0111-hosted-block-start-transport.md).

## Prepared global parameter input in native graphs

`prepare_hosted_parameter_graph_node` and its `64` counterpart add an explicit
input profile. Supply `HostedGraphPortMode::{MainOnly,Packed}`, fixed node options
with 1..4096 input-event capacity, and 1..4096 `HostedGraphParameterMapping` entries
from product-stable graph IDs to current native IDs. Main inspects at most 4096
current native parameters before activation. Missing, unknown, read-only and
ambiguous duplicate identities reject while retaining the inactive instance.
Metadata range/stepped constraints are cached without cookies or native pointers.

The whole portable block validates before native calls. Only global parameter
values are accepted; unknown graph IDs, other event kinds, out-of-range values or
nonintegral stepped values reject the entire block. Plain physical f32 event values
widen to native f64, including when audio is f64; parameter precision remains the
portable event precision. No normalization/scaling, per-note targets, gestures,
automatic recording/GUI service or other parameter policy is inferred. Same-frame
order and offsets are preserved with global wildcard targets and empty cookies.

Event slots allocate on main and never grow during processing. Sorted mapping
lookup is bounded, without callback locks/I/O/heap actions introduced by RAVET.
Native plug-ins must honor their callback obligations. Positive-frame input goes
through native process; valid zero-frame input uses active parameter flush on its
audio owner, with no native process/steady-time advancement. `parameter_flushes`
counts dispatches, without claiming acknowledgement. Native output is discarded
in this input-only profile. Parameter/state/rescan changes require stopped
retirement, metadata inspection and fresh preparation; this is a fixed snapshot.
The original audio-only factories keep rejecting nonzero event configurations.

```sh
CARGO_BUILD_JOBS=2 cargo build --release -p ravet-host --example parameter-fixture --locked
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-graph --locked -- \
  --f64 --parameters target/release/examples/libparameter_fixture.dylib org.ravet.host-parameter-fixture
```

The public synthetic stepped stereo gain fixture exposes writable native ID 1 and
read-only ID 2. The graph probe maps graph ID 9001 to native ID 1, sends gain 2 at
frame 0, gain 3 then 1 at frame 4 and gain 0 at frame 12, verifying last-event
order through wet/dry routes/swaps. Each instance also receives an empty-block
gain-4 flush. There are no product assets, state/editor/device or general host
compatibility claims. See [decision 0112](decisions/0112-hosted-graph-parameter-input.md).

## Native parameter output and the preparation builder

`HostedGraphNodeBuilder::new(options)` selects main-only audio by default.
`with_port_mode`, `with_transport_enabled`, `with_parameter_input` and
`with_parameter_output` select independent explicit profiles before `prepare`
(f32) or `prepare64`. The original factories keep their contracts. Validation,
metadata queries and storage allocation happen on main; failed preparation
retains the inactive instance. Configure exactly matching input/output event
capacities, with 1..4096 slots for each selected parameter direction.

Output mappings allow both writable and read-only native IDs. Each direction has
unique native/graph identities, checked against current metadata. Native global
parameter output maps to plain portable f32 values; f64 values round and must
remain finite/in-range/integral when stepped. Full native f64 parameter precision
is not preserved. Typed mapped output checks native ranges, offsets, order and
capacity. Any rejection poisons the block: audio/output events clear and
`BackendFailure` reports nonrollback native history. Unmapped, unsupported or
per-note output is intentionally filtered and counted. Cookies are never retained.
The native ABI still requires valid plugin event memory; this is not crash isolation.

`filtered_native_events` and `rejected_native_events` are independent relaxed
counters. Successful block counters/status require accepted mapped output; native
processing/steady time may advance on an output fault. Zero-frame output profiles
explicitly dispatch active parameter flush even without input, then publish mapped
output at offset zero. Flush counters count dispatch, not plugin acknowledgement.
Audio-only/input-only profiles retain void output behavior. No retry, rescan,
recording/GUI scheduler, per-note/MIDI/modulation/gesture output is added here.

For graph routing, prepare explicit event edges from host to the input node and
node to host (or subsequent nodes). Outer/native node output capacities must
match the graph's prepared routing contract. The public probe maps input graph
ID 9001 to native gain 1 and read-only native output 2 to graph ID 9100:

```sh
CARGO_BUILD_JOBS=2 cargo build --release -p ravet-host --example parameter-fixture --locked
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-graph --locked -- \
  --f64 --parameter-io target/release/examples/libparameter_fixture.dylib org.ravet.host-parameter-fixture
```

It verifies the last requested gain at frame 15 for each normal block and gain 4
at offset zero after flush, through two graph instances/wet-dry swaps. This is
synthetic macOS arm64 actual-binary evidence, without broad plugin/device/DAW/
platform/deadline claims. See
[decision 0113](decisions/0113-hosted-graph-parameter-output.md).

## Selected native note/MIDI input in graphs

The builder's `with_note_input(HostedGraphNoteInput { port_index, dialect })`
selects one current native input **index**, not its metadata ID. Inactive
preparation checks the note-ports extension, a maximum of 256 input ports, the
selected metadata and advertised dialect. Selection is fixed until retirement
and fresh preparation. Input capacity is 1..4096, shared with optional global
parameter input. Original factories retain their existing strict profiles.

`Clap` emits portable NoteOn/Off with selected port/channel/key and wildcard note
ID. `Midi` and `MidiMpe` accept validated short MIDI and portable notes; normalized
velocity rounds to 0..127. A quantized zero-velocity MIDI note-on follows MIDI's
note-off semantics. One/two-byte short messages pad unused native bytes with zero.
`Midi2` accepts framed UMP only, preserving group/words and padding to four native
words; portable normalized notes and short MIDI are rejected in this dialect.
No protocol conversion, MPE zone negotiation, group rewrite, SysEx assembly,
native note ID or note-expression translation is inferred.

Offsets and same-frame ordering survive mixed parameter/performance input.
Whole-block validation and encoding precede native calls. Unselected event kinds,
dialect mismatches and invalid values reject before mutation and silence audio.
Performance input in a zero-frame block rejects, including mixed parameter
prefixes: parameter flush is not a performance delivery API. Valid parameter-only
zero-frame behavior remains unchanged. A note-only node needs no params extension.
All event storage is prepared on main; native plugin callback obligations remain
its responsibility. Native note output uses the separate explicit profile below; port-rescan servicing remains separate.

The synthetic public parameter fixture exposes input index 1 with metadata ID
101 and four dialects. It deliberately checks the received target/payload. These
commands verify sample-timed mixed streams and mapped read-only output through
prepared graph edges, wet/dry routes, live swaps and main retirement:

```sh
CARGO_BUILD_JOBS=2 cargo build --release -p ravet-host --example parameter-fixture --locked
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-graph --locked -- \
  --f64 --ump-notes target/release/examples/libparameter_fixture.dylib org.ravet.host-parameter-fixture
```

Use `--clap-notes`, `--midi-notes` or `--mpe-notes` for the other selected dialects;
omit `--f64` for native f32. The independent public sine probe uses its actual
MIDI input index 0 and no audio input, checking timed onset and release silence:

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-sine --locked -- \
  'target/clap/release/RAVET Sine.clap'
```

These are synthetic offline native-binary checks, with no device playback or
DAW/platform/deadline guarantee. See
[decision 0114](decisions/0114-hosted-graph-note-input.md).

## Selected native performance output in graphs

`with_note_output(HostedGraphNoteOutput { port_index, dialect })` selects one
inactive-validated output index and advertised dialect, independently of input
selection and global parameter mappings. At most 256 outputs may be advertised;
1..4096 output slots are shared across accepted parameters/performance. A
performance-only profile needs no parameter mappings or params extension.

CLAP NoteOn/Off require the selected concrete port, concrete channel/key and
wildcard note ID. Specific note IDs, wildcard channels/keys, choke/end/expression
or other dialects/ports filter and count, because the portable events cannot
retain those meanings. Accepted normalized native f64 velocity narrows to f32
and must be finite within 0..1. MIDI/MPE retain complete short-message wire bytes
(including zero-velocity note-on), ignoring unused native padding. Malformed
selected statuses/data reject; SysEx has no short-message representation. MIDI2
retains framed UMP words/groups, ignoring unused padded words and making no
semantic conversion. No MPE configuration or automatic MIDI1/MIDI2 conversion.

Accepted events share exact native offsets/order/capacity checks with parameters.
Malformed selected output, overflow, out-of-range offset or decreasing order
poisons the whole block: clear audio/event prefixes and report nonrollback
`BackendFailure`. Native history/steady time may already advance. The existing
filtered/rejected counters cover both output kinds. Performance emitted during
zero-frame parameter flush rejects; the adapter never uses flush to deliver
performance. A params-less empty output-only block makes no native call.
Native event memory still must satisfy the trusted plugin ABI contract.

The `hosted-graph` note switches above now select both input and output. The
synthetic fixture echoes selected performance at its original frames, then emits
mapped read-only gain at the final frame. Explicit all-event graph edges verify
both directions through wet/dry swaps; parameter-only switches retain their old
mapping profiles. See
[decision 0115](decisions/0115-hosted-graph-note-output.md).

## Main-owned editors for hosted graph nodes

`HostedGraphNodeControl::editor(configuration)` creates the existing guarded
`PluginEditor` while borrowing the retained native instance. The separate audio
node/graph may continue running on its owner thread. Rust borrowing prevents
control reclaim/drop until the editor is destroyed. The main control/library
remain alive; editor drop pairs successful creation with hide/destroy. This adds
no editor calls to graph processing and does not change audio ownership.

The caller owns the OS main thread, live compatible parent and main event loop.
`PluginEditor::service_callback(&requests)` services at most one coalesced core
callback from the instance's own host mailbox, preserving restart/process flags;
re-requests wait for another tick. Parent/resize/focus, GUI extension requests,
parameter flushes, restart/rebuild and device scheduling remain consumer policy.
Close/destroy the editor before reclaiming the stopped graph token.

The native macOS example creates three guarded embedded editors while its hosted
gain graph processes 1000 blocks per editor. It checks child cleanup, resize,
finite exact stereo output, callback heap actions, audio stop and main retirement:

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-editor-smoke --example graph-hosted --locked -- \
  'target/clap/release/RAVET Gain.clap'
```

This explicit probe is for trusted zero-latency stereo default-unity gain bundles;
it opens no device or DAW session. See
[decision 0116](decisions/0116-hosted-graph-editor-ownership.md).

The optional device crate has an explicit buffered f32 graph-worker output
bridge. A non-Send graph session stays on its worker while a separate device
processor consumes a prepared queue; stop/join precedes main graph/native
retirement. No device clock or callback-thread teardown is inferred. See
[the device guide](audio-devices.md#buffered-graph-worker-output) and
[decision 0117](decisions/0117-buffered-graph-worker-output.md).

Buffered output workers also accept explicitly staged bounded graph input via
`pump_events`, acknowledging it only when a fresh render succeeds. Pending audio
publication does not consume or repeat new parameters/notes. Native profiles
must match the prepared event capacity. See
[the event-input guide](audio-devices.md#bounded-graph-worker-event-input) and
[decision 0118](decisions/0118-graph-worker-event-input.md).

`pump_io` additionally returns a caller-owned bounded generated-event prefix once
per successful fresh render. Pending audio publication never repeats generated
events; offsets describe the rendered block, without device-clock mapping or
control-thread delivery. See
[the output guide](audio-devices.md#bounded-graph-worker-output-events) and
[decision 0119](decisions/0119-graph-worker-event-output.md).

Fixed complete-batch event queues can connect main control to graph workers and
return worker-generated values. A borrowed batch stays retained until explicit
acknowledgement, and full/invalid publication exposes no prefix. Tags/context
remain caller-owned and scheduling/recovery stay explicit. See
[the queue guide](audio-devices.md#complete-graph-worker-event-batches) and
[decision 0120](decisions/0120-graph-worker-event-queues.md).

## Global parameter gesture output

`HostedGraphNodeBuilder::with_parameter_gesture_output(true)` includes native
global begin/end events alongside explicitly mapped parameter output values.
An output mapping is required before activation. Boundaries alias native IDs to
portable IDs, require writable cached metadata, and share prepared output capacity.
Disabled or unmapped boundaries filter. Mapped read-only boundaries, invalid
ordering/offset or overflow fail the entire block with nonrollback backend failure.
Zero-frame parameter flushes permit boundaries at offset zero. Pairing, repeated
ends, ownership, cancellation, recording and GUI delivery remain receiver policy.
The default output profile filters boundaries; native input translation is separate.

The public synthetic gesture fixture exercises f32/f64 rendering, zero-frame flush
and live graph swaps without audio allocation:

```sh
CARGO_BUILD_JOBS=2 cargo build --release -p ravet-host --example gesture-fixture --locked
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-graph --locked -- \
  --gestures target/release/examples/libgesture_fixture.dylib org.ravet.host-gesture-fixture
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-graph --locked -- \
  --f64 --gestures target/release/examples/libgesture_fixture.dylib org.ravet.host-gesture-fixture
```

This synthetic native probe opens no editor/device/DAW and does not establish
host automation recording. See [decision 0121](decisions/0121-parameter-gesture-events.md).

## Mapped global modulation in hosted nodes

`HostedGraphNodeBuilder::with_parameter_modulation_input(true)` and
`with_parameter_modulation_output(true)` independently enable global modulation
on their explicit parameter maps. Both default false and require the matching
mapping at preparation. Input must target cached writable native modulatable
parameters; unsupported targets reject the whole input before native mutation.
Amounts widen from finite f32 to f64 independently of scalar base range/step checks.

Enabled output requires writable modulatable cached metadata and finite f64 amounts
that narrow to finite f32. Disabled, unmapped and scoped native output filters;
selected invalid amount/order/offset or shared-capacity overflow clears the block
with nonrollback backend failure. Zero-frame parameter flush carries modulation
at offset zero. Native DSP owns combination/clipping/smoothing and persistence;
these signals do not overwrite saved base values by themselves. Metadata changes
require fresh stopped preparation, without cookies or automatic rescans.

The public synthetic modulation fixture keeps a stepped base separate from a
fractional signed modulation signal and reports both through explicit aliases:

```sh
CARGO_BUILD_JOBS=2 cargo build --release -p ravet-host --example modulation-fixture --locked
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-modulation --locked -- \
  target/release/examples/libmodulation_fixture.dylib
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-host --example hosted-modulation --locked -- \
  --f64 target/release/examples/libmodulation_fixture.dylib
```

This checks 1000 native blocks per precision, timing/flush/base telemetry, no audio
heap actions and main retirement; it opens no device/GUI/DAW. See
[decision 0123](decisions/0123-hosted-global-modulation.md).

Buffered workers can also consume explicit queued f32 audio input, routing native
effects between input/output queues without native lifecycle on device callbacks.
Fresh renders zero input shortages and report queue source/underflow progress;
pending output never consumes input twice. Graph faults retain their error until
explicit reset. See [the device guide](audio-devices.md#queued-audio-input-for-graph-workers)
and [decision 0124](decisions/0124-buffered-graph-audio-input.md).


## Mapped parameter gesture input

`HostedGraphNodeBuilder::with_parameter_gesture_input(true)` enables raw global
begin/end input alongside scalar values. An explicit input mapping is required
before activation. Cached mappings already require writable native metadata;
unknown or disabled boundaries reject the whole input before native rendering or
parameter flush, even after a valid scalar prefix. Boundaries share the prepared
input capacity, retain process timing/order and accept offset zero during empty
parameter flushes. Repeated ends are raw input, with no pairing/ownership inference.
They do not themselves change saved base values. The receiver owns recording and
GUI/DSP policy. No MIDI port is needed. Default input rejects portable boundaries.

Synthetic linked native f32/f64 fixture tests echo mapped input to separately
mapped output with exact timestamps, preserve base audio/values, normalize empty
flush timing, reject disabled/unknown input atomically, require a map before
activation and measure 1000 blocks without heap activity. This is offline ABI
fixture evidence; no third-party gesture receiver, UI recording or DAW support
claim. See [decision 0130](decisions/0130-parameter-gesture-input.md).


## Opt-in native note identities

Select an explicit CLAP note input/output port, then enable the matching builder
`with_note_identity_input`/`with_note_identity_output`. This profile carries
`NoteWithId` On/Off events, preserving concrete channel/key, sample order, native
0..=i32::MAX identity and f32 velocity (native output narrows checked f64 0..1).
No graph/native ID aliasing is inferred. Zero-velocity On retains On semantics.
Unknown voice/lifetime/duplicate ID policy belongs to the receiver.

Default input rejects identified notes; default output continues to filter them.
Unsupported MIDI/MPE/UMP selection or absent matching port rejects before
activation. Invalid input IDs reject the whole input before native mutation,
even after a valid note prefix. Selected identity output rejects raw IDs below
native -1 wildcard instead of Clack's negative-to-wildcard normalization; unrelated
ports filter. Wildcard-ID native output retains legacy NoteOn/Off. Performance
zero-frame policy and output fault/nonrollback rules remain unchanged.

Portable validation/queue tests preserve full u32 IDs; native unit tests cover
0/i32::MAX, zero On and malformed negative IDs. Linked f32/f64 fixture tests cover
1000 blocks per profile, echo/time/audio, disabled filtering, complete-prefix
rejection and no measured heap activity. Offline fixture evidence only, without
third-party voice/DAW/native deadline support. Scope, choke, completion and scoped parameter mappings use separate profiles below.
Scoped expression routing is described below. See
[decision 0132](decisions/0132-native-note-identities.md).


## Selected standard note expressions

Enable `with_note_expression_input(true)` or `with_note_expression_output(true)`
after selecting the corresponding CLAP note port. These choices are independent
of identified On/Off selection. Standard volume/pan/tuning/vibrato/expression/
brightness/pressure preserve full f64 values and optional channel/key/note ID
matching scope; the selected port is exact, rather than a wildcard across ports.
Portable IDs are u32 and native IDs are limited to nonnegative i32. Disabled or
invalid input rejects the complete input before native mutation. Missing ports
and MIDI/MPE/UMP dialect selection reject before activation.

Disabled native output, unknown expression types and other/wildcard ports filter.
For selected standard types, raw channel/key/ID values below -1 and invalid
finite ranges reject rather than becoming wildcards or clipped values. Selected
capacity/timing/scalar failures retain native history but silence/discard complete
portable output. Performance in an empty parameter flush remains unsupported.
The pinned Clack 0.2.0 type accessor omits Expression, so conversion explicitly
matches all seven public wire discriminators.

Portable, queue and native unit tests cover ranges, all seven types, wildcard
scope, exact f64 values and malformed fields. Linked f32/f64 fixtures cover 1000
blocks per profile, exact echo/audio timing, disabled filtering, whole-prefix
rejection, output capacity/nonrollback behavior and no measured heap actions.
This is offline ABI evidence; voice application and third-party/DAW/deadline
validation remain separate. See
[decision 0134](decisions/0134-scoped-note-expressions.md).


## Scoped release/choke directions

`with_scoped_note_input(true)` and `with_scoped_note_output(true)` require explicit
CLAP ports for their corresponding directions. Input converts NoteOffScoped and
NoteChoke without loss of optional channel/key/native ID, independently of
identified On/Off and expression flags. Complete disabled/invalid input rejects
pre-native. Output preserves wildcard-channel/key Off and choke; concrete native
Off retains legacy/identified selection. Default scoped output filters.

Exact selected-port raw scope checks reject channel/key/ID below -1 and out-of-
range concrete channel/key; other or wildcard ports filter. Off velocity narrows
checked finite native 0..1 to f32; choke's ignored native velocity is not validated
or carried as a scalar. Prepared capacities, input zero-frame rejection, output
fault/nonrollback and retirement rules remain unchanged. Voice release/termination
and NoteEnd completion belong to distinct receiver/protocol policies. Linked
f32/f64 fixtures test 1000 blocks per selection, exact echo/audio timing, invalid
prefix rejection, capacity/malformed output history and main-thread reclamation
without measured processing heap actions. See
[decision 0136](decisions/0136-scoped-note-release.md).


## Mapped scoped parameter values and modulation

`with_scoped_parameter_input(true)` and `with_scoped_parameter_output(true)`
require explicit parameter maps and a selected CLAP note **input** port before
activation. Output scope refers to originating input notes, independently of any
selected note output port. Existing nonzero prepared input/shared event capacities
apply. Input maps are writable; selected scoped output also requires writable
metadata. Default input rejects scoped portable events and default output filters
scoped native parameters.

Preparation caches native range, stepped/read-only and all scope flags. This
profile selects the most specific supplied dimension: note ID, then key, then
channel, then port. Only its corresponding per-scope automation/modulation flag
admits the event. Global automation/modulation flags grant no scoped support;
scoped modulation does not require the separate global modulation flag. The
remaining supplied target fields still restrict matching. All-wildcard NoteTarget
remains scoped to the exact selected input port.

Input checks mapping, scope/native ID, finite full-f64 values and native metadata
before any native callback. Scoped values obey cached range and integral stepped
constraints; signed finite scoped modulation is independent of base bounds/steps.
A bad event after a valid global/scoped prefix rejects the entire input. Output
filters unknown IDs and unrelated/wildcard ports; selected malformed scope,
unsupported flags/read-only metadata, range/step/scalar/timing/capacity reject
complete portable output after native mutation. Cookies are never retained and
input constructors use empty cookies. Only exact raw all--1 tuples are global,
preventing malformed scope normalization into global edits.

Scoped parameter process and zero-time flush preserve full f64 value/amount and
scope without narrowing to audio precision. No global state or voice application
is synthesized. Metadata/rescan/state changes require stopped retirement and new
preparation. Linked native f32/f64 fixtures cover 1000-block selected directions,
exact audio/echo/flush, separate saved base state, invalid prefixes/native ID width,
capacity/nonrollback and all four scope granularities. Independent native flag
combinations, scalar/step/unknown-ID, unsupported output and preparation checks
pass with measured heap-free prepared workloads. Offline synthetic ABI evidence
only; no third-party voice/DAW/deadline claim. See
[decision 0138](decisions/0138-hosted-scoped-parameters.md).


## Completion notifications on originating input endpoints

`HostedGraphNodeBuilder::with_note_end_output(true)` preserves `NoteEnd { target }`
notifications from the selected CLAP note **input** endpoint. A note output
selector, identity output profile and parameter mappings are not required. The
selected input profile requires nonzero input capacity, and completion delivery
requires nonzero bounded output capacity shared with other retained events.
Disabled completion output filters; unsupported input dialects/absent routes and
capacities reject before activation. Portable completion input to a hosted plug-in
rejects the whole input before native mutation, including preceding valid edits.

CLAP specifies that completion reports voice lifetime against the original note
input tuple and ignores timestamp and velocity; see its
[event contract](https://github.com/free-audio/clap/blob/main/include/clap/events.h).
The adapter checks raw channel/key/optional nonnegative i32 ID, ignores native
time/velocity, and filters other or wildcard ports. The portable carrier offset
is the previous retained event's offset, or zero if none, preserving delivery
order without inventing a termination instant. Subsequent timed events still obey
normal ordering. Selected malformed scope/capacity faults discard complete
portable output with native history retained. Parameter-only zero-frame flush
rejects completion output. The framework infers no voice tracking, duplicate-ID
policy or notification from note-off/choke.

Linked f32/f64 fixture tests cover complete delivery without a selected note
output or parameter output map, independent filtering, ignored maximum/backward
native timestamps and NaN velocity, carrier ordering, rejected input prefixes,
capacity faults, retained saved state and 1000 blocks without counted heap
actions. Raw tests cover native bounds, wildcard target, malformed fields and
flush rejection. Offline ABI evidence only; no third-party/DAW claim.
See [decision 0140](decisions/0140-note-completion.md).
