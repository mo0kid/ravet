# Native Performance AAX instrument reference

The optional local Performance AAX bundle uses the reusable native AAX processing
owner with an explicit no-input instrument profile and a bounded short-MIDI note
collector. It registers distinct mono/stereo factories with the public AAX SDK,
then owns the actual native f32 Performance processor, its exact audio-state cache
companion and complete Level codec. SDKs remain external; no SDK source is copied.

Build the local development bundle with the same external SDK roots as Gain:

```sh
python3 scripts/bundle-aax.py --plugin performance \
  --vst3-sdk /absolute/external/vst3 --aax-sdk /absolute/external/AAX
```

The default Gain command and explicit `--profile stock` retain their behavior.
Performance requires `--profile native`; unsupported combinations reject before
build commands. Performance produces a separate `target/aax/performance/release/`
`.aaxplugin` directory with applicable notices and an ad hoc local signature. It
is not installed into any system plug-in directory and has no production PACE or
Pro Tools acceptance claim. VST3 factory exports retained for external SDK entry
composition do not select or prove the native AAX profile.

The reference is sixteen voices, input stem None and selected mono/stereo output,
main planar f32, sample rate from checked SDK Initialize and at most 1024 frames.
The descriptor declares software-generator category, one LocalInput MIDI node
using all sixteen channels, no host bypass, no multi-mono promise and no
AudioSuite. Actual native schema has only writable global Level ID1, range0..1,
default0.2; no bypass value is synthesized. Its complete native RvS1 chunk persists
accepted Level intent, including values queued before rendering. Voice histories,
MIDI performance state and any in-flight output are transient. Public IDs are new
for this instrument and separate from Gain; neither is a commercial allocation or
released-session compatibility claim.

`RavetAaxNoteInput.hpp` validates a whole immutable borrowed stream of at most
1024 raw packets before publishing native events or count. Packet timestamps are
ordered relative block offsets, strictly below positive frame count. Same-frame
source order is preserved. Positive NoteOn velocities use the exact widened
f32(byte/127); zero-velocity NoteOn is scoped NoteOff, and NoteOff targets channel
and key without inventing per-note identity. Defined short channel/system-common/
real-time messages validate exact status, length and seven-bit payload bytes;
non-note messages are omitted without controller, expression, program or clock
semantics. Running status, undefined statuses, zero/long messages and SysEx F0/F7,
including valid SDK fragmentation, reject the whole block. Unused payload bytes
are ignored. SDK immediate flags accept only Boolean values and do not override
timestamps. No unversioned MIDI-node transport methods are used.

A valid empty stream renders current voices. Missing MIDI node or stream is a
malformed instrument context. The registered callback borrows the stream once;
a throwing node becomes a local rejection, supported silence and recorded status.
Caller supplies live node, Context, frame/owner metadata and correctly sized
immutable stream/packet ranges throughout the callback. Known range overflow,
alignment and alias faults are checked; foreign pointer provenance or capacity
cannot be proved by these helpers. Inputs are absent rather than represented by
non-null effect-style pointer metadata.

Before any complete-state apply, queue consumption or native DSP work, the owner
validates the whole output map and whole raw MIDI stream and checks the effective
trailing global controls plus notes fit the 1024 native-event budget. The newest
complete restore supersedes older global commands; trailing global controls at
block start precede equal-offset notes, retaining both source orders. No sort,
heap action or timestamp extrapolation occurs in this path. Zero-frame callbacks
with any queued intent or notes reject before mutation; the collector separately
permits timestamp0 when validating a zero-frame packet. A valid empty zero-frame
callback can reach the native processor without inventing a flush of queued edits.

Known malformed/late/capacity faults preserve pending accepted Level and native
voice/phase history. Supported output prefixes silence and backing tails remain
unchanged; output samples overlapping owner or borrowed MIDI/header/pointer
metadata are not writable supported buffers and are protected from clearing.
Invoked native errors fail-close readiness and retain the original latest AUDIO
status atomically, separate from stopped native lifecycle status. Explicit stopped
reset/reprepare restore accepted Level while clearing voice history and coalescing
queued intent, as Gain. CONTROL serializes SDK/codec/lifecycle, one AUDIO consumer
owns prepared DSP, and hosts stop/join AUDIO before lifecycle or retirement.
Foreign SDK effects are not rolled back after exceptional partial mutation.

Target helper normal/fast-math and real SDK model checks pass, as does the actual
registered module factory/Initialize/MIDI algorithm fixture. Evidence includes an
independent sine/velocity/attack timing oracle, scoped zero-velocity same-frame
order, exact Level chunk save-before-render, malformed state and packet preflight,
a separately initialized successful voice-phase oracle after late failures, and
10000 maximum/late-fault collector and2000 prepared instrument C++ allocation
cycles. Fresh isolated4/4 CMake and normal public4/4 package contracts pass,
including actual packaged ACF/export/SDK0x20801345980/registered algorithm checks.
Preserved native Gain18/18 and stock Gain14/14 packages and eight Python pipeline
tests pass. These receipts are separate from helper fast-math, whole-SDK and Rust
native allocation evidence.

No auxiliary buses, f64 AAX, SysEx/UMP/controllers/note expressions/programs,
generated MIDI/completions, latency/tail delivery, custom editor,
AudioSuite, production signing, real Pro Tools or other-platform/deadline support
is established. Host MIDI routing is distinct from local descriptor registration.
The native model separately adopts initialized public-SDK
[transport capture](aax-transport.md); raw precision, independent availability
and configured-loop limits are explicit there.
See [native AAX lifecycle](aax.md) and
[decision0218](decisions/0218-aax-native-short-note-instrument.md).
