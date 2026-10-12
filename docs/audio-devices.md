# Standalone audio devices

The explicit `ravet-device` crate adapts CPAL f32 output, input capture and
bounded duplex effect streams to portable processors. It is outside `ravet`'s
dependency graph. Explicit MIDI connections/handoff are provided separately by
`ravet-midi-device`; file worker adapters and prepared event/graph handoff have
their own guides and bounds. Recovery is caller-scheduled. Explicit complete-file
output-rate preparation and memory transport are available through FilePlaybackOutput
below. Automatic hotplug, device-clocked transport scheduling and physical recording
validation remain separate work.

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

## Sample-rate admission and retry

Every native output/input stream build uses shared control-thread rate admission.
After a successful build, the opener reads the device rate; a different value
causes the stream to be dropped before it can be returned or played. A query
failure also rejects the stream, as a backend error. Duplex openers recheck
input after output construction, since output can change a shared device clock.
Graph, file, recording and recovery paths inherit this check through these
openers; independently prepared offline renderers do not query devices.

`device_nominal_sample_rate(&cpal::Device) -> Result<u32, cpal::Error>` lets callers
choose the device's current rate before preparing a processor. On macOS it
resolves CPAL's public CoreAudio UID and directly reads
`kAudioDevicePropertyNominalSampleRate`. CPAL 0.18.2's `default_input_config` and
`default_output_config` read `kAudioDevicePropertyStreamFormat` instead; stream
ASBD rates can differ from the nominal device clock. The helper opens no stream
and changes no settings. Other backends provide only a CPAL default-config rate
proxy, using the opening direction (the public helper prefers output, then input).
This is not a measured clock or proof of negotiation on those backends.

`OpenError`, `ClockedDuplexOpenError`, `ReclaimableOutputOpenError` and
`ReclaimableClockedOpenError` now include
`SampleRateMismatch { requested: u32, actual: u32 }`. Input failures in clocked
duplex retain `ClockedDuplexOpenError::Input(OpenError::SampleRateMismatch { .. })`
so callers can identify the affected direction. These provisional enums remain
exhaustive: consumers must update exhaustive matches for the new variant.
Reclaimable failures retain their existing pending callback-retirement ownership;
wait for that proof before reusing the original processor.

CPAL's explicit sample-rate update timeout/failure and listener-disconnection
messages map to the same mismatch error when the nominal query succeeds.
Unrelated errors, including unsupported formats and general device-busy errors,
remain backend errors; a failed nominal query preserves the original build error.
A timeout can report `requested == actual`: the rate is known but CPAL did not
return a usable stream. Retrying is a caller decision, never an implicit loop.
Choose `actual`, rebuild/reprepare the processor and open again explicitly.

This is an opening-time snapshot. Later device-rate changes, default-output
rerouting, virtual-device conversion, clock drift, and audible pitch correctness
still require runtime monitoring or target validation; no continuous guarantee
is established by the offline tests. See [decision 0277](decisions/0277-device-rate-admission.md).

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
or recover errors. Use the separate prepared worker/ring recording controllers
for WAV, AIFF, FLAC, Vorbis or MP3 described in [audio files](audio-files.md). Physical
recording and shutdown timing still require device validation; device-clocked transport
and record scheduling remain separate work.

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

## Prepared file output

Enable `ravet-device/resampled-playback` explicitly for `FilePlaybackOutput`, an
output-only processor owning the complete output-rate clip prepared by
`ravet-audio-file::prepare_file_playback`. Configure its exclusive memory transport
before moving it into `PreparedOutput`/`open_output`. Preparation rejects mismatched
rate/channel/event/auxiliary/render configurations before callbacks and allocates
bounded scratch; valid processing only renders memory and copies planar output.
Reset pauses at zero/configured loop start; deactivate retires processing while
preserving clip/cursor. No cross-thread transport mailbox is added. Stop callbacks
before recovering/dropping clip storage. Retained causal filter delay/tail, complete
file budgets and failure-owned reader semantics are documented in
[audio files](audio-files.md#prepared-file-playback-at-a-selected-output-rate).
Actual `PreparedOutput` tests establish variable-buffer/sample/heap/lifecycle
behavior; physical output, deadlines, live buffered conversion, drift and device
clocked transport still require their separate implementation/validation.

### Prepared buffered file output

The default-disabled `resampled-playback` feature also exposes
`BufferedPlaybackOutput`. It owns `BufferedPlayback` from a native or explicitly
converted `ReadAheadWorker` and implements the existing actual `PreparedOutput` /
`open_output` processor path with preallocated planar bridging. Prepare rejects
sample-rate, channel and render-capacity mismatches before rendering. Callback
processing renders the prepared unique consumer, updates atomic monitor counters
and copies samples; it performs no file read, conversion, allocation or wait.

Configure play/pause through the exclusively owned endpoint before handing the
processor to an output owner. Reset pauses while retaining queue/cursor; deactivate
and prepare retain transport. A directly owned stopped processor can reclaim its
playback endpoint and perform matching stopped worker seeks. `open_output` keeps
its processor in a native closure; this API does not reclaim that inaccessible
endpoint or promise live physical seek/recovery. `BufferedPlaybackOutputMonitor`
returns relaxed snapshots that may span adjacent callbacks; counters wrap and
count converted output frames. Worker status distinguishes Failed/Stopped from
verified EOF; ended requires EOF and complete queue drain. Source-worker failures
retain previously queued audio for one exact drain and preserve their original error in worker shutdown.

Prepared adapter evidence covers exact conversion/tail, arbitrary callback sizes,
allocation-free callback execution, paused consumption, failed-worker prefix,
configuration and exclusive reset/deactivate/reprepare/seek/reclaim. It does not
establish physical-device support, clock mapping/drift control, device scheduling,
a cross-thread command mailbox or a callback deadline.

### Explicit stopped native output recovery

`PreparedOutput::new_owned` returns the original processor on ordinary preparation
failure. It preflights configuration and fallibly allocates scratch before calling
`Processor::prepare`; a failing processor must satisfy the trait's inactive
contract. The existing `new` keeps its consuming failure signature.
`into_processor` consumes a directly owned stopped renderer on control, deactivates
once and returns its processor for explicit preparation/reopening. It performs no
hidden source seek, reset or read-ahead shutdown.

`open_reclaimable_output` adds a native f32 output owner with a dedicated control
retirement worker. `ReclaimableOutput::stop` drops the native stream on control and
returns `OutputRetirement`. Poll `try_reclaim` without waiting or explicitly block
with `wait` on control. The pending proof has no deadline or cancellation guarantee:
CPAL may destroy callback captures asynchronously. Ordinary stream/receiver Drop
also leaves the worker retaining framework captures until retirement is safe.

`ReclaimedOutput` distinguishes normal stopped ownership from a caught retirement
worker fault that still returns the original owner. Processing panics permanently
silence the data callback; recovered ownership does not restore correct DSP history.
Original opening errors are returned with either `FailedOutputOwner::Immediate`
(preflight/preparation/spawn) or `Pending` (backend ownership may remain). No error
is replaced by a successful owner or file EOF claim. A foreign deactivation panic
after callback retirement can lose owner recovery and reports an exceptional worker
error. Calls and joins belong to control; no automatic retry or recovery occurs.

The framework's data/error closure captures only unique guards and primitive
state. Each guard Drop publishes its retirement flag and releases a non-last Arc.
The worker extracts renderer/scratch/counters only after both Acquire flags and
sole strong ownership prove that callback borrows and Arc fields are gone. An early
receiver drop or a caught polling panic cannot move final framework destruction
onto a callback. This narrowly audited shared-cell access is an explicit unsafe
boundary; native CPAL/OS closure allocation, locking and destruction are separate
backend limits. Panic handlers/runtime costs are exceptional.

A recovered `BufferedPlaybackOutput` retains its original unique consumer. Stop
its native callbacks first, obtain the processor, then use the matching worker's
`seek_stopped` and explicitly prepare/reopen the recovered owner. Failed candidate
seeks preserve the prior cursor and queue. This ownership integration does not
provide a live seek mailbox, clock/drift correction, arbitrary endpoint reclamation,
physical-device support or a teardown/callback deadline.

The reusable silent probe opens the exact default f32 profile without channel or
sample-format fallback:

```sh
cargo run -p ravet-device-probe --bin reclaimable-output --locked
cargo run -p ravet-device-probe --bin reclaimable-output --features resampled-playback --locked -- --converted
```

The default mode checks two native silent stop/recover/reopen cycles. The explicit
converted mode uses a 257-frame zero-valued Float32 WAV at 44.1kHz in every device
channel, bounded Sinc16 conversion and an independent retained-output integer
count. It drains through the actual native callback, reclaims the unique file
consumer, checks a failed stopped seek, seeks to output frame17 and explicitly
reopens/drains again. It checks exact consumed output coordinates/EOF and original
worker errors. It rejects unsupported default layouts/rates/formats. Underflow is
reported without advancing the source cursor and is not a scheduling guarantee.
Five-second control observation windows are local probe limits; they do not
establish callback or teardown deadlines. These zero-PCM receipts establish
environment-specific ownership/conversion composition, not audible quality,
clock alignment/drift or broad physical-device support.

## Output-clock buffered file and captured-input processing

With `resampled-playback`, `BufferedFileDuplexProcessor<P>` owns a unique
`BufferedPlayback` and a downstream f32 `Processor`. It accepts captured input
that `PreparedClockedDuplex` has already converted into the output clock. Child
inputs are ordered file channels first, captured channels second; the child owns
its mixing/routing. Every file queue and child rate equals the output rate. Outer
input/output counts are 1..64 and file+capture inputs must fit 64, with zero events,
output events and auxiliary buses. Preparation reserves all scratch before child
preparation, caps its arrays at 32 MiB and respects the file render bound. Existing
file, converter, FIFO and allocator storage is additional. No default or dependency
changes.

The ordinary composition is `PreparedClockedDuplex::new(
BufferedFileDuplexProcessor::new(file, dsp), raw_capture_consumer, input_rate,
output_rate, output_channels, options)`. Existing `ClockedDuplexOptions`, sinc/FIFO
feedback, capture priming/underflow/reset and explicit recovery policies remain.
A file read-ahead queue has no independent clock; its disk/worker availability
never drives a drift estimator. File and capture starts are not sample-synchronized;
capture priming can supply zero input while file playback already advances.

Each valid nonzero block renders file PCM once and makes at most two downstream
calls: its consumed file prefix, then zero-file PCM while paused, underrunning or
finished. Live capture and DSP continue in both segments. Child transport carries
file queue output-frame positions and `playing=true` only for the consumed prefix,
with `playing=false` and a stationary file position on the suffix. Positions above
i64::MAX become unavailable in `Transport`; typed u64 counters remain. No original
decoder, beat/tempo or hardware timestamp is inferred. A fresh preparation and
observed stopped position jump mark the next child call discontinuous; zero blocks
preserve that indication.

`statistics` and the relaxed `monitor` distinguish file consumption/position,
file underruns/worker state/verified drained EOF, logical device-render frames and
child calls/faults. Device frames include valid fault-silenced/poisoned callbacks;
malformed outer blocks reject before source consumption or child execution. Monitor
fields can span adjacent callbacks and counters wrap. File EOF is not downstream
DSP-tail or audible completion. Child latency/tail exclude worker, converter and
FIFO buffering. A failed source drains its queued prefix, leaves `ended=false` and
allows capture/DSP to continue; its original error belongs to worker shutdown.

A downstream fault silences the complete outer block, retains its original error
in `first_error` and returns outer `BackendFailure` because file consumption or an
earlier child's history may already have advanced. Poisoned retries consume no
more file PCM and make no child call until reset or successful preparation. Reset
pauses file playback without rewinding cursor/queue, resets the child and clears
poison. Preparation alone does not reset or consume file PCM. The directly owned
stopped wrapper's `into_parts` deactivates its prepared child once and returns
original playback/DSP owners; it performs no drain, seek or worker join. Optional
audio-owner hooks forward the existing caller-managed session contract. Foreign
lifecycle panics remain exceptional and do not imply a successful ownership receipt.

A native clocked stream still hides its consumer. This adapter does not create a
live seek mailbox, reclaim that native closure or migrate old file/DSP state.
Control recovery factories must explicitly create fresh worker/queue/DSP histories
and select restored file state; ordinary child faults require caller reset/reopen
rather than automatic recovery. Physical callback teardown, timestamp origins,
independent-clock alignment, long-session drift, latency and deadline evidence
remain separate. Scoped acceptance uses actual PCM/indexed workers and the actual
prepared clocked duplex consumer with simulated ±500 ppm capture schedules, not
a new physical-device validation claim.

## Reclaimable paired clocked streams and original-owner recovery

`open_reclaimable_clocked_duplex` adds an explicit f32 input/output pair with a
control retirement worker. Its nominal rates, corrected sinc/FIFO profile,
channel/ring/frame bounds and initial capture priming are the existing clocked
profile. No device/rate/format fallback or default changes. Opening retains the
original processor on ordinary preflight/preparation/spawn errors. A native build
failure may have handed callback owners to the backend and returns pending
ownership with the original error. `PreparedDuplex::new_owned` and
`PreparedClockedDuplex::new_owned` likewise retain the original processor/raw
capture consumer on normal failure; their legacy `new` signatures are preserved.
Fallible scratch reserves precede child preparation; failed child prepare must
leave it inactive. Foreign lifecycle panics remain exceptional.

A unique input data guard borrows the prepared capture field and a unique output
data guard borrows the disjoint prepared clocked renderer field. Two error guards
access atomic telemetry. All captures retain a preallocated shared cell; the
control worker retains its permanent owner. Guard Drop only publishes its Release
flag and drops a non-last reference. Reclamation requires all **four** callback
retirement flags acquired and stable sole strong ownership, including actual guard
field destruction. It does not infer retirement from pause, stream Drop returning,
flags alone or a retirement receiver being dropped early. No producer, consumer,
DSP, scratch or counter storage is disposed by a framework callback capture.
Native CPAL/OS callback/destructor internals retain their separate limits.

`stop` drops both native streams on control and returns `ClockedDuplexRetirement`.
`try_reclaim` never waits; `wait` is explicitly blocking control work without a
shutdown deadline. Dropping the receiver detaches the worker, which retains both
prepared owners until proof and disposes the result there if nobody receives it.
Normal completion and caught worker-poll fault are distinct outcomes, each with
exclusive stopped ownership and input/output/clock/panic statistics. Processing
panic permanently disables that data guard and makes histories exceptional; no
valid-history or audible-delivery guarantee follows.

The stopped wrapper exposes no rendering or mutable processor. `into_parts`
deactivates the original prepared processor once and returns it together with the
original matching raw consumer and prepared capture producer. Transient converted
queue/scratch and prefetched capture are disposed on control; actual remaining
queued raw frames retain their position. No hidden reset, seek, worker join, source
rollback or migration occurs. The recovered W215 wrapper can be consumed into its
original buffered file and child processor, then used for stopped failed/successful
indexed seeks and explicit fresh preparation. Original file counts/generation
remain authoritative. A live seek mailbox remains a separate capability.

`RecoveringReclaimableClockedDuplex` is an explicit caller-serviced selected-ID
coordinator over `Owned`, `Live`, `Retiring` and `Disabled` ownership states. Fault,
removal, `request_reopen` or disable stops both streams. Pending and disabled
service only polls retirement and never opens or invokes restoration. Known live
backend/capture/output panic faults retire before ID enumeration; repeated enumeration
errors cannot keep a known faulted pair live. A healthy pair retains its owners and
returns the original enumeration error without treating failure as device absence. After proof,
the original processor and stopped receipt are retained; the next available
service invokes the caller restoration hook before fresh preparation. A normal
hook error keeps the same, possibly advanced/mutated processor and receipt for
explicit retry. A successful hook consumes that receipt once; later ordinary open
failure retains immediate or pending ownership without rerunning successful
restoration. Start failure returns its original error and enters retirement.
Successful started opens advance generation; failed attempts do not. Disable may
still poll old retirement into a retained owner, and re-enable resumes the explicit
sequence. `into_owner` returns the original processor or pending proof, never
extracting from live callbacks. Exceptional failed retirement is `Lost`, with no
false ownership receipt.

Restoration belongs to the caller: reset/rebuild file/DSP histories, select a
stopped seek and decide how to handle worker-poll or processing panic evidence.
Device recovery watches backend/panic telemetry, while ordinary child DSP faults
keep their existing explicit reset/reopen policy. No background polling timer,
hotplug subscription, automatic fallback/state migration, synchronized stream
starts, timestamp alignment, latency, long-session drift or native deadline claim.
Focused acceptance includes guarded callback composition, synthetic asynchronous
backend/recovery behavior and two short silent paired CoreAudio cycles on one USB
interface. These runs establish original-owner retirement and reopening only.
Use `cargo run -p ravet-device-probe --bin reclaimable-clocked-duplex -- --list`
for read-only inventory. The explicit `--validate-silent INPUT_ID OUTPUT_ID` mode
opens the selected pair for two short silent cycles; it has no format/rate fallback.
