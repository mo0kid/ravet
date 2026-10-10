# 0170: LV2 bounded MIDI note input and native performance instrument

Status: prototype; local actual bundled modules and Gain regressions passed.

The native LV2 profile gains an explicit third `Instrument` template argument,
which defaults to false so the existing Gain input/output/control port contract
is retained. The instrument variant prepares zero audio inputs and mono/stereo
f32 outputs, schema-ordered writable controls and one selected input Atom
Sequence. A separate public factory embeds `ravet-performance-native`; consumer
archive symbols, product state and plugin URIs remain distinct from Gain.

The standalone reusable external-SDK C++17 `Lv2NoteInput` converter fully stages
at most 1,024 raw events from a Sequence body of at most 65,536 bytes. It validates
sequence/header alignment, declared body and padded event lengths, recognized
frame timestamp units, nonnegative ordered sample offsets, and selected MIDI
payloads before publishing any output or count. Zero-frame blocks accept only
offset zero. Unknown bounded Atom payloads are ignored after timestamp/length
checks. Hosts own live storage covering the declared sizes; the adapter cannot
prove arbitrary pointer accessibility. Compiler and native lifetime obligations
follow the ordinary LV2 helper.

Only complete MIDI1 short messages of one to three bytes are accepted. Defined
status values require their exact message length and seven-bit data bytes.
NoteOn and NoteOff become native events; NoteOn velocity zero follows the MIDI1
NoteOff convention. Other defined complete short messages are ignored. SysEx,
its F0/F7 delimiters, running status, undefined statuses and every long MIDI
payload reject the complete block. This is a note-input profile, without MIDI
pedals, pitch bend, expression, program changes, SysEx or MIDI2 support. Ignored
messages still consume the raw-event bound.

Native notes keep channel/key and the exact widened f32 velocity fraction
`byte/127`. No note IDs are invented. NoteOff scopes channel+key; duplicate native
voices at the same address share that release. Same-frame source order is kept,
with changed block-start control values preceding MIDI notes. The shared native
budget is 1,024 controls plus generated notes: a full 1,024-note input requires
unchanged controls. Failure before state/render preserves pending recall,
controls and voice/phase history. This instrument's native transactional sine
has no additional identified-note conflicts because input does not invent IDs.
No event output is prepared: native completion notifications are suppressed,
while internal release/stealing ownership still completes.

The existing `scripts/bundle-lv2.py` gains explicit `--plugin performance`, with
Gain remaining the default. A focused `scripts/lv2_bundle.py` module owns immutable
profiles/context, command execution, selected target builds, packaging and
validation. Both profiles share those mechanics. Performance produces its own
public `.lv2` directory with manifest, Turtle metadata, native module and applicable
SDK/RAVET licence notices. macOS ad hoc signing is local; no normal host directory
is installed or changed. Boundary tests cover profile separation, early invalid
SDK/platform rejection, signing selection and validator-failure propagation.

The public mono/stereo actual bundled module passed analytic sample timing, frequency,
MIDI velocity and attack samples; exact partitioned rendering; malformed late
input rollback; complete Level state and pending recall; same-frame ordering and
zero-velocity release; release completion; full shared capacity and 2,000 prepared
render/release cycles without C++ heap operations or URI mapper calls. The
converter also passed malformed payload/status/timing/unit/size atomicity and
its full raw/generated capacity. Both existing actual Gain audio/state suites
passed against the extended helper. C++ instrumentation does not measure Rust
heap traffic or callback deadlines. DAW integration,
metadata discovery, physical MIDI devices, other platforms and production
signing/support remain unvalidated.
