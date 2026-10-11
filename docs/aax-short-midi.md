# AAX complete short MIDI input

`AaxShortMidiInput` converts a complete borrowed public `AAX_CMidiStream` to
canonical raw native MIDI events. Every defined 1..3-byte channel message on all
16 channels and these system messages is preserved: F1, F2, F3, F6, F7, F8, FA, FB,
FC, FE and FF. F7 means a standalone canonical EndOfExclusive message; it does not
validate, infer or assemble SysEx. F0, fragments/long payloads, running status and
undefined statuses reject. The existing `AaxNoteInput` remains note-only, omits
other defined messages and rejects standalone F7.

At most1,024 packets and1,024 frames are accepted. Offsets are ordered and strictly
inside a nonempty block; a zero-frame block permits offset0. The boolean immediate
flag must be0/1 and does not invent an offset or scheduling override. Exact wire
length and seven-bit data validate for every packet. Unused packet bytes are not
interpreted. Stream/packet/output/count ranges are live, aligned and disjoint;
whole list preflight precedes publication. Failure preserves output and count.
Equal-offset source order and wire payload are retained in native kind14 events.
The locally supplied SDK documents the timestamp as the block-relative playback
position and its compatibility note identifies timestamp0 as immediate while the
flag is currently unset. Boolean1 is accepted without overriding the authoritative
timestamp. Tests cover flag1 at offset0/valid nonzero offsets, decreasing order
and nonzero offsets at zero frames; invalid timestamps still reject atomically.

`NativeAaxEffectParameters` and `NativeAaxProcessor` add a final, default-false
`RawShortMidi` template option for the explicit input-only instrument Context.
Context layouts and Gain/Performance defaults are unchanged. The raw owner uses
lock-free AUDIO admission before the SDK MIDI getter. Nested/concurrent callbacks
return Busy before getter, DSP, staging, diagnostics or active outer sample
mutation. CONTROL stopped lifecycle and snapshots require stop/join. Admission loads detect
an AUDIO callback already entered; they do not exclude a callback that begins
after the check, so the caller must stop and join before these operations.
Raw scalar acknowledgements, complete recalls and host value requests remain
accepted during AUDIO through the existing SPSC CONTROL intent queue. Existing intent scheduling requires
one CONTROL producer and one AUDIO consumer; this option is not a new thread-safe
SDK parameter manager.

The raw owner explicitly accepts fully preflighted zero-frame offset0 events and queued controls without phase/sample advancement. The existing note/effect default keeps its empty-only zero-frame restriction. Actual component fixtures prove zero-frame controller/note-on/off and known Full program request changes, untouched audio/tails, and late malformed/combined-capacity refusal before state/dequeue/history.

All packets and the effective trailing controls after the last pending complete
restore must fit the prepared event capacity **before** restore application,
command dequeue, transport capture or DSP. Main output/callback metadata preflight
also precedes native history. The raw callback validates the public
`AAX_IMIDINode` base alignment/range and rejects owner and known Context, Context-array, owner-pointer, frame and output
pointer-array metadata overlaps before its virtual getter. That exact known live base span joins protected callback metadata; audio
arrays or samples intersecting it reject, and fault silence only clears independently
admitted safe output prefixes. Private node extent, live typed storage and host
getter behavior remain caller/SDK obligations. Prepared internal64 capture/narrowing follows
[the AAX precision contract](aax-precision.md): data-dependent output overflow can
advance native history before rejection and never claims rollback. Independent
transport callback availability and noncoherent getter tuple limits remain those
of [AAX transport](aax-transport.md).

An optional constructor telemetry companion enables explicit stopped snapshots.
Its complete metadata/functions/count and exact core/sample identity preflight
before schema/cache creation. Cache ownership is CONTROL-only; borrowed outputs
must not alias the owner/model. No live automatic publication, UI reflection or
telemetry wire/state codec is implied. [Exact client acquisition](native-client-acquisition.md)
balances partial ownership on null/violating throw, while the native ABI remains
nonthrowing. A stopped snapshot detects and refuses an already admitted raw callback, but
stopped/joined ownership is required; the check is not a concurrency exclusion. Dynamic timing
candidates do not inherit a telemetry companion; snapshots describe only the
current slot and refuse if that selected slot has none.
This raw instrument package does not instantiate a dynamic-timing telemetry
consumer; that profile is not qualified by its instrument receipts.

For raw owners, `lastNativeProcessStatus()` starts at numeric1 before any native
return and survives prepare/reset/deactivate/reinitialization. There is no atomic
"has ever returned" bit: initial1 is numerically indistinguishable from an actual
returned1. Callers must track their own processing observations. Stopped per-block
diagnostics distinguish the latest admitted attempt, including fresh Preflight
flags after early Context metadata refusal; they are not a lifetime return history. An admitted early refusal without trusted frames does not clear audio. Pre-admission metadata refusal leaves prior diagnostics unchanged. This raw/native64 reset policy makes no claim about generated-output diagnostics.

The maximum packet profile uses fixed callback stack scratch. Apple Clang21
production Release (`-O3`, arm64) static-frame estimates for the actual raw-MIDI
plus transport profile are240/256 bytes for the SDK model callback,115280/115312
for the native32/64 owner, and57456 for its collector. Their partial nested sums
are172976/173024 bytes (about169KiB). These are compiler `.su` estimates, not observed
runtime stack measurements or an upper bound: other helper/host/native/caller
frames are excluded. The previous revision estimated a different no-transport
profile; its figures are historical. Small audio-thread stacks are unqualified;
callers must budget their actual build's complete call chain. Native64 sample
scratch remains heap-owned before AUDIO, separate from the event stack.

## MidiSine reference

The distinct `ravet_midi_sine_*_v1` exports in `ravet-performance-native::midi`
consume canonical raw MIDI through `SineInstrument<16>` or the additive
`SineInstrument64<16>` facade around the same render kernel. f64 output is genuine,
including values beyond f32 resolution; it is not a widened f32 output array.
Existing `SineInstrument::new` type inference and f32 sample behavior are preserved.

No-input mono/stereo synthesis retains existing note velocity, release, pedals,
bend, RPN/MPE negotiation and voice allocation. Pressure/timbre are observed voice
expression and do not modulate this sine. Program application is explicitly
disabled: raw bank/program requests remain exact transient observations, but do
not replace acknowledged global Level. Ordinary voice/negotiation behavior,
including zone-driven resets and request clearing, remains the existing receiver
policy. The complete state has Level ID1/default0.2 only, product `RAVETMS\0`,
version1. No bypass, program selection identity, channel patches or voice history
is invented as persisted state.

The23 stopped telemetry fields are documented in
[`RavetMidiSine.h`](../examples/performance-native/include/RavetMidiSine.h): frame/
packet totals, raw ordered checksum/class/count, voice slot0/lifecycle/velocity, acknowledged
Level and exact last bank/program request per channel. These are transient
validation observations, not host parameters or a session codec. Reset/prepare
clear voice/history/requests/counters while retaining Level; complete Level recall
preserves current voice history.

Build the distinct local reference with external SDKs:

```sh
python3 scripts/bundle-aax.py --plugin midi --vst3-sdk /path/to/vst3 --aax-sdk /path/to/AAX
python3 scripts/bundle-aax.py --plugin midi --internal-precision 64 --vst3-sdk /path/to/vst3 --aax-sdk /path/to/AAX
```

Host audio stays planar f32. Native precision is selected explicitly with exact
origin tables. Artifacts use `target/aax/midi/release` and `midi64/release`, distinct
`org.ravet.midisine.mono/stereo` effect identities and a no-bypass LocalInput node
mask0xffff. Stock/editor selections refuse before commands. Gain/Performance
paths and public IDs remain unchanged. Notices, local ad-hoc signing, exports/SDK
version and actual registered component contracts are mandatory; there is no host
installation or production PACE signing.

The external wrapper's module infrastructure requires an authored empty Steinberg
factory bootstrap. It exposes **zero VST3 classes**, rather than selecting an
unrelated DSP class. Actual startup/bundle hooks, compatibility descriptor identity,
zero class count, owned AAX registration/factory→Initialize→raw Process and balanced
shutdown are tested against actual modules. The bootstrap is not a stock VST3
algorithm mapping or a VST3 instrument export.

Target-only normal/fast collector and optional-telemetry validators plus actual
registered native32/64 modules pass. Tests cover all123 defined status/channel
variants, independent onset/bend/pedal timing, program/Level coherence, maximum
packet callbacks with no ordinary C++ new/delete, combined late capacity and raw/
transport/alias faults preserving queued intent and phase, and observed stopped-lifecycle refusal plus nested AUDIO Busy. Actual getter-blocked CONTROL calls prove accepted SPSC scalar/restore/request
publication with exact native Level telemetry before.3 and after a distinct.6 restore, plus independent phase/audio. An authored
coherent native core facade separately pauses inside authoritative DSP after its
queue snapshot: CONTROL publication leaves the current live voice/Level intact,
then the following block consumes a distinct complete restore in source order and
matches independent native audio. A1000-publication concurrent producer/consumer
stress passes in normal/fast C++ and a ThreadSanitizer-instrumented C++ owner;
the linked Rust archive is not sanitizer-instrumented. Qualified task receipts record the sanitizer argv/toolchain, imported `__tsan` symbols/runtime, and captured stdout/stderr/exit. The registered module worker is an uninstrumented separate test. This is bounded concurrency
evidence, not exhaustive interleaving proof.
Actual node admission tests prove no getter on owner-aliased/unaligned/null nodes,
public base-byte preservation on writable sample overlap and safe stereo silence
with backing tails intact.
Actual Rust tests prove shared f32 equality and f64 output above f32 resolution.
See [ADR0247](decisions/0247-aax-short-midi-input.md) for receipt scope. Public
packaging/default regression qualification passed after adoption.

Generic effect/auxiliary raw MIDI contexts, SysEx/UMP, host program menus,
AudioSuite/editor/generated output for this reference, Pro Tools/PACE validation,
other-platform hosts and real-time deadlines remain outside this package. Offline
new/delete counters do not cover arbitrary SDK/native/Rust/host allocations.



The raw profile is explicitly input-only: combining RawShortMidi with generated
output is a compile-time rejection. LastNativeProcessStatus retains the last
actually returned native AUDIO code in both precisions. Local Preflight, Busy or
Exception does not replace it; rawDiagnosticsStopped exposes attempted/returned,
state/process codes and command/sample transfer flags after stop/join. A native
refusal and violating throw poison readiness and preserve independently safe
output tails; no DSP/history rollback is implied.

Registration evidence uses owned GetEffectDescriptions and public SDK proxies;
virtual SDK overrides and the registered algorithm execute module code, while
inline diagnostic/stopped helpers also compile into the fixture. The proprietary
ACF host registration chain and a real Pro Tools session remain unvalidated.
The compatibility descriptor's dummy VST3 FUID has no selectable factory class;
only the owned AAX callback is a supported DSP creation path. The zero-class
bootstrap is not an assurance that unrelated stock wrapper creation can succeed.

The promotion packet hashes the actual isolated harness, executable-copy runner,
production test driver, manifests/lock and behavior-defining native/helper
sources. Harness5/5 receipts are distinct from production CMake6/6 receipts; the
production list includes explicit actual module arguments and the maintained
normal/fast positive-input/negative raw-generated compilation gate. Packaging
retains required exports and verifies local signatures; the shared native archive
also carries ordinary Performance exports, so the AAX bundle's export set is not
claimed exclusive. Registry/index/status/generated-doc adoption and fresh public
Gain/editor/Performance/stock plus joint workspace gates remain separate required
steps before public acceptance.
