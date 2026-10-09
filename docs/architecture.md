# RAVET architecture

RAVET keeps portable processing independent of plug-in formats, audio devices,
and editors. The gain reference product supplies one processor implementation;
the offline example and CLAP prototype exercise it through different boundaries.
The [catalogue](FEATURES.md#workspace-packages) lists the current Cargo packages.

## Crate boundaries

`ravet-foundation` owns the processor lifecycle, validated preparation limits,
borrowed planar audio, events, transport types, and validated borrowed parameter
schemas with fixed-size complete state and explicit migration. `ravet-signal` owns reusable
numerical components: a linear smoother, prepared double-precision delay and
feedback diffusion, fixed streaming 2x half-band conversion, and caller-owned
quadrature rotation, stable biquad filters, prepared complex FFT/FIR convolution
and selectable 1x/2x/4x/8x oversampling, phase oscillators, peak compression
and arbitrary-ratio sinc conversion. See [signal processing](signal-processing.md). Neither requires
a host or platform backend. `ravet-surface` supplies portable parameter attachments and contains an independently built
macOS Cocoa/WebKit native backend. `ravet` re-exports these three modules.
The explicit `ravet-webview` crate owns its Cargo/FFI binding, and is not part
of the umbrella graph.

`ravet-gain` implements the gain product using those portable contracts.
`ravet-clap` translates Clack's safe host audio and event views into that
contract. `ravet-gain-clap` owns plug-in metadata, ports, parameters, threading,
and the exported CLAP entry. `ravet-clap-smoke` is a development loader and host.
The umbrella crate does not re-export the CLAP adapter.

Proprietary consumer products live in separate private sibling projects and
are not workspace members. Only general reusable components enter the public
framework; see [publication policy](publication.md) and
[prepared DSP decision](decisions/0004-prepared-dsp-primitives.md).

Portable DSP runtime dependencies remain local workspace crates. The explicit
`ravet-data` package adds bounded XML, hashing, and legacy integer verification
through pinned quick-xml, sha2, and num-bigint dependencies; these allocating
services stay on control/worker threads. See [control data](control-data.md)
and [native editor](native-editor.md) for the new boundaries. The safe CLAP
bridge depends on Clack, while the export and smoke host isolate narrowly
allowed unsafe ABI operations. See the
[CLAP decision](decisions/0002-clap-backend.md#dependency-boundary) for dependencies
and foreign-pointer responsibilities.

The explicit `ravet-state` crate owns allocating transactional trees, undo/redo,
revision subscriptions and complete preset records on control threads.
`ravet-audio-file` owns bounded WAV/AIFF worker I/O and optional compressed clips and prepared SPSC audio handoff;
file methods never run in callbacks. `ravet-instrument` adds portable fixed-capacity
voice ownership and configured per-note expression, leaving DSP and device/format
exports to consumers. The explicit `ravet-graph` schedules owned portable nodes and prepared latency
routes; its distinct fault status exposes partial node progress rather than
claiming whole-graph DSP rollback. These crates are not implicitly re-exported
by the umbrella.
Native format reference wrappers share `ravet-gain-native` DSP through an explicit
ABI; their SDKs stay external. See [observable state](observable-state.md),
[audio files](audio-files.md), [MIDI](midi.md), [VST3](vst3.md),
[AudioUnit](audio-unit.md), [AAX](aax.md) and [LV2](lv2.md).

## Processor ownership and lifecycle

The processing thread exclusively owns mutable DSP state. Preparation validates
sample rate, channel layout, maximum frames, and event capacity, and may allocate
storage. Stop callbacks before preparing again or deactivating.

The lifecycle is create, prepare, process or reset, then deactivate. A failed
gain preparation leaves the processor inactive. Reset preserves preparation and
requested parameter values while snapping smoothing to the latest request.
Deactivation preserves that request for the next activation.

The reference gain accepts mono/stereo effects. Foundation also represents
no-input instruments and up to 64 channels per direction, plus eight auxiliary buses; contract tests exercise those boundaries,
but there is no instrument product or CLAP instrument adapter.

## Audio and event contract

Audio is borrowed planar `f32` or `f64`, specialized per processor, on one main bus and up to eight auxiliary buses. Separate-buffer views borrow immutable
inputs and mutable outputs; in-place views borrow mutable channels. Constructors
validate channel lengths, and accessors expose only active frames. Zero frames
are legal; unused backing tails remain untouched.

Input events are bounded by prepared capacity and ordered by nondecreasing frame
offset. Same-frame events retain input order and take effect before that sample.
Nonempty blocks require offsets below the frame count. Empty blocks allow only
offset zero and apply events without advancing DSP time.

Processors validate the complete block before applying events or changing DSP
state. A fault rejects the block, silences all active bus outputs and discards emitted events. Caller-owned output event storage has its own prepared capacity. These obligations
are documented by [Processor](../crates/ravet-foundation/src/processor.rs) and
[decision 0001](decisions/0001-portable-processing.md); types alone do not enforce
an implementation's real-time behavior.

## CLAP translation

Activation preallocates scratch audio and event storage. Callbacks copy host
inputs into scratch, run the gain processor in place, and copy results to host
outputs, allowing separate, in-place, and mixed host channel pairs. This costs
two sample copies per channel.

The bridge caps preparation at 65,536 frames and translation at 1,024 input
events, including ignored event kinds. Only global core parameter-value events
are translated. Validated block-start transport position/tempo/playback context is forwarded, with seek detection. Notes, modulation, output events, extra buses and `f64` processing remain outside this prototype. See [extended processing](extended-processing.md).

The main thread reads requested gain, boolean bypass and a 31-bit restore
generation from one atomic 64-bit word. Complete state load and inactive host
flush publish a new generation; audio synchronizes at callback boundaries and
reflects values using bounded conditional publication. State load explicitly
migrates legacy gain-only records. See [state protocol](session-state.md).
Foundation's [word mailbox](snapshots.md) remains available for arbitrary complete
multi-parameter publications with caller-owned encodings and contention policy.

The gain export has a separate [editor control bridge](editor-control.md): one
main-thread producer queues up to 32 complete edits or live gesture commands, and the serialized CLAP
consumer emits host begin/value/end while retaining output backpressure progress.
Restores invalidate older edits; accepted editor changes precede host automation
and are reflected independently of malformed host streams. A separate coherent
meter mailbox receives successful audio blocks. Fault diagnostics remain atomic,
without callback logging. A default optional macOS GUI uses this bridge through main-thread JSON;
`ravet-editor-smoke` owns a native parent/main loop to exercise the built bundle.

## Real-time evidence

Processing implementations must avoid allocation, deallocation, blocking, and
I/O. Allocation-counter tests cover the gain processor, CLAP bridge, and host ABI,
including reset and malformed inputs, on the measured processing thread.

These tests do not establish callback deadlines, native allocation behavior,
device latency, or DAW compatibility. [Work status](WORK_STATUS.md#validation)
records the executed workloads and environment. Future editor, broader state, and device
integration must preserve the processor's ownership and callback obligations.

## Portable parameter and meter layer

The [parameter guide](parameters.md) describes schema validation, complete
versioned records and explicit migration. Portable gain adds bypass and complete
gain/bypass restore; CLAP exposes both parameters and complete session recall. Signal provides peak/RMS accumulators; gain measures successful
nonempty blocks and offers a separate coherent meter mailbox. Audio owns restore
and publication timing, while the embedded editor owns display scheduling. Current meter bars have
no decay or peak retention; fuller display ballistics remain future work.
See [decision 0010](decisions/0010-parameter-state-and-meters.md).


## Native C++ editor backends

The separate [native editor boundary](native-editor.md) provides Cocoa/WebKit,
Windows HWND/WebView2 and Linux GTK3 X11/WebKitGTK prototypes. Linux format
consumers own a UI-loop timer and call the bounded GLib pump. Windows consumers
own an STA message loop and deploy the matching loader plus runtime prerequisite.
These C++ backends do not add browser dependencies to the portable graph or
extend the macOS-only Rust binding/CLAP GUI to other platforms. Product wrappers,
assets, licensing fixtures and format SDKs remain outside the public repository.
See [decision 0014](decisions/0014-cross-platform-native-web.md).

## Standalone device boundary

The explicit `ravet-device` crate adds CPAL outside the portable umbrella. Control
threads enumerate/select/configure output devices and prepare output-only f32
processors. Callbacks use preallocated planar scratch and partition variable
interleaved buffers into bounded blocks. `ravet-device-probe` validates silent
output callbacks. Input/duplex, MIDI and recovery remain pending; see
[audio devices](audio-devices.md) and [decision 0018](decisions/0018-output-devices.md).

## Control-thread runtime boundary

Optional `ravet-runtime` owns bounded files/HTTPS and background workers/timers
in safe Rust. `ravet-runtime-ffi` provides opaque handles, buffers, callbacks and
narrow OS queries. C++ facades translate ownership only. Product authorization
and storage/payload policy remain private; see [runtime services](runtime-services.md)
and [decision 0020](decisions/0020-rust-runtime-services.md). Surface dialogs are
OS UI shims. These services add no networking/thread dependencies to portable
processing and must stay outside audio callbacks.

## Native format reference boundary

`ravet-gain-native` exposes a narrow static-library ABI for the public reference
DSP/state codec. Native VST3 processor/controller classes compile against an
external SDK with preallocated scratch and bounded queue translation. Portable
crates acquire no SDK or pointer dependency. This is a gain prototype; generic
format/editor mappings remain pending. See [VST3](vst3.md).

## Optional native hosting boundary

`ravet-host` discovers explicit bounded CLAP candidates without executing them,
then loads a caller-selected trusted library through an unsafe native-code
boundary. It owns bounded byte-preserving metadata and exposes Clack's retained
entry for consumer-owned instance/callback/audio/state policy. A main-thread borrowing guard owns negotiated GUI lifecycle
with explicit native parent lifetime. It supplies
no automatic system scan, sandbox or device dependency to portable processing.
See [plug-in hosting](plugin-hosting.md).

Explicit graph audio sessions may bracket processing with optional
`Processor::start_audio`/`stop_audio` hooks. Their defaults are no-ops; existing
adapters retain ordinary portable behavior. Owner-confined borrowed graph sessions
start/stop nodes on audio and return stopped graphs for control deactivation.
See [graph sessions](audio-graphs.md#explicit-audio-owner-sessions) for rollback,
retirement and native integration limits.

Native/backend runtime `BackendFailure` clears outputs/events while acknowledging
that native histories may have advanced. This differs from validation rejection
before mutation; consumers stop/reset/rebuild under their session policy. Hosted
graph nodes use that explicit runtime fault contract. Their prepared f32/f64
scratch matches the native sample format; f64 requires advertised native port
support before activation, without precision conversion. Explicit packed-port
factories preserve native enumeration boundaries while graph routes use summed
main channel indices; portable auxiliary pairing is not inferred. Block-start
transport is separately enabled while stopped on main; unrepresentable known
context rejects before native calls with `InvalidTransport`. Separate parameter
input factories cache validated writable metadata/identities and fixed event slots
on main; audio processes exact offsets or flushes zero-frame input without
advancing native process time. The preparation builder also selects bounded global
parameter output, including read-only telemetry: checked mapped output publishes
through graph event routes, while runtime output faults clear the block without
rolling back native history. Other native event kinds remain separate mappings.

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
