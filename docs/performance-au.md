# AUv2 performance instrument reference

The public macOS AUv2 instrument embeds the public native performance sine VST3
reference and external Steinberg/Apple wrappers. Its RAVET-owned MusicDevice
adapter supplies bounded validated note scheduling, exact complete Level state
preflight and supported-buffer silence on rejected rendering. External SDK sources
stay outside RAVET. This is a local arm64 prototype with in-process host evidence.
Its schema/list/info, normalized controls, physical text and state validation use
the reusable [native AU helpers](native-au.md).
Its Render path also consumes their prepared planar audio mapper before SDK
parameter transfer and native processing. Its reusable AU block-start transport
collector also validates actual host callback data before native processing; the
VST3 bridge passes no context when playing/sample availability is unknown.

```sh
CARGO_BUILD_JOBS=2 python3 scripts/bundle-au.py \
  --vst3-sdk /path/to/vst3sdk --audio-unit-sdk /path/to/AudioUnitSDK \
  --plugin performance
```

The shared build pipeline compiles/validates the selected public VST3 reference,
builds the external AudioUnitSDK with two workers and creates
`target/au-performance/release/AU/ravet_performance_au.component`. The complete
component includes `Contents/Resources/plugin.vst3`, applicable Apple/Steinberg/
RAVET licence notices and a local ad hoc signature. Both component and embedded
module must travel together. No normal host directory is installed or changed.
Gain remains the default `--plugin gain`; its editor/program flags apply only to
Gain. [au_bundle.py](../scripts/au_bundle.py) shares the selected VST3 runtime and
keeps AU SDK/component packaging and validation in one focused layer.

The component identity is instrument `aumu`, subtype `RvPS`, manufacturer `Ravt`.
It has zero audio inputs, mono/stereo noninterleaved f32 output, 16 native voices
and normalized Level parameter ID 1 spanning 0..1 with default 0.2. The output
profile and native DSP bounds come from the public
[performance reference](../examples/performance-native/README.md). Voices, phase,
envelopes and transient expression are not persisted. Stream-format configuration
rejects f64, interleaved stereo and nonfinite sample rates; maximum-frames
configuration accepts 1..65,536 and rejects invalid values before SDK allocation
or capacity mutation.

Legacy `MusicDeviceMIDIEvent` NoteOn/NoteOff callbacks preserve channel/key and
exact widened f32 velocity fractions `byte/127`. NoteOn velocity 0 means MIDI
NoteOff. Ordinary MIDI notes use absent native identity, allowing the same key on
different channels and duplicate addresses; Off scopes channel+key. Other valid
short MIDI is ignored. Full-width API status/data words are validated before SDK
byte narrowing: running status, values outside their byte/7-bit ranges, undefined
system statuses and nonzero unused short-message arguments reject atomically.
Defined short system messages are ignored; SysEx start/end statuses reject.
Pedals, pitch bend, MIDI2/EventList, SysEx and native note
expression input are outside this profile. It does not advertise a MIDI2 protocol.
Callbacks must be serialized by the host; this reference makes no SDK allocation,
locking or callback deadline promise.

The AU StartNote/StopNote API supplies explicit opaque IDs. StartNote accepts
instrument 0 or `kMusicNoteEvent_UseGroupInstrument`, groups 0..15 and exactly two
note parameters: finite pitch 0..<128 and
velocity 0..127. Fractional pitch produces initial tuning in cents, and floating
velocity is not quantized to MIDI bytes. Zero StartNote velocity retains native
note ownership. IDs encode the monotonically increasing generation, group and
key, without truncation during StopNote. At most 1,048,575 generations are issued
per component lifetime; exhaustion rejects rather than wrapping or reusing a live
ID. Wrong group, unissued generation, malformed input and failed enqueue preserve
the caller's ID output. This bounded reference policy does not establish a generic
AU note-identity allocator.

Pending note events must have nondecreasing offsets below 65,536. Equal-offset
order is preserved. The expanded native event budget is 1,024, with one slot
reserved for possible complete-state recall; queued parameter operations share
the remaining budget. Ordinary unchanged-control batches permit 1,023 notes.
Fractional StartNote tuning uses a second event slot. Rejected callbacks retain
previous accepted queued operations. A queued offset outside the next active
slice rejects that slice before DSP and discards the note queue. SDK parameter
intent remains subject to its own transfer policy. Successful zero-frame delivery
requires offset 0. Reset clears queued notes and voice history, preserving Level;
opaque ID generations are not recycled. AU parameter setters reject unknown IDs,
nonfinite/out-of-range Level and exhausted event capacity before SDK mutation.

State is the complete canonical native `RAVETPS\0`, version 1, Level schema.
Save makes the processor's Level authoritative and writes matching processor and
controller streams, avoiding a stale asynchronous controller cache. Restore
validates both native streams and AU dictionary field types and component
identity values before calling the SDK. Malformed product/version/length/values/
types preserve live state. Both
ClassInfo preset and ClassInfoFromDocument restore paths use this preflight.
There is no effect bypass parameter/property or factory-program catalogue.

An owned render dispatch calls AUBase and checks native processor status. On
failure it clears valid exact mono/stereo caller buffer prefixes up to the smaller
of the request, prepared maximum and 65,536 frames; invalid/unaligned ranges are
skipped, and backing tails survive. This includes AUBase rejecting an oversized
slice before virtual DSP. The silence flag is set only when all requested samples
in valid supported buffers were cleared. Host pointers must be live and cover
byte counts. The dispatch accesses the SDK's documented component instance
storage; it does not replace or modify SDK sources.

Actual bundled-component tests load/register the factory only inside the probe
process, then exercise mono/stereo 48kHz output, timed analytic 440Hz notes/velocity/
attack, same-key channel isolation, fractional identified notes, same-frame MIDI
zero ordering, sample-timed Level, coherent state/default/native preflight,
preparation/format rejection without capacity mutation, malformed AU dictionary
identity/controller-type rejection, capacity rejection, invalid-offset recovery,
oversized-prefix silence/tails, typed initialized host callbacks, State2/legacy
precedence, malformed sample/signature rejection before queued MIDI/Level
consumption and exact negative sample-boundary recovery,
SysEx rejection and 200 synthetic render/release cycles. The nested VST3 module
also passes its own contracts and SDK validator. Boundary tests verify profile
selection, packaging ownership, preserved Gain options and failure propagation.
This establishes no REAPER/AUv2 discovery or session recall, auval, sandbox,
physical-device, other-architecture/platform, UI/accessibility or production
support. See [decision 0174](decisions/0174-auv2-native-performance-instrument.md).
