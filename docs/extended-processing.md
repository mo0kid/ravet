# Extended portable processing

`AudioBuffer`, `ProcessBlock` and `Processor` now take a sample type, defaulting
to `f32`; `f64` is supported explicitly. Views borrow planar storage and never
convert precision. `AudioLayout` permits up to 64 channels per direction,
including input-only sidechains and no-input instruments; both directions cannot
be empty. Processors advertise only the layouts they implement. Gain and its
CLAP adapter still reject anything beyond mono/stereo f32 main-bus effects.

`ProcessConfig::with_auxiliary_buses` accepts up to eight additional layouts.
Supply their views in preparation order through `ProcessBlock::auxiliary`;
frame counts and channel layouts must match exactly. No auxiliary slice means
no auxiliary buses. Constructors check backing capacities before processing.
Validation rejects the whole block before state or samples change.

Output events use caller-owned `OutputEvents` storage, constructed empty for each
callback, with the active frame count. Set `with_output_events` during preparation
and supply exactly that capacity in `ProcessBlock::output_events`. Push preserves
the accepted prefix on capacity, value, timestamp or ordering errors; equal-frame
events retain insertion order. Zero-frame callbacks permit only offset zero.
Consumers own format conversion and delivery/backpressure. The CLAP effect
adapter currently rejects configured output events rather than silently dropping
them. `ProcessBlock::clear_outputs` silences main/auxiliary active outputs and
clears emitted events on faults, preserving inputs and backing tails.

`Processor::latency_samples` defaults to zero; `tail` defaults to `Tail::None`.
Other tails are finite sample counts or infinite. These declarations do not
implement latency compensation or drain processing. Format/device adapters must
map them to their own lifecycle protocols; latency changes cannot invoke host
restart APIs from DSP callbacks.

## CLAP transport

`EffectBridge::process_with_transport` forwards known block-start playing state,
beat position, positive finite tempo and seconds converted to sample position at
the prepared sample rate. Absent transport retains unknown fields. Unadvertised
fields are ignored; invalid advertised tempo/overflow rejects the block and
silences host output. The gain wrapper supplies CLAP's process transport.

Consecutive known sample positions detect seeks/loops with one sample tolerance:
playing callbacks advance by the previous accepted frame count; stopped callbacks
retain their position. Reset or missing context breaks tracking. Rejected blocks
do not advance tracking. The legacy `process` entry point supplies no transport.
`Transport.details` additionally carries explicit recording, active loop and
pre-roll flags, positive numerator/denominator signature, finite BPM-per-sample
tempo increment, zero-based bar number/start and active beat/sample loop bounds.
CLAP context supplies these flags even when false; absent context leaves details
unknown. Signature/tempo/bar require their native validity flags. Active loops
require strictly increasing bounds in each advertised timeline, including after
seconds-to-sample rounding; inactive native loop fields are ignored. A loop flag
without either timeline is preserved without guessing bounds. No timeline
extrapolation, DSP resets or mid-block updates are inferred. This is tested
adapter conversion, not new DAW transport validation or deadline evidence. See
[decision 0127](decisions/0127-extended-block-transport.md).

See [decision 0017](decisions/0017-extended-processing.md) and
[work status](WORK_STATUS.md).


## Native-precision main-bus CLAP adapter

`MainBusBridge<P, S>` supplies a separate f32/f64 main-bus adapter for effects or
no-input instruments, with 0..64 inputs and 1..64 outputs. Prepare at most 65,536
frames and 1,024 events. Input/output scratch is separate; legal input/output
aliases copy before DSP runs. Native precision matches the host buffer exactly;
wrong or dual precision rejects and silences every supplied output. No sample
conversion occurs. Maximum f64 scratch is 64 MiB. Preparation/drop happen outside
audio; process/flush/reset do not allocate, block or perform I/O.

An optional selected note-port index translates validated MIDI short messages
with timestamps. Status determines length; three-byte CLAP padding beyond it is
ignored. Other-port MIDI and unsupported kinds are ignored but all timestamps
and total event counts validate. Global parameter mapping retains existing
semantics. Complete UMP packets also retain numeric words and timing on the
selected port. Unused words in the fixed CLAP array are ignored. Flush applies
only parameters. CLAP note IDs/wildcards, pointer-based SysEx, modulation and
auxiliary buses are pending. Exporters must declare
only implemented ports/precision/dialects; the original gain adapter is unchanged.
Block-start transport continuity follows the rules above.

Fixtures exercise no-input synthesis, invalid-event preservation, exact f64
64-channel copying and prepared heap behavior. The [sine CLAP reference](sine-clap.md)
consumes the f32 path through a built native bundle. This does not establish
arbitrary DAW f64/multichannel compatibility. See
[decision 0040](decisions/0040-clap-main-bus-instrument.md).

## CLAP generated-event delivery

`MainBusBridge::prepare_with_output` accepts an independent generated capacity of
0..1024 events and an optional selected MIDI output port. `process_with_output`
and `flush_with_output` supply a real host queue and return `BridgeReport` with
DSP status plus `OutputDelivery`. Audio/input scratch limits remain unchanged;
generated event storage is allocated separately during preparation. The original
`prepare`/`process`/`flush` API retains zero-output policy, rejecting nonzero
capacity before DSP instead of silently discarding output.

Global physical parameter values map to CLAP param-value events. Validated raw
MIDI retains sample offset/bytes, with zero padding for short messages. UMP
output maps to CLAP MIDI2 events with zero unused words. Portable
normalized notes default to selected-port MIDI with 7-bit velocity rounding; a
positive note-on rounds to at least one, and zero remains MIDI velocity zero.
Exporters must advertise compatible MIDI output ports/dialects. Reserved CLAP
parameter ID u32::MAX or MIDI without a selected port makes the entire generated
stream unrepresentable; delivery emits nothing and reports all undelivered events.

Generated streams are preflighted for representability before touching the host
queue. Successful queues retain equal-frame order. When the host rejects an event
(often capacity), delivery stops immediately and reports its accepted prefix and
discarded suffix. Audio/DSP/transport have already progressed and remain valid;
there is no rollback, hidden retry, deferred allocation or later-block replay.
Consumers explicitly own overflow diagnostics/recovery. Invalid input or a DSP
fault silences audio and delivers no generated events. Parameter-only flush input
uses zero-time processing; generated flush events have offset zero. Host queue
callbacks can allocate or block outside this crate's control; framework paths and
prepared fixture queues make no heap operations.

Four fixture groups verify parameter IDs, raw wire lengths, selected MIDI ports,
normalized-note quantization, equal-frame ordering, zero-time flush, whole-stream
preflight, host prefix rejection, rejected input/DSP output discard and 10000
prepared delivery callbacks. Other formats/exported MIDI output ports still need
their own mappings/host evidence. See [decision 0047](decisions/0047-clap-output-events.md).

## Sample-timed UMP representation

Foundation `EventData::Ump` stores one framed packet without allocation and shares
ordered offset/capacity validation with other events. `ParameterEvents` retains
its MIDI 1-only `translate_midi`; `translate_midi_ump` explicitly includes CLAP
MIDI2 events. `MainBusBridge` uses the combined path when a selected input port
is present. UMP data words remain exact, including opaque reserved types; semantic
validation and negotiated protocol are processor responsibilities.

Generated UMP requires a selected output port, uses the existing whole-stream
representability check and accepted-prefix rejection policy, and canonicalizes
unused CLAP array words. This supplies packet transport, not MIDI 2 synthesis.
The sine and gain reference dialect advertisements remain MIDI 1-only. An
all-message-type echo fixture checks timing, port routing, words, unused padding,
parameter-only flush, ignored-event timing and 10,000 prepared callbacks with zero
framework heap operations. No MIDI 2 DAW/device support is inferred from this test.

## Ordered CLAP auxiliary buses

`MultiBusBridge<P, S>` prepares the full main/auxiliary `ProcessConfig` in native
f32 or f64. Audio input and output port indexes are compact **independently**:
visit the main bus then auxiliaries, skipping zero-channel directions. For a
stereo instrument with an input-only mono sidechain, native input port 0 is the
sidechain and output port 0 is the main stereo bus. An output-only auxiliary is
output port 1. Exporters must advertise these exact ports and selected precision;
this mechanism alone does not add ports to existing gain or sine references.
The [sidechain reference](sidechain-clap.md) advertises stereo main input/output
and an input-only mono detector through the adapter.

All inputs are copied before any host output write, including aliases across
buses. Complete port counts, channels, precision, timestamps and transport are
checked before calling DSP. Rejected input silences every output and preserves
processor/transport history. Processor faults may follow internal progression;
generated events are discarded on faults. Successful generated delivery retains
whole-stream preflight and host prefix-refusal behavior. Every native output's
constant mask is cleared after a successful write.

The adapter accepts up to eight auxiliary buses, 64 channels per direction per
bus, 65,536 frames and 1,024 input/output events each. Combined prepared sample
and event storage is capped at 64 MiB, so maximal bus/channel/frame shapes cannot
all be combined. Processor-owned memory remains separate. Parameter-only `flush`
uses matching zero-frame main/auxiliary views and does not advance transport
history. Construction/destruction allocate outside callbacks; reset, processing
and flush use prepared storage. Native fixture tests check compact sidechains,
output-only auxiliaries, f64 precision, complete rejection/silence, generated
events and 10,000 processing/flush/reset workloads. DAW auxiliary-port validation
and other format mappings remain separate work. See
[decision 0058](decisions/0058-clap-auxiliary-buses.md).

## Explicit parameter gesture events

`EventData::ParameterGesture { id, phase }` carries a raw global begin/end
boundary with stable parameter identity and sample offset. It changes no scalar
value and imposes no ownership, pairing or recording policy. Parameter graph
filters include matching boundaries; performance filters exclude them. Complete
worker event queues retain their identity, phase and order. Generated CLAP output
maps boundaries without a MIDI port; hosted graph output requires an explicit
opt-in and writable output mapping. See [the hosting guide](plugin-hosting.md#global-parameter-gesture-output)
and [decision 0121](decisions/0121-parameter-gesture-events.md).

## Global modulation signals

`EventData::ParameterModulation { id, amount }` carries a finite signed physical
amount separately from saved base values. A new amount replaces the previous
signal; the consumer implements combining, clipping, smoothing, reset and
persistence. Parameter graph filters include the matching ID; performance filters
exclude it. Complete worker queues preserve order and reject non-finite amounts.

CLAP input translation is opt-in through
`with_global_parameter_modulation(true)` on `ParameterEvents`, `EffectBridge`,
`MainBusBridge` or `MultiBusBridge`. Default input continues to ignore modulation.
Only global targets map; native amounts must narrow to finite f32, and flush
normalizes offsets to zero. Invalid whole input rejects before DSP. Generated
output maps global amounts without a MIDI port and preserves host prefix refusal.
Exporters must advertise compatible native flags and explicitly implement DSP;
no reference processor support or saved-state changes are synthesized. See
[decision 0122](decisions/0122-global-parameter-modulation.md).


## Explicit note identities

`EventData::NoteWithId { id: NoteId, phase: NotePhase, channel, key, velocity }`
carries On/Off with a concrete channel/key and product-local u32 identity. Ordinary
note scalar validation applies; On with zero velocity remains On. The framework
does not assign IDs, impose uniqueness, match voices or infer release lifetime.
Performance graph filters and complete worker queues preserve the whole event.
The full u32 identity is portable; backend limits are checked at conversion.

Hosted CLAP graph builder `with_note_identity_input(true)` and
`with_note_identity_output(true)` require explicitly selected CLAP note ports for
the corresponding directions. Native IDs preserve 0..=i32::MAX; invalid/disabled
input rejects before native calls, and disabled output retains filtering of
specific-ID notes. Legacy wildcard-ID native output still maps ordinary NoteOn/
Off. Concrete malformed negative IDs reject in the selected identity profile.
MIDI/MPE/UMP paths cannot silently remove identities. Default generated CLAP
performance output uses MIDI, so an identified note makes its whole generated
stream unrepresentable before host delivery; audio/DSP already progressed.
The public sine receiver continues to consume legacy notes/MIDI only. No instrument
voice-ID allocation, note choke/end, third-party/DAW or native deadline
claim is added. See [decision 0132](decisions/0132-native-note-identities.md).


## Scoped note expressions

`EventData::NoteExpression { target: NoteTarget, expression, value: f64 }` carries
seven standard expression units. `None` channel/key/ID fields match any voice in
that field within the routed endpoint. Volume is a linear multiplier from zero
through four, with zero explicitly muting the voice; pan is 0 left, 0.5 center, 1 right; tuning is relative
semitones from -120 through 120. Vibrato, expression, brightness and pressure
are 0..=1. All values must be finite. Expressions replace preceding expressions;
receiver DSP owns matching, application relative to voice defaults and smoothing.
They do not edit global parameter state or imply a voice allocator.
CLAP has a narrower volume range (strictly above zero through four). Exported
and hosted CLAP conversion rejects portable zero volume without epsilon
substitution; portable queues and native records retain its exact f64 bits.
See [decision 0159](decisions/0159-zero-volume-note-expression.md).

Performance graph filters and complete worker queues retain scope and all f64
bits. Hosted builder `with_note_expression_input`/`with_note_expression_output`
require explicit CLAP ports for each enabled direction. Input rejects disabled,
invalid or native-too-wide IDs before processing; output filters disabled,
unknown types and unrelated ports. Selected malformed output faults after native
processing, discarding the block's audio/events while retaining native history.
Default generated MIDI output rejects an expression-containing whole stream
before host delivery, retaining already rendered audio/DSP. The public sine
receiver keeps its existing legacy-note/MIDI behavior. No voice matching, note
lifecycle, per-note parameter or third-party/DAW/deadline claim is added. See
[decision 0134](decisions/0134-scoped-note-expressions.md).


## Explicit native CLAP performance ports in exported adapters

`ParameterEvents`, `EffectBridge`, `MainBusBridge` and `MultiBusBridge` accept
`with_clap_note_input_port(Some(index))`; the main/multibus adapters also provide
`with_clap_note_output_port(Some(index))`. Indices are bounded to 0..256; `None`
keeps default native-note filtering/MIDI-generated-note behavior. Selections are
independent of raw MIDI/UMP ports and audio buses. Exporters must advertise the
selected native CLAP dialect and implement the processor's matching/voice policy;
the adapters create neither metadata nor synthesis behavior.

During processing, selected native On/Off with concrete channel/key preserve
optional IDs, timing and checked f32 velocity. Native wildcard IDs map legacy
notes; specific IDs map NoteWithId. Selected expressions retain all seven standard
units, wildcard scope and exact f64 values. Unknown expression types and other/
wildcard ports filter; malformed selected standard data rejects the whole block
before DSP and silences native audio. Scoped Off and choke support is described below. Native selected representable
performance at zero frames rejects; parameter flush ignores performance input.
All host event count/order/offset checks still include ignored kinds.

Generated legacy and identified notes use native On/Off with unquantized f32
velocity widened to f64; zero-velocity On stays On. Expressions preserve full
scope/value. IDs beyond nonnegative i32, absent native ports or native performance
in parameter flush make the entire generated delivery unrepresentable before any
host prefix; successful audio/DSP remain progressed. Raw MIDI/UMP keep their
separate selected ports. Host refusal still reports the delivered prefix and
discards the suffix without replay. This closes adapter identity/expression
translation; public gain/sine semantics and port metadata remain unchanged.
Offline f32/f64 main-bus and f64 auxiliary-bus tests cover timing, precision,
filtering, pre-native rejection, output refusal and prepared heap behavior.
See [decision 0135](decisions/0135-exported-native-performance-ports.md).


## Scoped note release and choke

`NoteOffScoped { target: NoteTarget, velocity: f32 }` carries a matching key release
with finite 0..=1 velocity; `NoteChoke { target }` carries a choke request with no
velocity. Each optional channel/key/ID field is a wildcard within its routed
endpoint. Full portable u32 IDs remain valid; native conversions restrict IDs to
nonnegative i32. Receivers own voice matching, release envelopes and termination/
fade policy. Neither event synthesizes an allocator, note completion or saved
state edit. Concrete legacy/identified Off events retain their existing meaning.

Performance graph routing and complete worker queues preserve these events.
Hosted builder `with_scoped_note_input`/`with_scoped_note_output` require selected
CLAP ports per enabled direction. Defaults reject portable scoped input and filter
native scoped output. Enabled input supports all matching Off/choke scopes and
rejects invalid whole prefixes before native mutation. Output preserves wildcard
channel/key Off and all choke scopes; concrete Off retains legacy/identity profile
selection. Malformed raw scope rejects instead of negative-to-wildcard collapse;
unrelated/wildcard ports filter. Choke's native velocity is ignored, as specified.
Selected output faults follow existing complete discard/native nonrollback policy.

Exported adapters' explicit native CLAP ports also support scoped Off/choke.
Default generated MIDI rejects these streams before any host prefix, retaining
successful audio/DSP. Native zero-frame performance/parameter flush restrictions
remain unchanged. Note end is a distinct plug-in-to-host completion protocol tied
to input-port identity and ignored timestamps; it is not inferred from Off/choke.
Tests cover portable ranges/prefix preservation, complete queues, selected native
fields/defaults, linked f32/f64 timed fixture audio and output faults plus exported
scope/velocity/heap behavior. Offline evidence only, without third-party voice,
DAW, hardware or deadline claims. See
[decision 0136](decisions/0136-scoped-note-release.md).


Scoped parameter values/modulation use distinct f64 events with NoteTarget scope
within a routed input endpoint. They remain parameter events for graph filtering,
independently of note expressions and saved global state. Exported CLAP adapters
provide independent input/output scope selectors, preserving exact values even
when audio uses f32, including normalized zero-time parameter flush. See the
[scoped parameter profile](parameters.md#scoped-values-and-modulation-in-exported-clap-adapters).


## Processor-to-controller note completion

`EventData::NoteEnd { target }` reports completion on the originating note input
endpoint, independently of key release and choke. Optional channel/key/ID scope
uses the usual checked portable ranges. Its carrier offset follows normal event
buffer bounds/order but conveys delivery order rather than an exact voice
termination time. Performance graph filters and complete worker queues retain it;
receiver code owns identity reuse and lifetime tracking.

Exported `MainBusBridge`/`MultiBusBridge::with_note_end_output_port(Some(index))`
select an input endpoint below 256 independently of note output, MIDI/UMP and
scoped parameter routes. Missing routes/native-too-wide IDs reject whole generated
delivery before any host prefix while rendered audio/DSP remain progressed.
Native note-end ignores timestamp/velocity, and the adapter retains the bounded
carrier offset in its event header. Parameter flush rejects generated completions.
Incoming native end notifications are filtered as unsupported processor input;
general input count/order/offset validation still includes ignored events.

Hosted completion output uses a separate input-endpoint profile and canonical
carrier ordering; see [plug-in hosting](plugin-hosting.md#completion-notifications-on-originating-input-endpoints).
No exported voice allocator or public reference instrument behavior is inferred.
Tests cover portable/graph/queue preservation, native f32/f64/main/multibus output,
whole preflight, prefix refusal, flush and prepared heap policy. See
[decision 0140](decisions/0140-note-completion.md).
