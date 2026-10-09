# Standalone audio devices

The explicit `ravet-device` crate adapts CPAL f32 output, input capture and
bounded duplex effect streams to portable processors. It is outside `ravet`'s
dependency graph. MIDI devices, integrated recording, event queues, hotplug
subscriptions and automatic recovery remain pending.

## Control thread

`output_devices()` returns host-local IDs, names and output configuration ranges.
`output_device(Some(id))` selects that ID with no default fallback; `None` selects
the current default. Refresh enumeration after device changes. IDs belong to the
backend and are not promised to be portable across machines or reinstallation.
CPAL errors are returned rather than replacing devices silently.

Pass an explicit CPAL `StreamConfig` to `open_output`, a maximum scratch frame
count and an owned `Processor<f32> + Send + 'static`. There is no implicit sample
format conversion, sample-rate fallback or resampling. The device must support
f32 at the requested settings. Fixed buffer sizes are backend requests; callbacks
may have another size. Prepare allocates scratch before building the stream.
Call `play`, `pause` and drop from the control thread. Pause/resume retains history
and logical sample position. Backend errors increment a counter; recovery is
caller-directed stream reopening, not a callback action.

## Processing thread

`PreparedOutput` is also independently usable for offline adapter tests. It
prepares 1..=64 output channels, no inputs, no events/auxiliary buses and
1..=65,536 scratch frames per channel. Larger callbacks are split into bounded
blocks and their planar outputs interleaved into the device buffer. This policy
requires processors whose output remains correct across block partitions.
Transport carries a monotonic logical sample position starting at zero; it is
not a device clock or host playback/latency estimate.

A partial interleaved frame rejects and silences the entire callback without
advancing DSP. A processor fault silences its sub-block; later sub-blocks continue
and time advances. Silence status is enforced. Scratch is cleared before each
sub-block, so omitted output writes cannot expose stale audio. The framework's
normal rendering path allocates/deallocates nothing and takes no locks or I/O.
Processor implementations retain their own callback obligations.

The live callback contains unwinding panics, silences output and permanently
disables DSP for that stream. Reopen to recover. Panic handlers/unwinding are
exceptional and have no real-time guarantee; panic=abort cannot be contained.
Diagnostics use relaxed atomic counts, not coherent meters, logs or latency
measurements. Native backend operations are outside the Rust allocation counter.

## Probe and evidence

```sh
CARGO_BUILD_JOBS=2 cargo run -p ravet-device-probe --locked -- --list
CARGO_BUILD_JOBS=2 cargo run -p ravet-device-probe --locked -- --validate-silence
# Select an enumerated device explicitly:
CARGO_BUILD_JOBS=2 cargo run -p ravet-device-probe --locked -- --validate-silence device-id
```

The probe opens the selected device's default f32 output configuration for one
second and produces silence on every channel. A non-f32 default configuration
returns an error. It fails if no callbacks/frames occur or any fault/error/panic
is reported. This does not test audible output, input, round-trip latency or
callback deadlines. [Work status](WORK_STATUS.md) records the exact tested setup.
Offline tests check interleaving, partition/time continuity, fault silencing,
malformed frames, preparation bounds, panic disable/silencing and 1,000
allocation-free callbacks. Callback counts include disabled callbacks; frame
counts describe completed DSP sub-blocks, not hardware clock time.

## Dependencies and distribution

CPAL 0.18.2 is pinned, with default features and no optional ASIO/JACK backends.
Its downloaded manifest declares Apache-2.0. The macOS dependency manifests
reviewed during integration declare coreaudio-rs 0.14.2 MIT/Apache-2.0,
dasp_sample 0.11.0 MIT OR Apache-2.0 and mach2 0.6.0 BSD-2-Clause OR MIT OR
Apache-2.0; objc2/block2 declare MIT; generated objc2 framework bindings and dispatch2
declare Zlib OR Apache-2.0 OR MIT, and libc declares MIT OR Apache-2.0. Retain applicable notices when distributing
binaries. This scoped review is not a release-wide audit of every target's
backend dependencies; follow [licensing policy](../LICENSING.md) before release.
Linux's default ALSA backend needs system development libraries; other targets
still need native runtime validation. Compilation alone does not establish
platform support.

## Input capture and duplex effects

`input_devices` enumerates IDs/configuration ranges, and `input_device` selects an
exact ID or the default without fallback. `open_input` opens an explicit f32
stream and returns its unique audio ring consumer. `PreparedCapture` supplies the
same capture path for offline tests. Ring capacity is a power of two 1..65536
frames, with 1..64 channels. Capture publishes a whole-frame prefix, drops the
newest suffix on overflow and counts it. A partial frame rejects the whole input
buffer without touching queued data. No input samples are written to files or
transmitted by these APIs.

`PreparedDuplex` dequeues bounded interleaved input, zero-fills underflow and feeds
prepared planar effects; output callbacks partition at 1..65536 scratch frames.
Input/output counts are independently 1..64. Stats expose underflow frames;
faults and silence are enforced as in the output adapter. The live wrapper uses
the shared panic containment path and disables DSP until reopening after panic.
Events, auxiliary buses and output event capacity are zero.

`open_duplex` builds explicit input and output streams with matching nominal
rates, a ring capacity and scratch maximum. Input starts before output; failed
output start attempts to pause input. Pause attempts both streams, retaining FIFO
and DSP history. Restart is sequential, not sample-synchronized. Independent
clocks can drift, producing counted underflow/overflow; there is no automatic
resampling or compensation. Even on one interface, this FIFO path does not claim
sample-exact capture/output alignment or round-trip latency. Reopen to clear FIFO
or recover errors. Use a separate worker/ring for recording through
[WAV services](audio-files.md); integrated transport/record scheduling is pending.

```sh
CARGO_BUILD_JOBS=2 cargo run -p ravet-device-probe --locked -- --list-input
CARGO_BUILD_JOBS=2 cargo run -p ravet-device-probe --locked -- \
  --validate-duplex-silence input-id output-id
```

The duplex probe receives one second of input into the bounded FIFO and emits
silence on every output. It retains only counters, with no recorded sample file.
Both default configurations must be f32 at the same rate. The probe requires
input/output callbacks and no backend/DSP errors; underflow/overflow counters
are reported, not hidden. Offline tests cover capture backpressure, invalid-frame
rejection, queued ordering, variable duplex partitioning, underflow silence,
processor fault silence and 1000 heap-free capture/render workloads. See
[decision 0028](decisions/0028-input-duplex-devices.md).

## Caller-scheduled output recovery

`RecoveringOutput` owns an exact host-local output ID, fixed f32 configuration,
scratch bound and a factory for fresh processors. Construction validates these
bounds without opening a device or invoking the factory. The first enabled
`service` enumerates the exact ID, prepares/opens/starts output and reports its
opened generation. Healthy streams report Running. Missing devices pause/drop
output and report Waiting; backend errors or permanently disabled panic DSP
cause pause/drop before a fresh open. Ordinary block processor faults alone do
not trigger reopening. Enumeration errors retain the existing stream; failed
open/start leaves no stream for a later retry. There is no device/default fallback.

Applications call service on their control thread and choose poll cadence/retry
backoff. No timer, hotplug subscription, settings change, state migration or
clock compensation is created. The factory owns restored state and any repeated
preparation policy. Statistics belong to the current stream and reset on each
successful open; the diagnostic generation wraps at u64 exhaustion. Disabling
immediately releases output; reenable opens fresh DSP on next service.
`request_reopen` releases now and schedules another enabled open. Pause errors
while releasing are best effort; stream destruction still follows. Backend
work/destruction can block. Native hosted plug-ins need their own thread-correct
lifecycle; this owner does not supply CLAP/VST3 scheduling.

```sh
CARGO_BUILD_JOBS=2 cargo run -p ravet-device-probe --locked -- \
  --validate-recovery-silence output-id
```

Controlled tests cover disappearance, unhealthy stream, failed-open retry,
healthy preservation, enable/drop ordering and constructor validation without
factory/device work. The silent native probe manually reopens and disables/
reenables three generations, asserting callback/fault counters and exact
control-thread processor preparation/deactivation. UMC1820/macOS arm64 20-output
96 kHz evidence is recorded in work status. This proves those restart paths;
physical unplug/replug, real backend-error recovery, other devices/platforms,
duplex recovery and deadlines remain unvalidated. See
[decision 0090](decisions/0090-explicit-output-recovery.md).

## Caller-scheduled duplex recovery

`RecoveringDuplex` extends the same explicit recovery ownership to an exact
input/output pair, with fixed f32 configurations at equal nominal rates, scratch
bound and a power-of-two 1..65536-frame ring. It validates both IDs/layouts/rates/
capacities without native enumeration or factory work. A first service opens
fresh streams/FIFO/DSP and starts input before output. Missing either device,
input/output backend errors or output DSP panic releases both streams before
retry. A known-missing input stops the pair before output enumeration. Other
enumeration errors retain the current pair. FIFO underflow/overflow and ordinary
processor faults alone do not reopen. Failed open/start leaves no pair.

Disable/reopen drops the pair; reenable creates a fresh ring and processor on
next service. Statistics sample current input/output counters independently;
generation counts refer to pairs. Factory-owned state restoration, polling/
backoff, best-effort pause/destruction and thread/lifecycle limitations match the
output recovery owner. No physical recording file, fallback, settings change,
hotplug subscription, drift correction or sample synchronization is introduced.
Matching nominal rates still permit independent-clock FIFO underflow/overflow.

```sh
CARGO_BUILD_JOBS=2 cargo run -p ravet-device-probe --locked -- \
  --validate-recovery-duplex-silence input-id output-id
```

Constructor tests cover pair/layout/rate/ring bounds without factory/native work;
the shared controlled lifecycle tests cover release/retry ordering. The native
probe checks absent exact input/output IDs without changing hardware and three
silent generations with callback/fault/control-thread lifecycle counts. Samples
remain only in each transient FIFO and are not retained or written. UMC1820
18-input/20-output 96 kHz evidence belongs in work status. Physical removal,
real backend errors, clock drift, other devices/platforms and callback deadlines
remain unvalidated. See [decision 0091](decisions/0091-explicit-duplex-recovery.md).

## Explicit clocked duplex conversion

`PreparedClockedDuplex` and `open_clocked_duplex` add opt-in input-clock conversion.
Capture and output use their own explicit f32 configurations; nominal rates may
differ within 8000..192000 Hz and the complete corrected rate ratio must remain
within the [sinc converter](signal-processing.md) bounds. The processor prepares
at the output rate, with 1..64 inputs/outputs and no auxiliary buses/events.
Nothing changes device settings or chooses fallback devices/formats.

`ClockedDuplexOptions` sets 1..65536 scratch output frames, even 16..256 taps per
input channel and an explicit `FifoClockConfig`. Its capacity must match the
power-of-two capture ring (2..65536 frames); target occupancy is strictly inside.
Conversion storage is allocated before processor preparation; all rendering
storage is allocated before native output stream construction.
The existing live callback contains panics and disables DSP until reopening.
Prepared ordinary conversion/rendering allocates nothing. Native operations and
processor code retain their own timing obligations.

Initially, input remains zero until capture reaches target occupancy. Thereafter,
queued capture plus prefetched input feeds the bounded smoothed proportional
clock controller, capped at ring capacity for feedback. Positive correction
consumes input faster. All input channels share the same correction and conversion
phase. The conservative fixed filter cutoff covers the complete allowed ppm range;
no live table replacement occurs. Output chunks are converted and immediately
processed; callback counts refer to native buffers, not scratch partitions.

If capture runs out, remaining input is zero, counted as underflow. Converter and
controller histories reset and input reprimes; processor history and logical output
position continue. Priming zeros are counted separately. The gauge includes
prefetched input and may exceed raw ring capacity by up to one scratch block.
Clock and ordinary stream statistics use independent relaxed atomic loads, without
one coherent complete-block snapshot. Nonfinite audio propagates and converted
f32 overflow/headroom is not guaranteed.

Start, pause and destruction use the existing paired-stream ownership. Pause
preserves FIFO/converter/DSP histories; start remains sequential, without physical
sample synchronization. Explicit reopen creates fresh histories. Automatic
recovery/hotplug, MIDI, recording, latency estimation and native plug-in lifecycle
scheduling are separate work. The proportional controller has a steady occupancy
offset and cannot guarantee arbitrary configuration stability or long-session
physical drift handling from synthetic tests.

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-device-probe --locked -- \
  --validate-clocked-duplex-silence input-id output-id [seconds]
```

This probe requires explicit enumerated IDs, uses each device's default f32
configuration, emits only silence and retains only counters. It checks three
fresh pairs (default one second each; optional 1..60 seconds), pause/resume, callback/error/correction bounds and control-thread
processor preparation/deactivation. Underflow, overflow and priming remain visible.
Native evidence and its limits belong in [work status](WORK_STATUS.md); the probe
alone does not establish physical alignment, long-session stability or deadlines.
See [decision 0104](decisions/0104-clocked-duplex.md).

## Caller-scheduled clocked duplex recovery

`RecoveringClockedDuplex` owns exact input/output IDs, fixed f32 configurations,
`ClockedDuplexOptions` and a fresh processor factory. Construction checks IDs,
channels/rates, native buffer bounds, FIFO/controller/sinc limits before native
enumeration or factory calls. Sinc preflight checks allocate no coefficient tables.
The first enabled `service` opens and starts the pair. Missing either device,
either backend error or output panic releases both streams before retry; input
absence releases before attempting output enumeration. Enumeration errors retain
the existing pair. Ordinary DSP faults or capture overflow/conversion underflow
alone do not reopen. A failed open/start leaves no pair for the next service.

Disable and `request_reopen` release immediately; reenable/service creates fresh
FIFO, converter/controller histories and factory-owned DSP. Healthy service
preserves the current pair. Input/output/clock counters are independent and reset
on successful reopen; pair generations wrap at u64 exhaustion. Control-thread
poll cadence/backoff, factory state restoration and blocking backend operations
remain application choices. No default fallback, settings mutation, hotplug
subscription, automatic timer or native plug-in thread scheduling is introduced.

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-device-probe --locked -- \
  --validate-recovery-clocked-duplex-silence input-id output-id
```

The silent probe checks missing exact IDs without opening streams, three pairs
with manual reopen and disable/reenable, callback/error counters and exact
control-thread preparation/deactivation. It records no input files. Shared
controlled session tests cover failure/retry/release ordering. Physical unplug,
real backend faults, long-session drift and deadlines require separate evidence.
See [decision 0105](decisions/0105-clocked-duplex-recovery.md).

## Buffered graph-worker output

`PreparedGraphOutput::new(graph, producer, config)` prepares a fresh no-input f32
graph and planar/interleaved worker scratch. Config requires zero input/
auxiliary capacities, 0..1024 events in each direction, 1..65536 frames and matching 1..64 producer
channels. Worker scratch is at most 32 MiB, separate from graph/native scratch.
Move it to an explicitly owned worker and call `start_audio` there. The borrowed
`GraphOutputSession` retains the graph's non-Send owner session, stopping nodes
on that worker before prepared storage returns to main for retirement.

Each `pump(transport)` renders one fixed block only after the prior queue suffix
has been published. Partial publication retains exact whole-frame order, without
rerendering or advancing native history again. Progress separates rendered,
published and pending frames. Caller transport applies only to a fresh render;
no device clock is inferred. Graph faults publish none of the faulty block, but
prior native histories may advance. Explicit `reset` resets graph histories while
preserving queued/pending audio. Stop/join endpoint owners before queue clearing.
No loop, sleep, spin, thread creation or I/O occurs inside pump.

`RingOutput::new(consumer, rate)` is a no-input f32 Processor for `open_output` or
`PreparedOutput`. Preparation requires matching rate/channels and zero events/
auxiliary buses. Valid blocks consume a whole-frame prefix, zero every missing
sample, and count source/underflow frames. Invalid blocks do not dequeue. Clone
its `monitor()` on control before callback handoff for relaxed independent live
counters; these are not coherent latency/clock measurements. RingOutput does no
rate conversion, priming or waiting. Consumer scratch is at most 16 MiB.

This explicit queue bridge keeps graph/native lifecycle on a dedicated worker,
rather than relying on device callback teardown to stop native nodes. The caller
owns worker wake/sleep/priming, matching rates, device pause/drop, worker stop/join,
graph retirement and retained native control reclaim. Queue underruns, buffering
latency, worker deadlines, input/live edits/recovery remain separate work; bounded
explicit event input is described below.

The public silent probe loads only the trusted RAVET Sine reference, sends no
notes, and routes stereo silence into selected output channels. Its default run
uses an offline device renderer; explicit native selection uses host-local IDs:

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-device-probe --bin graph-output --locked -- \
  'target/clap/release/RAVET Sine.clap'
# Explicit short native silent run, with an ID from the existing --list command:
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-device-probe --bin graph-output --locked -- \
  'target/clap/release/RAVET Sine.clap' '<device-id>' 96000 20 10
```

The tool primes 4096 queue frames, measures graph-pump heap actions, reports device
and queue counters, then stops/joins the worker before main node/instance return.
Offline tests do not establish device/platform/deadline support. See
[decision 0117](decisions/0117-buffered-graph-worker-output.md).

On macOS arm64, the public loaded sine worker passed a 10-second silent UMC1820
run at 96 kHz/20 output channels: 1875 callbacks, 960000 source frames, zero queue
underflows/processor faults/backend errors/panics, then owner stop/main return.
This short observation does not establish clock alignment, long-session drift,
physical faults or deadlines. See W110 in [work status](WORK_STATUS.md).

## Bounded graph-worker event input

Prepare 0..1024 input slots, with compatible native node input profiles where
needed. `GraphOutputSession::pump_events(events, transport)` accepts the borrowed
ordered sample-offset stream only for a fresh fixed render. Its progress field
`input_events_consumed` is false while an earlier audio suffix is still draining:
retain incoming events and context for another call. No parameter or note applies
twice merely because queue publication was partial. `pump(transport)` remains the
empty-input convenience method. Both convenience methods provide empty output
storage; use pump_io below for a configured positive output capacity.

Graph validation rejects malformed outer input before node execution; graph
faults can still follow earlier node mutation. An error is not delivery
acknowledgement or a safe automatic retry. Pending calls do not validate or
consume newly supplied input. Consumer event queues, timestamp conversion,
device-clock mapping, retiming late input, GUI service and seek/recovery policy
remain explicit; this is an owner-thread pump API, not an asynchronous scheduler.

The native public sine probe has an offline-only note mode, with timed note-on
and note-off routed through its selected native MIDI graph input and the queue:

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-device-probe --bin graph-output --locked -- \
  --offline-notes 'target/clap/release/RAVET Sine.clap'
```

It verifies delayed onset, finite stereo sound, release silence and measured
heap-free worker/device rendering. This mode rejects device arguments and opens
no native stream. The existing native mode stays silent. See
[decision 0118](decisions/0118-graph-worker-event-input.md).

## Bounded graph-worker output events

Prepare 0..1024 output slots and explicit graph output-event routes, with matching
native node profiles where needed. `pump_io(events, transport, storage)` takes
caller-owned storage of exactly the configured output capacity for each fresh
render. The returned `output_event_count` prefix is valid; storage tails remain
untouched. Pending audio-suffix calls return zero output count and leave storage
untouched. Input acknowledgement still distinguishes fresh renders from pending
publication. Missing/wrong output capacity rejects before graph node processing
when a fresh block is needed.

Generated events are returned once when rendering succeeds, even if the queue
accepts only a prefix or no frames yet. Offsets refer to that rendered block;
the worker makes no audible/device-clock delivery promise. Consumer-owned
telemetry queues, block identity/correlation, GUI/control delivery and scheduling
remain explicit. Graph/output faults produce no valid returned prefix and may
follow native input/history mutation; no automatic retry or replay is performed.
Queue publication of a faulty rendered block is suppressed.

The native public fixture probe maps stepped gain input and read-only gain output
through a two-node graph, checking sample offsets and exactly-once event return
while a four-frame queue retains eight-frame block suffixes:

```sh
CARGO_BUILD_JOBS=2 cargo build --release -p ravet-host --example parameter-fixture --locked
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-device-probe --bin graph-telemetry --locked -- \
  target/release/examples/libparameter_fixture.dylib
```

It opens no device or GUI. See
[decision 0119](decisions/0119-graph-worker-event-output.md).

## Complete graph-worker event batches

`graph_event_queue(GraphEventQueueConfig { block_frames, event_capacity,
queued_batches })` prepares a unique producer/consumer pair for either control
input or worker-generated output. Limits are 1..65536 fixed frames, 0..4096 events
and 1..64 queued batches. Each batch occupies capacity+1 fixed slots, including
unused payload: publication commits the entire header/context/tag and event
stream atomically. Queue values use at most 32 MiB, plus one retained consumer
buffer. Create/clone any surrounding control objects outside callback processing;
endpoint/buffer drop belongs after producer/consumer owners stop.

`try_publish(tag, transport, events)` validates count, ordered in-block offsets,
finite parameter values and normalized notes before publication. MIDI/UMP remain
validated exact payloads. Full/invalid input publishes no prefix; disconnected
consumer rejects when observed. Tags and transport are opaque caller context,
without automatic uniqueness, clock conversion or native transport validation.

`next_batch()` dequeues a complete batch into prepared retained storage and lends
its immutable values. Dropping the borrow retains it; `acknowledge()` explicitly
releases the pending batch after consumption or deliberate discard. No newer
batch replaces it. A consumer may retain one batch in addition to the configured
queued budget. Producer teardown leaves pending/queued batches available to drain.
Use input acknowledgement from pump_io to retain staged input during partial
publication; graph faults require explicit consumer policy. Output queue fullness
also requires retained-data, discard or pacing policy; never rerender to replay
native-generated events. Publish/read/ack have no allocation, locks, wait or I/O.

The native graph-telemetry probe now uses separate complete-batch queues for main
input and worker output, checks 1000 tags/parameter streams, once-only mapped
read-only output, retained audio suffixes and owner/main retirement. It opens no
GUI/device and does not convert worker tags into audible/device timing. Consumer
wake/sleep, input staging and GUI/recovery policy remain explicit. See
[decision 0120](decisions/0120-graph-worker-event-queues.md).

## Explicit parameter gesture events

`EventData::ParameterGesture { id, phase }` carries a raw global begin/end
boundary with stable parameter identity and sample offset. It changes no scalar
value and imposes no ownership, pairing or recording policy. Parameter graph
filters include matching boundaries; performance filters exclude them. Complete
worker event queues retain their identity, phase and order. Generated CLAP output
maps boundaries without a MIDI port; hosted graph output requires an explicit
opt-in and writable output mapping. See [the hosting guide](plugin-hosting.md#global-parameter-gesture-output)
and [decision 0121](decisions/0121-parameter-gesture-events.md).

Graph renders support at most `ravet_graph::MAXIMUM_GRAPH_EVENTS` (1024) per
direction. Event queues can prepare 4096 slots independently; choose batches
that fit the receiving graph and selected native node profile. Oversized worker
profiles reject before node preparation.

Finite global `ParameterModulation` signals also travel through parameter graph
filters and complete event queues with stable identity/order. They do not change
saved base values by themselves or imply native node support. See
[the parameter guide](parameters.md#global-modulation-signals) and
[decision 0122](decisions/0122-global-parameter-modulation.md).

## Queued audio input for graph workers

`PreparedGraphOutput::with_input(graph, consumer, producer, config)` adds explicit
queued f32 input while preserving `new` for no-input graphs. Input/output ring
channels match the graph layout (1..64 each), auxiliary buses stay empty and
combined worker planar/interleaved scratch stays within 32 MiB. Input/output rates,
capture/publication policy and queue owners are explicit; no device or clock is
inferred. `pump_io` still carries bounded staged/generated events.

A fresh render dequeues a whole-frame prefix, zero-fills missing frames, and
reports `input_frames_consumed`/`input_underflow_frames`. Pending output publication
reports both as zero and neither consumes nor repeats new input/events. Malformed
outer event/output-storage contracts reject before dequeue. Graph faults may
consume input and advance native history; faulty output is suppressed and the
worker retains the error until `reset`, without draining further input. Reset
preserves queued/pending audio and input queue data; no automatic retry/replay.
Stop/join owners before queue clearing and main graph/native retirement.

The offline public gain probe checks queued stereo input, a half-block output
queue, exact retained samples, a starved block and owner retirement:

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-device-probe --bin graph-input --locked -- \
  'target/clap/release/RAVET Gain.clap'
```

It opens no device/GUI/DAW and does not establish physical capture, independent
clock alignment, latency/deadlines or recovery. See
[decision 0124](decisions/0124-buffered-graph-audio-input.md).

## Buffered input-clock conversion for graph workers

`PreparedClockedInput::new(raw_consumer, converted_producer, input_rate,
output_rate, options)` prepares the existing sinc/FIFO-clock conversion on an
explicit worker, publishing converted f32 frames into a graph-rate queue. Queue
channels match (1..64); nominal integer rates are 8000..192000 with existing sinc
ratio/correction limits. Clock capacity matches the raw ring and target is strictly
inside it. Fixed frames are 1..65536; its output block adds at most 16 MiB to
converter/internal/source/destination queue memory.

`pump()` converts a fresh block only after its prior suffix has published. Full
or partial destination queues retain whole frames and do not consume capture or
advance sinc/controller state again. Target-occupancy priming and underflow/reprime
follow the existing clocked-duplex policy; fresh converted blocks may contain
intentional zeros. Progress reports converted/published/pending frames, and
statistics expose independent converter callbacks/clock occupancy/correction.
These are not hardware timestamps, a latency measurement or drift stability
proof. Capture/thread pacing, nominal rate selection, graph/event scheduling,
stream shutdown/join and endpoint retirement stay explicit.

The loaded public gain probe can insert nominal 96k-to-48k conversion before its
queued native effect graph:

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-device-probe --bin graph-input --locked -- \
  --clocked 'target/clap/release/RAVET Gain.clap'
```

This synthetic offline profile checks settled stereo levels, retained suffixes,
1000 graph blocks and no heap actions; it opens no physical device/GUI/DAW. See
[decision 0125](decisions/0125-buffered-clocked-input-worker.md).

## Live replacement in buffered graph workers

`PreparedDynamicGraphOutput::new(engine, producer, config)` and `with_input`
share fixed-worker scratch, event, queue, starvation and fault-latch semantics.
The engine must already be prepared at the exact config returned by
`DynamicGraph::configuration()`; no graph/node is reprepared. Retain its separate
control endpoint for prepared offers and main-thread retirement.

A pending output suffix repeats no processing or handoff. Only fresh rendering
attempts one candidate; `progress.swap` returns its outcome separately from
processing/publication, with None for pending calls and fixed workers. Retirement
full or candidate start rejection preserves old processing. Copied queued audio
keeps its old graph result; no crossfade, tail drain, state migration or rate/
layout/latency change is inferred. Graph/event/GUI generations remain caller policy.

A swap can precede a processing failure. `session.last_swap()` retains the last
graph-processing handoff outcome even on errors; it is historical through pending,
outer-rejected and fault-latched calls. Successful current calls use progress.swap.
Faults suppress faulty publication and require explicit reset before more rendering.
Worker finish/stop/join precedes `into_graph` on main, engine/retired graph drop and
separate native control reclaim. Each in-flight graph/native budget is additional
to shared worker scratch/queues.

The public offline two-instance gain probe replaces a graph while old audio is
partially published, checks exact old/new samples and main retirement while the
new graph continues:

```sh
CARGO_BUILD_JOBS=2 cargo run --release -p ravet-device-probe --bin graph-live --locked -- \
  'target/clap/release/RAVET Gain.clap'
```

It opens no device/GUI/DAW and establishes no physical timing/deadline/recovery
claim. See [decision 0126](decisions/0126-buffered-live-graph-workers.md).
