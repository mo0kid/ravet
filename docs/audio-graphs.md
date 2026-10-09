# Prepared processor routing graphs

The explicit `ravet-graph` crate schedules owned portable f32 or f64 processors in a
static acyclic graph. Explicit hosted f32 nodes use the separate
[host adapter](plugin-hosting.md#prepared-hosted-f32f64-graph-nodes). This scheduler is not
an implementation of the `Processor` trait: a later node's runtime fault cannot
roll back already processed node histories or requested parameter values.
Its `GraphStatus` reports that distinction explicitly.

A graph owns at most 128 stable-ID nodes and 4096 channel connections. Each node
supplies its accepted main-bus layout and `Box<dyn Processor<S> + Send>`; controllers,
GUI ownership and proprietary wrappers stay outside this graph. Routes connect
host input or node output channels to node input or host output channels with
finite gains. Parallel routes sum in declaration order. Missing channels/IDs,
duplicate node IDs and cycles reject during construction. Empty graphs can
perform direct channel routing. Disconnected nodes still execute.

Preparation calls all node lifecycles and allocates scratch/delay storage. It
accepts the declared host layout, up to 65536 frames and 1024 input events, no
auxiliary buses. Default broadcast mode has no event output; explicit event
routing enables bounded generated events. Total graph-owned prepared storage is capped at
64 MiB (16777216 f32 or 8388608 f64 values); this excludes node-owned allocations and temporary
preparation peaks. Maximum accumulated path latency is 65536 samples. Failure
leaves every node and the graph inactive.

Topology follows dependencies, independent of node declaration order. Incoming
routes align to the latest input arrival across a node's channels. Output routes
align to the latest host output arrival, including delayed dry routes. Declared
node latency drives prepared delay buffers; changing latency requires reprepare.
Tail declarations combine serial tails and route delays, choosing the maximum
parallel tail; overflow conservatively becomes infinite. No implicit tail drain
or time-varying latency compensation is supplied.

Input is copied before host output writes, allowing in-place channel swaps and
parallel dry/wet routes. Unconnected inputs are zero. Complete outer layout/
event validation precedes node execution. Events broadcast to all nodes by default; transport always broadcasts at block
start. Event offsets remain unchanged; consumers need globally unique parameter IDs or
nodes that ignore unrelated events. Explicit per-node event routing and generated output are described below.

A node's silence status is enforced. A node fault clears host output and disables
the graph until reset; earlier histories/parameters may already have advanced.
Reset clears all node histories and delay lines without allocation while retaining
requested parameters according to node contracts. ABI panic containment belongs
to the caller. Graph editing, deactivation and dropping belong after callbacks
stop. No processing allocation, locks or I/O are introduced by the scheduler;
node implementations remain responsible for their own callback behavior.

Tests cover reversed declarations with serial/parallel gain and sample events,
latency-compensated impulse paths, block partitions/reset, invalid outer data,
fault disable/recovery, cycles/IDs/channels, empty cross-channel aliases and failed
reprepare. 1000 prepared render/reset workloads report no Rust heap actions.
Hosted native f32/f64 main-only or explicit packed native port integration uses
owner sessions, with separate opt-in checked block-start native transport. Explicit global parameter input/output maps stable graph identities and routes
verified native global output through prepared event edges; other
native event mappings, feedback/delay cycles, editor/device scheduling
and callback deadlines remain separate work. See
[decision 0029](decisions/0029-prepared-audio-graphs.md).

## Native graph precision

`Graph<S>`, `GraphNode<S>` and `Connection<S>` select one native sample precision
for the whole graph, defaulting to f32. `Graph::<f64>` owns f64 processors, route
gains, scratch and latency delay lines; no precision conversion occurs. Mixing
f32/f64 nodes requires an explicit separate adapter, which this graph does not
provide. The sealed `GraphSample` contract admits only foundation f32/f64 samples.

The 64 MiB graph-owned preparation limit counts bytes at the selected precision.
Default f32 call sites retain existing behavior. Native f64 fixtures preserve
values below f32 resolution through delayed parallel routes and exact channel
alias swaps, check split-block equivalence/reset, reject oversized byte storage,
and report zero heap operations across 10,000 prepared render/reset callbacks.
These are offline processing checks, not f64 DAW/device or deadline evidence.
See [decision 0055](decisions/0055-native-graph-precision.md).

## Prepared graph replacement

`DynamicGraph::new` pairs one prepared active graph with a unique `GraphControl`
and two 1..4-slot ownership queues. Control prepares an entire replacement and
`offer`s it. Its full `ProcessConfig`, precision and declared path latency must
match the initial contract; inactive/faulted/mismatched graphs and full queues
return graph ownership to the caller. No graph is rebuilt on audio.

For each valid outer block, audio selects at most one queued graph in FIFO order.
Invalid input silences output before consuming any proposal. Replaced graphs
return through retirement storage; `take_retired` transfers ownership to control
for reuse, or bounded `reclaim` destroys them there. If retirement storage is
full, replacement waits and the current graph renders. Dropping control prevents
further replacement while keeping the current graph available. Construct/drop
both endpoints, reclaim graphs and prepare proposed histories outside callbacks.

Replacement starts the proposed graph's history and does not preserve old node
state, drain old tails, crossfade or change the host's latency. Consumers own
discontinuity policy, parameter/state transfer and aggregate memory budgets.
Each graph's scratch cap is 64 MiB; this is not a cap on all in-flight graphs or
node-owned storage. The selected graph retains the explicit fault/reset policy.
The engine itself is not a `Processor` implementation.

The graph crate pins MIT-or-Apache-2.0 `rtrb` 0.4.0 for safe SPSC ownership
transport; see its [producer contract](https://docs.rs/rtrb/0.4.0/rtrb/struct.Producer.html).
It stays outside the portable umbrella. Tests check accepted FIFO replacement,
backpressure, invalid-input preservation, control loss, returned errors, exact
configuration and native f64 precision. Destructor probes report no node drops
in callbacks; 10,000 graph-reuse/render/reset iterations and 100 synchronized
control/audio-thread transfers report zero callback heap operations. Scheduler
deadlines and native host/device use remain unvalidated.

## Explicit event graphs

While inactive, `set_event_routes` replaces default input broadcast with up to
4096 explicit event connections. An empty list delivers no events. Edges connect
host input or generated node output to node input or host generated output,
selecting all events, one stable parameter ID or performance events (notes/MIDI/
UMP). Repeated edges deliberately duplicate events. IDs and combined audio/event
cycles validate before replacing routes or order; active edits reject. Event
dependencies participate in the node topological schedule even without audio edges.

Preparation uses `maximum_events` for each node input and `maximum_output_events`
for each node writer and host output. Both maxima are at most 1024. When host
output capacity is nonzero, the outer callback supplies matching initially empty
`OutputEvents`. Event scratch is included in the graph's 64 MiB byte cap. Equal
frame events retain connection declaration order then source order; merging sorts
only by offset and never allocates. Zero-frame callbacks accept offset zero.
Offsets are preserved exactly: audio compensation does not delay events or move
packets into future blocks. Consumers own audio/event alignment policy.

Merge overflow returns `GraphStatus::EventFault` with source/destination IDs
(`None` means host), silences audio, discards all host generated output and
disables processing until reset. Earlier node histories may have advanced.
Generated host output is copied only after the complete merge succeeds, so no
partial event prefix escapes a graph fault. This follows the scheduler's existing
explicit nonrollback policy; it does not acquire the atomic `Processor` contract.

Tests cover reversed declaration order, parameter/performance filtering, generated
chains, stable equal-time ordering, zero-frame events, duplicate-edge overflow,
failed-edit preservation, combined cycles, fault reset and 10,000 heap-free
merge/reset callbacks. A native f64 graph routes opaque UMP words unchanged.
Native host/device event scheduling and deadline evidence remain separate.

## Explicit audio-owner sessions

`Processor::start_audio`/`stop_audio` are optional prepared session hooks with
no-op defaults. Ordinary portable graph `process` behavior stays unchanged;
existing format/device adapters do not automatically opt into these hooks.
Nodes requiring split native thread lifecycle use an explicit graph session.
Control prepares the graph, transfers it to its audio owner, then calls
`Graph::start_audio` or `DynamicGraph::start_audio` on that owner. The returned
borrowed session is non-Send/non-Sync and exposes rendering/reset/timing queries,
without exposing the underlying mutable graph. Finish/drop it on that owner
before transferring the prepared graph back for control deactivation/destruction.
Do not forget/leak the guard. Active preparation rejects `AudioSessionActive`;
deactivation requires the session to have ended.

Nodes start in topological order. A returned start failure leaves the failing
node stopped, rolls earlier successful nodes back in reverse order and faults
the graph until reset. A local rollback guard also stops earlier nodes during
unwinding start panics; the failing hook retains its own rollback obligation.
Successful session finish stops nodes in reverse order, retaining preparation and
histories. Default portable hooks leave existing render/reset results unchanged.
Hook implementations obey prepared callback allocation/locking/I/O contracts;
panic containment remains caller ABI policy and has no real-time guarantee.

While a dynamic session is active, accepted candidates start on the same owner
before old nodes stop. A failed candidate start is returned through retirement
with `GraphSwapStatus::StartRejected`, and the old graph keeps rendering. Full
retirement or invalid outer blocks prevent candidate execution. Old nodes stop
before their graph enters control retirement; control still deactivates/destroys
it. Session finish stops only the currently selected graph. Proposals must be
prepared, fault-free and outside an audio session, with existing exact config/
latency checks. No crossfade, tail drain, state migration or implicit reprepare.

Thread/callback fixtures check start rollback (error and unwind), reverse stop,
invalid input, candidate failure/backpressure, retained rendering, control-only
deactivation/destruction and heap-free rendering/swaps. Native f64 default hooks
retain sub-f32 precision and large values. This supplies lifecycle mechanics;
it does not itself map native CLAP ports/events, schedule a device or prove
platform/deadline support. A dedicated audio worker can hold a session; a native
callback requiring Send cannot capture this non-Send guard. See
[decision 0107](decisions/0107-graph-audio-owner-sessions.md).

The native preparation builder also selects one explicit note/MIDI input index
and dialect, sharing bounded slots with optional global parameters. Whole input
validates before native calls, preserving offsets/order; zero-frame performance
rejects before parameter flush. CLAP notes use wildcard IDs, MIDI/MPE quantize
velocity, and MIDI2 preserves framed UMP without translation. See the
[selected-input guide](plugin-hosting.md#selected-native-notemidi-input-in-graphs)
and [decision 0114](decisions/0114-hosted-graph-note-input.md) for limits and probes.

A separate selected native performance-output profile shares bounded output
storage with mapped parameters. It preserves representable CLAP notes, short
MIDI/MPE or framed UMP through explicit graph edges; unsupported targets filter,
and selected malformed output clears audio/events with nonrollback failure.
See [the output guide](plugin-hosting.md#selected-native-performance-output-in-graphs)
and [decision 0115](decisions/0115-hosted-graph-note-output.md).

Hosted native graphs expose guarded main-thread editor creation through their
retained control. The separately owned audio node may run, while exclusive
editor borrowing prevents control retirement until GUI destruction. Core
callbacks can be serviced once per consumer main tick through the guard.
See [the hosting guide](plugin-hosting.md#main-owned-editors-for-hosted-graph-nodes)
and [decision 0116](decisions/0116-hosted-graph-editor-ownership.md).

The optional device crate has an explicit buffered f32 graph-worker output
bridge. A non-Send graph session stays on its worker while a separate device
processor consumes a prepared queue; stop/join precedes main graph/native
retirement. No device clock or callback-thread teardown is inferred. See
[the device guide](audio-devices.md#buffered-graph-worker-output) and
[decision 0117](decisions/0117-buffered-graph-worker-output.md).

## Explicit parameter gesture events

`EventData::ParameterGesture { id, phase }` carries a raw global begin/end
boundary with stable parameter identity and sample offset. It changes no scalar
value and imposes no ownership, pairing or recording policy. Parameter graph
filters include matching boundaries; performance filters exclude them. Complete
worker event queues retain their identity, phase and order. Generated CLAP output
maps boundaries without a MIDI port; hosted graph output requires an explicit
opt-in and writable output mapping. See [the hosting guide](plugin-hosting.md#global-parameter-gesture-output)
and [decision 0121](decisions/0121-parameter-gesture-events.md).

Finite global `ParameterModulation` signals also travel through parameter graph
filters and complete event queues with stable identity/order. They do not change
saved base values by themselves or imply native node support. See
[the parameter guide](parameters.md#global-modulation-signals) and
[decision 0122](decisions/0122-global-parameter-modulation.md).

Prepared dynamic f32 engines can also run through buffered worker input/output.
Offers are considered only on fresh renders, after old copied audio suffixes
publish, with explicit swap progress and historical handoff outcome on errors.
Main retirement remains separate from worker lifecycle. See
[the device guide](audio-devices.md#live-replacement-in-buffered-graph-workers) and
[decision 0126](decisions/0126-buffered-live-graph-workers.md).
