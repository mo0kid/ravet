# LV2 gain reference

The public reference exports mono and stereo LV2 descriptors with f32 audio ports
and block-start physical gain/bypass controls. It uses reusable native processor,
cached schema and coherent complete-state tables through the external-SDK C++17
[NativeLv2Processor helper](../crates/ravet-native/include/RavetLv2Processor.hpp).
Consumer factories still own plugin URIs, Turtle metadata and product identity.
The helper now requires the exact originating core pointer returned by cached
schema metadata and the exact shared parameter-schema pointer returned by the
cache and state companions. Callback-equivalent copied cores and separately
copied schema tables refuse before any resource creator. Metadata accessors are
queried once on CONTROL; a violating query exception invalidates the helper
before acquisition. The native ABI remains nonthrowing. Existing constructor,
port and lifecycle contracts are unchanged. Cache/state top-level table addresses
have no additional authoritative accessor in this ABI; a copied top-level view
that retains the exact actual core/schema provenance is not falsely rejected.
See [decision 0261](decisions/0261-lv2-exact-origin-admission.md).
No LV2 SDK source is included in RAVET.

```sh
CARGO_BUILD_JOBS=2 python3 scripts/bundle-lv2.py --sdk /path/to/lv2
```

Supply an external source SDK containing `include/lv2/core/lv2.h` and `COPYING`.
Version 1.18.10 was exercised. The script builds with two jobs, copies Turtle
metadata and the binary into `target/lv2/ravet_gain.lv2`, adds notices and tests
the actual packaged binary. macOS receives an ad hoc binary signature. No global
host installation occurs. `--plugin gain` is the default; `--plugin performance`
selects the separate instrument bundle through the same build/package pipeline. Linux compilation is configured but its runtime is
unvalidated; this fixture has no Windows branch.

Plugin URIs are `urn:ravet:gain:mono` and `urn:ravet:gain:stereo`. Mono ports are
input, output, gain, bypass; stereo ports are left/right inputs, left/right
outputs, gain, bypass. The helper accepts one main input/output bus with 1..64
channels and 1..1,024 writable schema controls, bounded by its explicit template
arguments; Gain selects at most two channels and two controls. Fixed native IDs,
physical ranges and stepping come from the prepared schema. Gain spans 0..4 with
unity at 1; bypass snaps to 0/1 at 0.5. Finite controls clamp/snap through native
schema mappings; nonfinite controls reject atomically with output silence.
Unchanged canonical controls do not restart smoothing. All controls require
connected, aligned pointers. Inputs may alias outputs; output ranges must be
mutually distinct. Host pointers must cover the stated frame count, and control
and instance metadata must stay disjoint from writable samples.

Processing supports zero-frame changes and at most 65,536 frames. Full audio and
control preflight precedes state application and rendering. Unsupported larger
blocks silence at most the supported prefix; backing tails survive. Null sample
ports are allowed for zero frames. Ordinary rejection allows later valid blocks;
a contained panic disables the instance until recreation. Activate resets
history and preserves requested values. Native consumer hooks remain responsible
for atomic backend behavior; a failure after state application cannot generically
roll back arbitrary consumer history. Build with ordinary IEEE finite semantics.

`state:interface` saves a complete native canonical state as portable POD
`atom:Chunk` under `urn:ravet:gain:state`. The `urid:map` instantiation feature is
optional for processing and required for state calls. Save may overlap audio:
it reads the coherent publication, without accessing DSP or connected controls.
A contended one-attempt snapshot returns an error; there is no retry loop. Restore
must be serialized with all callbacks in the LV2 Instantiation threading class;
`state:threadSafeRestore` is not advertised. Wrong type, flags or native payload
reject before publication. A missing property restores schema defaults. Save
propagates the host's store error.

Hosts also persist control-port values independently. Explicit restore holds its
values until a connected canonical control changes. Each changed control wins for
its parameter at the next valid block. Controls reapplied by the host during
recall become authoritative. If controls are unavailable at restore, their first
valid connection wins. This policy prevents unchanged old port values from
immediately undoing a requested complete-state restore.

Actual-binary contracts check both descriptors, preparation, wet/dry samples,
control recall, zero-frame changes, malformed-control silence, lifecycle, tails,
input/output aliases, state/default recall, malformed payload atomicity, missing
features, host refusal, disconnected/misaligned/overlapping ports and oversized
blocks. Another probe checks 4,000 run/save cycles without C++ heap operations or
URI mapper calls and coherent saves concurrent with 20,000 audio blocks. This
probe does not measure Rust allocations, physical devices or audio deadlines.

The Gain profile has no Atom port, sample-timed automation, MIDI input, custom UI,
programs, f64 or auxiliary buses; the separate instrument profile is described
below. No `hardRTCapable` declaration is made. LV2 host
discovery, metadata validation and actual DAW session recall need separate
evidence. See [decision 0167](decisions/0167-lv2-native-schema-state.md) and the
[native processing guide](native-processing.md).

## Performance instrument

The separate public [performance factory](../examples/performance-lv2/README.md)
exports `urn:ravet:performance-sine:mono` and
`urn:ravet:performance-sine:stereo`. It links only the public 16-voice native sine
reference, with mono/stereo f32 outputs, Level control ID1 and one Notes Atom input.
There are no audio inputs. Level defaults to 0.2, ranges over 0..1 and uses the
same complete-state/control precedence policy above; state product `RAVETPS\0`,
version 1 persists Level only. Active voices and DSP history are not saved.

`NativeLv2Processor<1, 2, true, true>` explicitly selects the instrument profile;
Gain's default effect profile is unchanged. Instantiation requires `urid:map` to
cache Sequence, MIDI event, frame timestamp and the selected time:Position
identifiers. The fourth template argument opts into validated transport; the
default MIDI-only instrument template retains its prior URI and processing
requirements. The MIDI converter
accepts a Sequence body of at most 65,536 bytes and 1,024 raw ordered events,
including ignored events. Timestamps use audio frames (default unit 0 or explicit
`atom:frameTime`); beat units reject. Zero-frame blocks accept offset 0 only.
The host provides aligned storage covering the complete declared body.

Complete MIDI1 short messages of 1..3 bytes are validated for exact defined status
length and seven-bit data. NoteOn/NoteOff retain channel/key and velocities as
exact widened f32 fractions `byte/127`. NoteOn velocity 0 maps to NoteOff according
to MIDI1 convention. NoteOff releases all matching voices at that channel/key;
no per-note IDs are invented. Other defined short MIDI messages are ignored.
SysEx and F0/F7 delimiters, running status, undefined statuses and long MIDI
payloads reject the whole block. Other Atom types are ignored after bounded
length/timestamp checks, except the opted-in Position object checks below. Pedals, pitch bend, expression, program changes and
MIDI2 are outside this note profile.

Changed block-start controls precede same-frame MIDI, preserving note source
order. The native capacity of 1,024 events is shared by controls and notes; a full
1,024-note block needs unchanged controls. Malformed input and capacity failure
precede state application and DSP, preserving pending recall and voice history.
There is no Atom output port. Native completion notifications are suppressed;
internal voice completion still occurs. The converter separately exposes atomic
bounded conversion in [RavetLv2NoteInput.hpp](../crates/ravet-native/include/RavetLv2NoteInput.hpp).

Build a self-contained public instrument bundle using the same entry point:

```sh
CARGO_BUILD_JOBS=2 python3 scripts/bundle-lv2.py --sdk /path/to/lv2 --plugin performance
```

This produces `target/lv2/ravet_performance.lv2` with its manifest, instrument
metadata, actual module and LV2/RAVET licence notices. The script validates that
packaged module and signs it ad hoc on macOS. It performs no host installation.
Shared build/package/runtime mechanics live in
[scripts/lv2_bundle.py](../scripts/lv2_bundle.py); selecting Gain explicitly clears
the optional performance CMake profile so the default build remains independent.

For direct native development and both references' regression contracts:

```sh
cargo build -p ravet-gain-native -p ravet-performance-native --release --locked
cmake -S examples/gain-lv2 -B target/lv2/performance \
  -DCMAKE_BUILD_TYPE=Release -DRAVET_LV2_SDK=/path/to/lv2 \
  -DRAVET_GAIN_LIBRARY="$PWD/target/release/libravet_gain_native.a" \
  -DRAVET_PERFORMANCE_LIBRARY="$PWD/target/release/libravet_performance_native.a"
cmake --build target/lv2/performance --parallel 2
ctest --test-dir target/lv2/performance --output-on-failure
```

This produces `target/lv2/performance/ravet_performance_lv2.so`. Gain's library is
required for the shared CMake configuration and regression targets. The optional
performance target uses its own archive and separate metadata in
`examples/performance-lv2`. These direct commands exercise the actual native module; the bundling command
above packages its metadata for explicit host discovery. Discovery/recall in a
DAW remains unvalidated.

The synthetic mono/stereo probe verifies analytic timing/velocity/frequency,
sample-exact split blocks, atomic malformed-input faults, Level state and pending
recall, same-frame ordering, release, full shared event capacity and 2,000 prepared
cycles without C++ heap or URI mapper calls. The existing Gain actual-module suites
also pass against the extended helper. This evidence establishes no LV2 DAW
support, physical MIDI, hard real-time deadline or other-platform runtime. See
[decision 0170](decisions/0170-lv2-midi-performance-instrument.md).


## Prepared LV2 block-start transport

The external-SDK C++17 [RavetLv2Transport.hpp](../crates/ravet-native/include/RavetLv2Transport.hpp)
collector publishes a canonical `RavetNativeTransportV1` only after complete
sequence and recognized time-object preflight. Cache its 14 distinct nonzero
`Lv2TimePositionTypes` URIDs on stopped control; collection never calls `urid:map`.
The performance factory advertises `time:Position` alongside MIDI on its existing
Notes and transport Atom input and forwards the direct native snapshot to its
processor. Gain retains its effect ports/default profile. The generic helper's
fourth template argument defaults to false and requires the existing instrument
Atom port when enabled; no effect Atom port or generic bus factory is inferred.
The external source SDK must also include `include/lv2/time/time.h`.

The selected exact scalar encodings are Long `time:frame`, Double `time:beat`,
Float `time:beatsPerMinute`, `time:speed` and `time:beatsPerBar`, and Int
`time:beatUnit`. Sample positions retain the full signed i64 range without passing
through a float. Beat retains its original f64 value, while Float tempo widens
exactly and acquires no additional source precision. Beat must be finite, tempo
finite positive, and signature numerator finite integral 1..65535 with denominator
1..65535 before conversion. Both signature fields are required to publish a
signature; an individually valid incomplete pair remains unavailable while other
fields survive. Speed exactly 0 or 1 means known stopped or playing. Reverse,
fractional, other and nonfinite speeds reject the block instead of becoming a
guessed stopped state.

At most one Position object at frame offset 0 supplies the block-start snapshot.
Later timestamped Position objects are fully validated, then omitted; the helper
never subtracts offsets, extrapolates a timeline, dispatches mid-block transport
or carries fields from previous blocks. An absent/empty block-start Position is
unknown transport and reaches native DSP as a null context. Duplicate offset-0
Positions reject. The current performance DSP ignores transport, so this adds a
validated format boundary rather than transport-driven musical behavior.

The collector shares the bounded sequence contract: 8-byte-aligned live storage
covering a body of 8..65536 bytes, at most 1024 ordered raw events, frame unit 0 or
`atom:frameTime`, and offsets within the active slice (offset 0 only at zero
frames). Sequence reserved pad must be zero. Each Position permits at most 128
unique nonzero property keys, including ignored keys. Complete object/property/
padded value extents receive preflight; recognized properties require context 0
and exact scalar type and byte size. Unknown properties and other bounded Atom
bodies remain opaque without recursive validation or inferred semantics. Foreign
storage must remain live and disjoint from output; address checks do not establish
memory ownership. IEEE finite checks inspect foreign representations under
optimized fast-math, though the full processor retains its ordinary IEEE build
requirement.

A complete native signature requires DETAILS. Its recording, loop and pre-roll
false flags are unknown/default placeholders: this vocabulary/profile supplies
none of those Booleans, and native DETAILS has no per-flag availability. Consumers
must not interpret them as host-known false values. Without a complete signature,
DETAILS stays absent. Bar/barBeat/frame rate do not become inferred native bar
pairs, global beats or sample conversion; discontinuity, recording, loops,
pre-roll, output time objects and tempo ramps have no mapping in this profile.

Malformed recognized fields, including a later Position that would otherwise be
omitted, reject before complete-state apply, parameter publication and DSP.
Accepted pending recall, connected control intent and native phase/history survive
for the next valid block; supported output prefixes are silenced and backing tails
survive. The native processor still owns backend failure atomicity after apply.
The collector uses bounded local storage without heap, map callbacks, locks or I/O.

Normal and optimized fast-math collector fixtures cover exact sample extremes,
f64 precision/signed zero, Float widening, independent/partial/absent context,
unsupported speeds, type/context/length/padding/order faults, duplicate keys and
objects, maximum property/event counts and prior-output retention. Both exercise
actual Rust zero-frame native acceptance of collected snapshots, plus corrupted
canonical dependencies. A forwarded native-table fixture proves exact context
reaches the real processor and a late fault causes no state-apply/DSP calls, while
the default MIDI-only profile retains opaque-time behavior. The actual packaged
mono/stereo module verifies mixed Position/MIDI timing, phase/control/pending-state
retention, supported silence/tails and 2000 prepared cycles with zero counted C++
heap actions or URI maps. Gain actual-module regressions remain required. These
local macOS arm64 checks establish no Linux runtime, DAW discovery/session timeline,
metadata validator, device, complete backend allocation or real-time deadline
support. See [decision 0191](decisions/0191-lv2-block-start-time-position.md).


## Exact-origin qualification

Both bundling profiles require the maintained origin-admission validator in
addition to their actual selected-module contracts. Synthetic coherent native
facades exercise byte-identical copied core and cache/state copied-schema refusal,
null/throwing metadata getters, zero resource creators on every refusal, unchanged
invalid-helper callback storage, exact one-time balanced acquisition/retirement and
real Gain processing. A top-level cache/state copy retaining actual origins is an
explicit positive case, reflecting the existing ABI's provenance limits.

Gain and Performance actual modules exercise the same corrected helper with
canonical exported tables and retain their audio/control/state/MIDI/transport
regressions. Local isolated qualification and fresh public archive acceptance are
recorded separately; no LV2 DAW, other-platform or deadline claim follows.

The focused isolated composition passes all seven maintained CMake tests and the
self-contained Gain (three validators) and Performance (five validators) bundles.
Four bundler Python tests include mandatory origin-gate refusal propagation.
These receipts use recorded canonical isolated native archives; fresh public
archive/reference and shared programme acceptance remain separate pending gates.


## Optional native generated output

The explicit `--plugin note-routing` profile consumes the canonical reusable native
NoteRouting processor, with timed mono/stereo short MIDI output, Level state and
silent audio. `NativeLv2Processor` appends default-zero generated capacity, leaving
Gain and Performance ports unchanged. The reusable `Lv2MidiOutput` stager admits
the whole configured Chunk body capacity before native state/DSP, validates the
complete prefix, publishes once and writes only independently admitted safe empty
Sequences after failure. It rejects processing reentry before active buffers or
diagnostics mutate. CONTROL retains stopped/joined caller exclusion.

See [generated output](lv2-output.md) for exact body/total sizes, caller lifetimes,
status provenance, no replay, late-failure history limits and the scoped reference.
Target-only CMake12, packaged mandatory6 and Python6 receipts use the recorded
canonical native archives. A separate root replay rebuilds all three canonical
Rust native archives, passes all twelve actual-native profiles and packages and
validates NoteRouting through the maintained pipeline with actual Cargo and strict
ad-hoc signature verification. Public workspace acceptance remains separate; no
LV2 DAW, SysEx, UMP/MIDI2, auxiliary host routing or editor is inferred.
