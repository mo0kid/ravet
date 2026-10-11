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

## Live convolution transitions

Enable `ravet-graph`'s default-disabled `convolution-transitions` feature for
`convolution_transition(initial, maximum_frames, TransitionOptions)`. It works with
`ravet-signal`'s `Convolver32/64` and `ConvolutionBank32/64`. Construction on CONTROL
returns a `Send` control endpoint for a worker and an audio endpoint. Construction creates
a one-slot handoff, a four-slot returned-engine queue, a 16-slot report queue, one channel
of sample scratch and per-frame gain scratch. This uses the same boxed-owner `rtrb`
handoff and deferred retirement as graph replacement.

The worker prepares an engine or bank, for example from `prepare_impulse_response`, and
calls `offer`. Channel count, maximum frames and, under `LatencyPolicy::RequireEqual`,
latency are checked there; a refused engine is returned unchanged. `Ok` means the engine
was queued, not that it will play. Audio `process` admits the whole call before any
change, then handles at most one proposal per block boundary (including zero-frame calls).
A `CrossfadePolicy { frames, law }` runs old and new engines together for `frames` frames:
`y = S(g_old) * y_old + S(g_new) * y_new` with `t = (k + 1) / frames`. `EqualGain` uses
`(1 - t, t)` and `EqualPower` uses `(cos, sin)(t * pi / 2)`, computed once per frame and
equal to (0, 1) at the end to rounding. Setting `frames = 0` is an explicit cut.

The new engine starts from its own prepared (empty) history. For a response longer than
the fade, its output still lacks the tail from earlier input, so for stationary input the
blend dips in level by up to `1 - (energy of the first frames taps / total energy)`. It
recovers as the history fills. Engines with different latencies are not aligned. Both
limits have explicit, optional remedies described under
[transition continuity](#transition-continuity-history-priming-and-latency-alignment).

When a fade ends, the old engine returns through the returned queue, or through one
deferred slot while that queue is full. If neither has room, it stays as a silent,
unprocessed outgoing owner and is retried at the next boundary. Further starts wait
(`retirement_blocked`) until the worker calls `take_returned` or `reclaim`, which drop
engines on that thread. Returned engines keep their history; call their `reset` before
reuse. Mid-fade proposals follow `MidFadePolicy`:

- `Refuse` returns a proposal that arrives during a transition unused. If the returned
  queue is full, it stays marked refused in the handoff and is returned as soon as a slot
  frees, even after the fade. It never starts.
- `QueueOne` keeps the proposal in the one-slot handoff (further offers get `Full`). It
  starts at the first boundary after the fade.
- `LatestWins` keeps one waiting proposal and returns older ones as `Superseded`. At the
  first boundary after the fade, a proposal offered after the last mid-fade boundary
  supersedes the waiting one, and the newest starts.

`LatencyPolicy::Report` accepts latency changes and reports `TransitionEvent::Started {
previous_latency, latency, fade_frames }` with absolute frames. The consumer applies its
host's latency change; during a fade the output mixes both latencies. Per-call
`TransitionStatus` is authoritative, while reports are best-effort with loss counts.
`reset` clears both engines' histories and keeps the fade position.

Audio-side `process` and `reset` do not allocate, free, lock or panic. Construct and drop
both endpoints outside the callback:

- Dropping the audio endpoint frees its current, outgoing, waiting and deferred engines on
  the dropping thread.
- The queues, with any engines still queued in them, are freed by whichever endpoint is
  dropped last.
- If control is dropped first, audio keeps processing without freeing anything until its
  own endpoint is dropped.

Tests show bit-exact blends against independently computed references for f32 and f64.
They cover both laws, three partition pairs and block sizes 64, 1, 7, random and zero.
They also cover each mid-fade policy, latest-wins after the fade, refuse under
backpressure, latency refusal and reporting, refused calls and offers leaving state
unchanged, and zero audio-side allocations and frees through handoff, fade, retirement,
backpressure and shutdown. Debug and release runs include a three-seed worker/audio
stress test of each policy. The transition is not a `Processor`, and offline tests make
no deadline claim. See
[decision 0272](decisions/0272-impulse-response-preparation-and-live-convolution-transitions.md).

### Transition continuity: history priming and latency alignment

`convolution_transition_with_continuity(initial, maximum_frames, options, continuity)`
takes a `TransitionContinuity { priming, alignment }`. `TransitionContinuity::NONE`
(the default) behaves exactly like `convolution_transition`, which delegates to it.
The default path's ADR 0272 behaviour is guarded by the 15 unchanged transition tests
and by digests of output samples, statuses, offer results, reports and returned engines
recorded from the ADR 0272 build in 108 scenarios.
Construction checks every length, and the total storage (at most 256 MiB of samples),
before allocating. Storage is reserved fallibly, and a refusal (`Priming`, `Alignment`,
`Storage`, `Allocation`) returns the identical initial engine.

`PrimingPolicy::History { frames, frames_per_block }` records the last `frames` input
frames of each channel in a ring prepared at construction. When a proposal is taken,
the incoming engine first processes the recorded history and its output is discarded.
Its state then matches an engine that had been running on the same input. Choose
`frames` of at least the incoming `tail_samples` (`taps - 1 + latency`). The output
from the fade start then equals a never-switched engine with the new response to
rounding, and the level dip disappears. If fewer than `frames` frames have been
recorded (shortly after construction or `reset`), only the recorded frames are primed.

Priming work is bounded at `frames_per_block` history frames per channel per block
boundary, on top of normal rendering. Construction requires either
`frames_per_block >= frames` or `frames_per_block > maximum_frames`:

- If the recorded history fits in one step, the fade starts at the boundary where the
  proposal is taken.
- Otherwise the fade is delayed. The old engine plays alone while the incoming engine
  catches up through the ring, and the fade starts at the first boundary whose
  remaining lag fits. With blocks of at most `b` frames and an initial lag `g`, at
  most `ceil((g - frames_per_block) / (frames_per_block - b))` blocks are delayed.

`TransitionStatus::{priming, primed_frames}`, `TransitionEvent::PrimingStarted` and
`priming_sequence`/`priming_lag` expose the schedule. A proposal that is being primed
counts as an in-progress transition for `MidFadePolicy`, including a proposal that is
pending at the boundary where priming completes: `Refuse` refuses it, `QueueOne` keeps
it queued and `LatestWins` moves it to waiting, superseding an earlier waiting
proposal. `reset` forgets the history, and a proposal being primed then starts at the
next boundary from empty history. A single-block prime can cost up to `frames`
frames of engine work at one boundary; choose `frames_per_block > maximum_frames` to
spread it over several blocks.

`AlignmentPolicy::DelayLower { maximum_difference }` prepares two delay lines of
`maximum_difference + maximum_frames` frames per channel. Each line records its
engine's raw output. For a transition from latency `a` to `b`, the lower-latency path
is delayed by `|a - b|` during the whole fade, so both paths are aligned at
`max(a, b)`:

- If `b > a`, the old path is delayed from the fade start. The output latency changes
  to `b` at the start frame, and the old path repeats its last `b - a` frames while
  its gain is near one.
- If `b < a`, the new path is delayed. The output latency stays `a` until the frame
  where the fade completes, then changes to `b`, skipping `a - b` frames of the new
  path while only it is audible.
- A cut changes the output latency at the start frame, with no delay.

Without priming, the incoming engine has no output from before the fade start. When
`b < a` its delayed path therefore reads zeros for the first `a - b` fade frames and
is silent while its gain rises; if `a - b` is at least the fade length it contributes
nothing before the release, and the level dip is deeper. Priming fills those frames.

Each change is reported in `TransitionStatus::output_latency_change` and
`TransitionEvent::OutputLatencyChanged` with its absolute frame. `output_latency_samples()`
gives the current value; the consumer applies its host's latency change. A proposal
whose latency differs from the current engine's by more than `maximum_difference` is
refused at the boundary where it would start. It is returned unused as
`ReturnReason::Refused` with `TransitionEvent::AlignmentRefused` and
`TransitionStatus::alignment_refused`, through a reserved returned slot. Without a free
slot it stays where it is (`retirement_blocked`) and nothing starts. The check happens
on the audio side because only that endpoint knows which engine is current.

Both policies are allocation-free on the audio side. Each block copies the input
into the ring and each engine's output through its line, in time proportional to the
block. Taking a proposal forgets the incoming line and `reset` forgets both, in
constant time: each line counts the frames written since it was forgotten and reads
older positions as zero, so no clear proportional to the prepared size is needed.
During a fade, the delayed path (old or new) therefore reads zeros for its delay after
a `reset`.

Tests check bit-exact output against independently computed references for f32 and
f64 banks across block sizes 64, 1, 7 and random:

- **Priming.** A primed engine equals a fresh engine fed the recorded input. For a
  4,096-tap response with a 256-frame fade, the windowed level dip is -7.50 dB without
  priming (-7.44 dB predicted from the missing tail energy) and 0 dB with priming.
  Output equals a never-switched engine bit-for-bit when the history start is
  partition-aligned, and to 3.3e-16 (f64) or 1.8e-7 (f32) of peak otherwise.
- **Delayed fade.** The fade starts at the independently simulated boundary, within the
  documented bound, with at most `frames_per_block` primed frames per block.
- **Alignment.** Aligned blends have zero error. The default policy's misalignment
  reaches 1.22 times the peak during the fade. On 148 impulses across three latency
  changes, the measured onset always equals the reported output latency.

Further tests cover priming from a history shorter than requested (before the ring
fills and after `reset`), `LatestWins` proposals arriving during priming, `reset`
during aligned fades in both directions against an independent reference, refusals
that leave output, histories and returned engines untouched (including under
backpressure), setup refusals with zero allocations, zero audio-side allocations
through priming, alignment, refusal and reset, and a three-seed worker/audio stress
test of each mid-fade policy. These are offline results;
they make no deadline claim. See
[decision 0275](decisions/0275-convolution-transition-priming-and-latency-alignment.md).

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

Hosted VST3 Graph-bank nodes accept these events only through an explicit
per-node translation: declared parameter routes, a selected note bus and
transport, with refuse or ignore policies. See
[framework event translation](vst3-hosting.md#framework-event-translation).
A refused block is silenced and reported as `ProcessStatus::Silence`, so the
Graph continues; the reason is counted and recorded on the bank control, not
in `GraphStatus`. Broadcast routing therefore needs either `Ignore` policies
or explicit per-node event edges, unless silent refused blocks are acceptable.

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

## Typed serial stages and shared-control mono histories

`TypedChain<S, T>` owns a sealed heterogeneous tuple of one through eight concrete
`Processor<S> + Send + 'static` stages and uses the existing `Graph` scheduler for
execution. Both native sample precisions are available. `stage::<I>()` preserves
the concrete type; invalid static indices fail to compile. The initial profile is
an effect with equal nonzero main input/output channel counts (1..64), at most
65,536 frames and 1,024 input events at construction. Construction admits sizes
that `prepare` can still refuse because of the existing 64 MiB Graph scratch cap
(for example 64 channels, 65,536 frames and eight stages). Auxiliary buses and
generated output events are rejected. Accepted input events broadcast through the
canonical graph policy. The facade supplies no parallel topology, instruments or
channel conversion.

`stage_mut_stopped::<I>()` returns an exclusive concrete processor after control
thread deactivation and invalidates rendering until successful reprepare. Readonly
getter borrows exclude concurrent rendering/mutation.

### Audio sessions: prefer the guard

`start_audio()` returns the canonical non-Send audio guard with typed readonly
access and bypass flags. Prefer it: it stops every stage in reverse order on the
starting thread when it is finished or dropped, and its borrow keeps control
mutation, extraction and transfer out of reach until then. A forgotten guard
(`mem::forget`, a leak) is misuse, not valid use: the audio owner stays live and the
object that would have stopped it is gone.

### Manual `Processor` hooks are a caller obligation

Consumers may instead call the `Processor` hooks directly. `Processor::start_audio`
must then be paired with `Processor::stop_audio` on the thread that started it,
before transfer to another thread, deactivation, extraction or drop. Unlike the
guard, this is a caller obligation: the facade adds no automatic stop, no owner-thread
cleanup and no thread check while rendering, and a manually started chain is `Send`.
While a manual owner (or a forgotten guard) is live:

- `prepare` and `stage_mut_stopped` **refuse** with `AudioSessionActive`;
- `deactivate` (inherent and `Processor`), `into_stages` and drop do **not** refuse;
  they **panic** through the canonical Graph assertion;
- `stop_audio` on a different thread panics and leaves the owner live;
- rendering, reset, readonly getters and bypass flags keep working.

A caught `deactivate` panic changes nothing: the chain stays prepared with the owner
live and can still be stopped on its owner thread. A panic from `into_stages` or drop
destroys the facade during the unwind: each stage is dropped exactly once, without
`stop_audio` or `deactivate`, and the original tuple is not returned. If that panic
happens while the thread is already unwinding, the second panic aborts the process;
no safeguard against this is provided. `into_stages` only drops the internal graph;
it never stops audio. Foreign lifecycle panics remain exceptional and cannot
manufacture a successful recovery receipt.

### Bypass, faults, storage

Per-stage bypass copies corresponding dry channels and skips processing and input
event delivery, freezing child histories. Reset still reaches bypassed children.
Prepared child latency/tail declarations are retained: the dry bypass signal is
**not latency compensated**, and consumer/host alignment is separate policy.
Canonical whole-block preflight refuses before child execution. A child fault can
follow committed earlier histories; the original first `GraphStatus` remains
available through `first_fault()`, including after a failed start. The `Processor`
facade maps executed-child faults and poisoned render retries to `BackendFailure`,
with complete output/event silence. A start retried on a faulted graph returns
`NotPrepared` and keeps the original fault. Reset/reprepare/deactivation clears the
fault; no history rollback occurs.

Stable private storage consists of exactly one `UnsafeCell` record per stage. All
wrapper and getter pointers derive from that cell, and owner/session borrowing
excludes overlapping references. The internal graph drops before the records,
including during unwind; extraction drops the graph before safely recovering the
original tuple. Sealed internal helpers require a private-construction permit,
preventing safe external generation of duplicate wrappers or exposed storage.
The private wrapper is transferable only when its processor is `Send`; it makes
no `Sync` promise. Record-Vec payload is checked against 4 MiB before/after fallible
reservation. Ordinary constructor refusals recover the exact original tuple.
Existing Graph allocations and caller-owned processor storage have separate
limits; allocation aborts and foreign panics are not recoverable refusal receipts.
The `SAFETY` comments in the module state the validity, provenance and exclusivity
argument per access; it is reasoning, and no Miri run supports it.

### Shared-control mono histories

`MonoDuplicator<S, C, K>` owns one control object, shared by reference with 1..64
separate histories implementing `SharedMonoKernel<S, C>`. The explicit control
thread factory must return inactive kernels. Its failure returns original
controls/factory, exact constructed prefix and original error; prior factory side
effects do not roll back. Checked kernel-Vec payload is at most 4 MiB; kernel-owned
and control-object memory remain caller obligations. Every kernel reads the same
control object during a call, with no framework control clones, mutexes or callback
reference-count operations. The framework never mutates the object while kernels
are prepared, but sharing a reference does not make it immutable: keeping the object
effectively immutable while prepared (or making interior mutation deterministic and
real-time safe) is an implementor obligation, as are kernel work, callback
allocation/locking behavior and nonfinite/denormal policy. The framework does not
filter or reject nonfinite samples and bypass forwards them unchanged.

Preparation accepts exact equal main channels with no events, auxiliary buses or
generated events. Failed kernel preparation is inactive and deactivates earlier
successful kernels in reverse exactly once; `prepare_failure()` records index and
original error and is cleared by the next `prepare` only. A refused or failed
`prepare` leaves the duplicator unprepared. Dropping an active duplicator
deactivates its prepared kernels once, in reverse. Latency and tail are the
maximum across kernels while prepared. Scalar/frame/planar/in-place rendering
preflights complete shape and frame capacities before histories or outputs change;
`Processor::process` additionally silences active output frames on refusal; empty
prefixes/suffixes are preserved. Bypass returns dry samples and freezes histories.
Mutable stopped control access invalidates readiness and requires reprepare.
`into_parts()` returns original controls and kernel owners after once-only control
thread deactivation. There is no hidden coefficient migration, smoothing, state
crossfade or native sample conversion.

### Qualification status

The test sources cover every tuple arity/index, both precisions/all 64 mono counts,
typed chains up to 64 channels, constructor/prepare/factory refusal ownership,
start-failure payload and retry mapping, reverse audio rollback and reverse stop for
eight stages, ordinary and unwinding drop/extraction including manual-owner panics
and owner-thread recovery, discriminating getter/session compile-fail examples,
real sample-timed Gain nodes, shared-control stopped mutation through the typed
getter, and shared TPT/ladder histories through the typed chain, canonical Graph and
existing FIR. The f32 consumer deliberately bridges to the existing f64 FIR.
Independent difference-equation, four-dimensional nodal Newton and convolution
oracles cover six selected nonlinear profiles; allocation-counter windows cover the
guard render loop, bypass, the `Processor` facade and the getters, and direct mono
render/bypass paths. These tests do not establish a hardware deadline or foreign
processor behavior.

The revised isolated package passes 74 runtime tests and 21 doctests, strict
all-target lint, strict Rustdoc and formatting. Root checked all 295 declared
source inputs and command-log hashes. A moved test-fixture address assumption and
a test-only lint failure were preserved before correction; executable production
behavior is unchanged. These results do not establish public integration. No Miri
pass is claimed; the cell provenance proof remains a review argument. See
[ADR 0260](decisions/0260-typed-chain-shared-mono-ownership.md).
