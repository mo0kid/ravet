# Portable MIDI short messages

Start with the [small MIDI synth, MPE and MIDI 2.0 examples](examples.md) for
runnable walkthroughs of the APIs described here.

Foundation provides validated `MidiMessage` values and a nonallocating
`MidiDecoder`. The API covers MIDI 1.0 channel voice messages, defined system
common short messages and real-time messages. It does not implement SysEx payload
storage, MIDI files or device/plug-in delivery. UMP framing is supplied separately.
Configured per-note voice state is supplied separately below.

## Explicit device connections and packet handoff

The separate `ravet-midi-device` crate uses pinned MIT `midir` 0.11.0 for native
input/output enumeration and connections. Select an exact current backend ID;
there is no default fallback, automatic reconnect or hotplug subscription.
Names can duplicate and IDs can change. Connection creation/teardown and output
`send` belong to control/worker threads. See the dependency's
[connection documentation](https://docs.rs/midir/0.11.0/midir/struct.MidiInput.html).

`PreparedMidiInput` and its unique `MidiReceiver` share a prepared power-of-two
ring of 1..1,024 packets. Complete explicit-status short messages and complete
SysEx packets up to 1,024 bytes including F0/F7 are validated before publication.
SysEx payload bytes are seven-bit; fragmented data, embedded real-time messages,
running-status packets and UMP reject. A full ring drops the newest whole packet.
Relaxed counters report accepted, full drops and invalid input. Consumer `pop`
returns an owned fixed-storage packet preserving FIFO order and raw connection
timestamps. Timestamp units are microseconds with a connection-local arbitrary
origin; consumers must map them to audio blocks explicitly.

Framework capture/pop use atomic prepared storage without allocation, locks or
I/O. Native backend callback allocation, scheduling and output-send behavior are
outside this guarantee. Creation/drop allocate and close can synchronize threads.
No audio callback should enumerate, connect, close or call native output send.
Complete SysEx support here does not change Foundation's short-message decoder
or add variable payloads to portable sample events.

The explicit Unix virtual-endpoint fixture creates a unique temporary destination
and sends only to itself; it does not open physical MIDI hardware:

```sh
CARGO_BUILD_JOBS=2 cargo run -p ravet-midi-device --example loopback --locked
```

On macOS arm64, actual CoreMIDI virtual input/output transferred note, controller,
bend, clock and complete SysEx messages in order, then closed both endpoints.
Tests cover malformed/full queues, maximum payloads, counter rollover, 10,000
threaded transfers and 10,000 allocation-instrumented callback/consumer cycles.
No physical hardware, Linux/Windows runtime, UMP or deadline is validated. See
[decision 0032](decisions/0032-midi-device-packets.md).

`MidiMessage::from_bytes` requires explicit status, exact length and seven-bit
data bytes. `as_bytes` preserves wire representation. `kind` returns zero-based
channels, raw seven-bit controller/pressure/velocity values and fourteen-bit
pitch bend/song position. Pitch bend center is 8192; its semitone range belongs
to the receiver. Note-on with velocity zero decodes as note-off while retaining
the original bytes. No MIDI controller policy or synthesis is imposed.

The decoder accepts one byte at a time, preserving channel running status and
partial messages across calls. Real-time messages emit immediately without
interrupting the partial message. System common cancels running status; a new
non-real-time status discards incomplete data. Undefined real-time status rejects
without disturbing running status. Invalid non-real-time status clears it.
Explicit reset clears all partial/running state when a port disconnects.

SysEx start reports `UnsupportedSysEx` once, then skips payload until EOX or
another non-real-time status. Real-time messages within it still emit. EOX is
reported as a short message. This avoids allocating unbounded SysEx data and
allows a caller to recover; it is not SysEx support. Data without running/partial
status reports `MissingStatus`.

`EventData::Midi(message)` carries a validated message at the event's sample
offset. Input/output capacity, ordering and zero-frame rules are unchanged.
Device adapters must supply timestamps and map them into blocks; the decoder
has no clock or scheduling policy. The current gain processor ignores MIDI, and
the current CLAP translator still accepts only global parameter-value events.

Tests cover all channel/status combinations, resolution endpoints, running status,
interleaved real-time messages, common-message cancellation, malformed messages,
SysEx skip/recovery and timed output preservation. Message codes follow the
[MIDI Association message summary](https://midi.org/summary-of-midi-1-0-messages).

## Voice ownership and configured per-note expression

The explicit portable `ravet-instrument` crate provides `VoiceAllocator<N>` with
1..128 fixed slots. It accepts validated short messages, retains channel/key,
velocity/release velocity, held/sustained/released state and expression snapshots,
and returns a changed-slot bitmask plus optional started/stolen voice. The DSP
owner ends release tails with `finish`; there is no hidden oscillator or envelope.

Polyphonic duplicate notes occupy separate slots; note-off releases the oldest
held match. Stealing prefers idle, oldest released, oldest sustained, then oldest
held. Age rollover renumbers with bounded stack work, preserving order. Sustain
and sostenuto thresholds are 64. CC120 kills immediately; CC123 releases held
notes respecting both pedals; CC121 clears relevant controllers and releases sustained notes. Released
tails freeze expression. Poly key pressure overrides channel pressure until a
new note/controller reset. Unimplemented message semantics are ignored.

`MpeZone` explicitly selects a lower or upper single zone with 1..15 members.
Zero-based master is 0 or 15. Master notes and notes outside the zone are ignored.
Member bend/pressure/CC74 timbre update per-note expression; master bend is additive
and master sustain/sostenuto affect the zone. Bend ranges are explicit finite 0..96
semitones (typical member 48/master 2); endpoints map to exact bipolar ranges.
Channel reuse releases earlier held/sustained notes and freezes their expression,
allowing a new note while old release DSP tails finish. This is explicit receiver
policy, not complete MPE conformance. The static allocator ignores RPN. Opt-in single-zone negotiation is described
below; overlapping zones and MIDI-CI remain pending. Separate master pressure/timbre
snapshots are available; the DSP owner chooses their combination with members. The
[MIDI Association overview](https://midi.org/midi-polyphonic-expression-mpe-specification-adopted)
describes the master/member distinction; the supplied static configuration
must match the sender.

Tests exercise duplicate-note ownership, sustain/release, priority stealing,
pressure retention, frozen tails, lower/upper filtering, master/member bend,
channel reuse, reset and age rollover. 10000 prepared workloads and the rollover
fixture count no Rust allocation/deallocation. No controller hardware, synthesis
product or instrument-format export is validated. See
[decision 0027](decisions/0027-midi-voice-ownership.md).

## Opt-in single-zone RPN negotiation

`RpnDecoder` tracks selectors independently on all 16 channels and emits absolute
Data Entry updates. CC101/100 select an RPN in either order; null 127/127, NRPN
selection, CC121 and System Reset cancel pending data. Selection changes discard
old data; CC38 requires a preceding CC6. Each CC6 infers zero fractional data.
CC96/97 increments and NRPN values are not decoded.

`NegotiatedVoices<N>` explicitly chooses lower or upper zone and starts in
polyphonic mode with two-semitone bends. RPN6 CC6 on that side's master enables
1..15 members; zero disables. Invalid channels/counts leave configuration and
voices unchanged. Each accepted zone message clears all voices, including tails,
and controllers, reports occupied slots to the DSP owner, and restores 48/2
member/master bend ranges (2/2 when disabled). Repeated zone messages also reset.

While enabled, RPN0 CC6 values 0..96 adjust the master independently or apply the
last member range to every member. Held/sustained snapshots refresh immediately;
released tails retain their previous expression. Fractional CC38 ranges and
polyphonic RPN0 are ignored. Reset clears notes/controllers/selectors but retains
negotiated configuration. Consumers apply returned masks to their DSP slots;
these APIs do not generate sound or communicate with a controller themselves.

The chosen wire behavior follows the [MIDI Association MPE specification](https://midi.org/mpe-midi-polyphonic-expression),
sections 2.1 and 2.4; single-zone support remains a scoped receiver policy rather
than complete MPE conformance. Master pressure and CC74 snapshots refresh all
held/sustained members, remain distinct from member values and freeze on release. Tests cover both sides, live bend changes,
frozen tails, invalid/null/NRPN cancellation and 10,000 allocation-free negotiations.
See [decision 0037](decisions/0037-single-zone-negotiation.md).


## Portable sine instrument

`SineInstrument<N>` connects voice ownership, chosen-side negotiation and linear
ADSR to a no-input mono/stereo f32 processor. Prepare a finite positive sample rate with ADSR counts fitting u32, at most 65,536
frames and 1,024 events, with no auxiliary/output-event buses. Complete block
validation precedes every event/state change; faults silence active outputs.
Zero-frame events apply without advancing phase/envelopes. New or stolen notes
reset phase/envelope; live pitch bend retunes while retaining phase. Sustain
holds the envelope; released voices finish after their 50 ms release and return
the slot. The fixed envelope uses 5 ms attack, 10 ms decay and 0.8 sustain.

Mix level has stable ID `INSTRUMENT_LEVEL_ID` (1), defaults to 0.2 and clamps to
0..1. Mix is scaled by level/N; stereo duplicates mono. Normalized portable note
velocities quantize to seven bits (positive values produce at least one); MIDI
messages preserve raw velocity. Master/member bend adds before frequency
conversion, clamped to 49% of sample rate. Pressure/timbre snapshots do not
modulate the sine. Unknown parameters and transport are ignored. Held/sustained
notes can sound indefinitely, so declared tail is infinite. Reset/reprepare
clear voices/history while retaining requested level and negotiated zone/ranges.

Processing/reset use no heap, blocking or I/O. Drop/deactivate/prepare require
stopped callback ownership. This is a portable reference; no editor, complete
state codec, f64, plug-in instrument adapter, device deadline, click-free stealing
or bandlimited waveform bank is claimed. The executable example renders a
synthetic timed note and release into memory without opening a device:

```sh
CARGO_BUILD_JOBS=2 cargo run -p ravet-instrument --example synthesis --locked
```

Tests compare attack samples against an independent sine/ramp formula, prove
identical split-block output, complete release/sustain/stealing, rejected-block
state preservation, stereo tails and 10,000 heap-free synthesis workloads. See
[decision 0039](decisions/0039-portable-sine-instrument.md).

## Universal MIDI Packet framing

`UmpPacket` owns up to four numeric words and validates the message-type word
count. `UmpDecoder` incrementally frames all sixteen message types, including
reserved types, without allocating or guessing a recovery boundary. `finish`
reports incomplete input without discarding it; `reset` explicitly discards it.
Payload bits remain opaque. Group lookup returns a value only for currently
defined grouped types; utility, stream and reserved types return `None`.

`from_be_bytes` and `write_be_bytes` explicitly select big-endian serialization.
UMP itself does not prescribe a universal transport byte order. The writer
rejects undersized destinations before mutation and preserves unused tails.
See the primary [UMP v1.1.2 specification](https://amei-music.github.io/midi2.0-docs/amei-pdf/M2-104-UM_v1-1-2_UMP_and_MIDI_2-0_Protocol_Specification.pdf)
for framing and transport separation. This framing API does not validate message
semantics, assemble SysEx, negotiate protocols or supply native UMP device and
plug-in adapters. Existing processor events and sine synthesis still use MIDI 1.0.

## MIDI 2.0 channel voice codecs

`Midi2Message` interprets MT=4 packets and encodes every defined channel voice
status: notes, poly/channel pressure, control change, registered/assignable
absolute and relative controllers, per-note controllers and pitch bend,
channel pitch bend, program/bank changes and per-note management. Fields retain
16-bit velocity and 32-bit control data. Relative data uses signed two's
complement; pitch bend is unsigned with midpoint `0x80000000`. A zero-velocity
MIDI 2.0 Note On remains Note On. Group and channel use zero-based 0..15 values.

Sender construction rejects out-of-range seven-bit fields. Per-note controller
indices are eight-bit. Encoding zeroes reserved fields; receiving ignores reserved
bits for forward compatibility. Unknown note attribute types retain their type
and data. Type zero canonicalizes its ignored data to zero, and an absent bank
ignores bank bytes. Reserved status 7 and other message types reject. Preserve
the original `UmpPacket` when exact unknown/reserved bit retention is needed.

The codec performs no quantization, MIDI 1.0 translation, controller interpretation,
voice ownership, profile discovery or native delivery. Consumers must negotiate
a group's protocol and implement performance policy. Existing sine/CLAP and
device paths do not automatically gain MIDI 2.0 performance support.

## UMP short messages and SysEx payloads

`UmpMidi1` encodes/decodes MT=1 system short messages and MT=2 MIDI 1.0 channel
voice messages while retaining the explicit group. It preserves defined status
and data without changing protocol or resolution. Unused padding and reserved
high data bits are ignored on reception and zeroed on encoding. F0/F7 and other
reserved UMP system statuses reject; byte-stream SysEx bracketing belongs to
separate payload conversion.

`UmpSysExChunk` encodes/decodes MT=3 SysEx7 and MT=5 SysEx8, with up to six
seven-bit bytes or thirteen unrestricted bytes per packet. Payload excludes F0/F7
and stream ID. Complete/Start/Continue/End forms are explicit; shorter and empty
Start/Continue chunks do not end a message. SysEx8 retains its stream ID and
supports the explicit End/count=15 abort marker. Invalid count/form or seven-bit
data rejects. Unused padding is ignored on decoding and canonicalized on encoding.
These are payload codecs; manufacturer IDs and profile meanings stay opaque.

`SysExAssembler` borrows caller storage and accepts one active group/stream at a
time. It validates ordering and total capacity before copying each chunk, returns
complete payloads as borrowed slices and distinguishes explicit abort. A failed
chunk preserves accepted partial state for retry; `reset` explicitly discards it.
Consumers route concurrent streams to separate assemblers and enforce SysEx7
interspersing/termination rules on the whole UMP stream. No packet timestamps,
manufacturer conversion, negotiated stream limits or native UMP delivery are
implied. Existing native MIDI packets remain complete MIDI 1.0 byte messages.

## Two-zone MPE voice receiver

`MpeReceiver<N>` uses one shared 1..128-slot pool and independent lower/upper
zones. It starts ordinary polyphonic. RPN6 on channel 0 or 15 enables 1..15
members or disables with zero; latest configuration shrinks/disables overlap.
Both active zones share at most 14 member channels; a lone zone can reuse the
inactive master's channel and occupy 15. Every accepted configuration clears
all notes/controllers/tails and reports occupied changed slots, even when the
other zone's layout did not change. Consumers stop/reset corresponding DSP slots.
Repeated configurations also reset. New/changed ranges default to member/master
48/2 semitones; RPN0 whole 0..96 values update common members or master per zone.

Unlike the original single-zone policy, member channels accept simultaneous
notes, and masters accept compatibility notes. Oldest matching held note-off and
global deterministic stealing reuse existing allocation contracts. Member bend,
pressure and timbre freeze after note-off, including pedal-sustained notes.
Master expression continues to update held, sustained and released voices in its
zone while frozen member components remain intact. Master-note bend has no
additional member contribution. Member poly-pressure and zone-only sustain/sostenuto/
reset/all-notes/all-sounds messages are ignored; master controls stay isolated.
Separate master/member pressure/timbre fields leave their combination to DSP.

Both zones disabled return to ordinary 2-semitone polyphony. Unassigned channels
are ignored while a zone is active. Reset/System Reset clears notes/controllers/
selectors but preserves configuration/ranges; `finish` frees a DSP-completed slot.
The sine instrument continues using the original chosen-zone policy. Consumer
sound catalogue/commit, other pedals, mode switching, MIDI-CI and device/DAW
conformance remain separate. All 256 size pairs in both orders, shared-channel/
master notes, ranges, freeze/master tails, isolation/fallback and 10,000 prepared
workloads are covered. See [decision 0059](decisions/0059-two-zone-mpe-receiver.md).

## Prepared UMP device handoff

`PreparedUmpInput::new(1..4096)` returns unique producer/`UmpReceiver` endpoints.
Each `TimedUmp` retains a framed packet, explicit `UmpProtocol` and opaque u64
timestamp. Native producers define the clock domain; these timestamps do not
inherit the MIDI 1.0 connection-local microsecond meaning. There is no conversion
of numeric words, protocol semantics, timestamp origin or audio sample offsets.

`push_words` validates up to 4096 words at a known boundary before publishing
anything. Malformed/oversized whole batches publish nothing; an empty batch is
valid. On full/abandoned queues the accepted prefix remains, the suffix is
reported discarded and is not replayed. One timestamp/protocol label applies to
all packets in that batch. `try_push` publishes one caller-owned Copy record.
Relaxed/wrapping statistics count accepted packets, full-refused packets and
invalid batches; publication does not guarantee consumption if abandonment races.
Already-queued data remains drainable after producer loss.

Construction/drop belong to control threads. Push/pop use no framework heap,
locks or I/O; native callbacks must serialize the unique producer. Tests preserve
all framed types and exact words/timestamps/protocol labels, reject malformed
whole batches, exercise overflow/abandonment and 10,000 prepared and separate-
thread workloads. Native UMP connections and clock/negotiation policy are separate.
See [decision 0060](decisions/0060-prepared-ump-handoff.md).

The separate [CoreMIDI UMP backend](core-midi-ump.md) now supplies protocol-selected
macOS inputs, exact-ID outputs and owned virtual destinations through this queue.
Its synthetic native test preserves MIDI 1/2 words across all groups; physical
UMP devices and negotiated protocol/clock integration remain unvalidated.

## Sustain and sostenuto ownership

CC66 captures only physically held notes on its off-to-on transition. Repeated
on values never capture later notes; notes already sustained after note-off or
released are not captured. `Voice::sostenuto_held()` exposes the retained latch,
and changed-slot masks include latch changes. `VoicePhase::Sustained` means a
received note-off is retained by either pedal. Releasing one pedal preserves
notes retained by the other; neither revives a released tail. CC121 clears both
pedals/latches and releases retained keys; CC120 kills slots. Slot finish/steal,
System Reset, zone reconfiguration and legacy member reuse clear captured
ownership. Pedal-off preserves the note-off release velocity.

Ordinary polyphony isolates pedals per channel. Configured single-zone and
two-zone receivers accept both pedals only on the respective master; zone
compatibility master notes participate. The sine instrument consumes this
allocator behavior with sample-timed CC66 and its existing release envelope.
No soft/half-pedal, piano acoustics or controller hardware conformance is claimed.
The [MIDI Association control table](https://midi.org/midi-1-0-control-change-messages)
defines CC66 and its switching threshold; the
[Yamaha MIDI implementation](https://usa.yamaha.com/files/download/other_assets/5/320475/CBXK1XGE.pdf)
describes capture before subsequent notes. See
[decision 0079](decisions/0079-sostenuto-ownership.md) for receiver policy and tests.

## MIDI bank and program requests

`MidiProgramDecoder` holds sixteen independent pending zero-based 14-bit banks
and last requests. CC0/32 replace their seven-bit half in either order, initially
zero; bank changes alone do not request a sound change. Program Change emits
`MidiProgramRequest { channel, bank, program }` with the pending bank, including
repeated identical requests. Subsequent bank edits preserve the last request.
CC120/121/123 leave this decoder's bank/request intact; explicit/System Reset
clears it. Invalid channel queries return None. Fixed storage/apply/reset are
heap-free. The [MIDI Association overview](https://midi.org/about-midi-part-3midi-messages)
and [controller table](https://midi.org/midi-1-0-control-change-messages) describe
the bank/program combination. These are requests, including unavailable sounds,
rather than committed consumer program identity.

A consumer can use `ProgramBank::midi_index` then `state` for exact complete
selection, preserving current values when unavailable. Channel/MPE master routing,
sample offsets, atomic publication, voices, host/program extensions and UI
notifications remain consumer responsibilities. Existing static voice allocators ignore
Program Change; negotiated single-/two-zone receivers return routed requests,
and the sine reference consumes them on its chosen master.
See [decision 0080](decisions/0080-complete-program-banks.md).

## Sine factory program recall

The public `SINE_PROGRAMS` catalogue contains three complete level states at
MIDI bank 0: program 0 Default (0.2), 1 Quiet (0.05), 2 Full (1.0), with permanent
IDs 1..3. `SineInstrument::programs()` returns its validated immutable bank.
Only the chosen side's master (channel 0 lower, 15 upper) stages bank/program
requests, including before MPE negotiation. This is one global reference level,
not sixteen different channel patches. Other channels and unavailable addresses
preserve level. Recall preserves sounding voices, oscillator phase/envelopes,
zone/ranges and release tails. Level automation and programs obey event order
with no smoothing; reset/reprepare/System Reset clear pending banks while keeping
level. Accepted RPN6 zone configuration also clears selectors/requests;
RPN0 range-only updates preserve them. Program identity is not persisted or inferred from custom level edits.

The CLAP reference receives these changes through process MIDI events, reflects
the resulting level into its existing complete state and coalesces changed
active level publication into a main-thread value rescan. No host program menus,
General MIDI sound bank, per-channel patches or sample/asset loading is added.
See [decision 0081](decisions/0081-sine-program-recall.md).

## Routed MPE program requests

`NegotiatedChanges::program_request` carries an optional request, separately
from changed voices/configuration. `NegotiatedVoices` and `MpeReceiver` stage
banks/request programs on all independent channels in ordinary polyphony. Active
zones accept them only on the respective master, ignoring member and unassigned
messages without staging hidden bank changes. Two zones retain independent master
banks; repeated requests return again. Recall requests never change voice state
or set `configuration_changed`. Consumers choose/catalogue-validate/commit sounds
per master and handle existing notes explicitly.

`last_program_request(channel)` reports the last accepted request, not committed
sound identity. Reset/System Reset and every accepted RPN6 configuration clear
all pending banks/requests, including unrelated-zone masters, matching the
existing complete-controller reset. RPN0 range updates and CC121 preserve program
selectors/requests. The sine reference now uses these negotiated requests rather
than a second decoder, retaining its chosen-master/global-level restriction.
The [MIDI Association MPE overview](https://midi.org/copy-of-midi-polyphonic-expression-mpe-specification-adopted)
describes master-channel Program Change. This remains explicit receiver policy
with no General MIDI sounds or native controller/DAW conformance claim. See
[decision 0082](decisions/0082-mpe-program-requests.md).


Exported native CLAP adapters can explicitly select note/expression input and
output ports separately from raw MIDI/UMP ports. Identified notes preserve native
IDs, ordinary notes preserve unquantized f32 velocity and standard expressions
preserve full f64 scope/value. Metadata and voice application belong to exporters;
the sine instrument retains its MIDI/legacy-note behavior. See the
[explicit native performance profile](extended-processing.md#explicit-native-clap-performance-ports-in-exported-adapters).


Note-end completion reports a processor's voice lifetime to its controller on
the originating input endpoint. It remains distinct from release/choke and has
no implicit MIDI/MPE/UMP conversion. Native/exported adapters and graph queues
carry it without implementing an allocator; see the
[completion profile](extended-processing.md#processor-to-controller-note-completion).
