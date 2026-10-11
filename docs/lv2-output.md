# Native LV2 generated short MIDI

`RavetLv2MidiOutput.hpp` stages at most 1024 canonical native short MIDI records
into one LV2 Atom Sequence. It is an external-SDK C++17 helper; no LV2 SDK sources
are included in RAVET. The optional output profile of `NativeLv2Processor` consumes
this helper and the native processor's existing generated-output ABI. The default
`MaximumOutputEvents = 0` preserves Gain and Performance port layouts and calls.
The opt-in public NoteRouting reference uses the same authoritative Rust processor
as its other native consumers, with mono or stereo silent audio output, Level
ID 1, complete 24-byte Level state, timed note input and optional block-start
transport. It adds one Atom output port after the existing input Atom port.

Build a local bundle with external LV2 headers:

```sh
python3 scripts/bundle-lv2.py --sdk /path/to/lv2 --plugin note-routing
```

The output directory is `target/lv2/ravet_note-routing.lv2`, with its module,
manifest, routing TTL and applicable notices. The pipeline requires the actual
packaged-module contracts, normal/fast staging and native-owner contracts, and
native-origin contracts. Signing and validator failures propagate. This command
creates local artifacts; it does not install a plug-in or validate a DAW.

## Output allocation and publication

CONTROL maps all required URIDs once. A generated-output instance should reside
on the CONTROL heap: prepared event/output scratch is stored in the instance.
The host supplies a live allocation aligned to eight bytes, containing an Atom
header whose type is `atom:Chunk`. `atom.size` declares available **body** bytes,
excluding its eight-byte header. On every run the declared body must cover the
configured worst case, even when DSP will generate zero events:

- body: `8 + 24 * MaximumOutputEvents` bytes;
- complete allocation: `16 + 24 * MaximumOutputEvents` bytes.

For the reference's 1024-event capacity, these are 24584 and 24592 bytes. TTL's
`resize:minimumSize` describes the total 24592 bytes. After each call the host must
reset the header to Chunk and restore its available body capacity before reuse.
A published Sequence's `atom.size` reports only the used body bytes. The reference
accepts at most 1024 frames and 1024 combined changed-control/note events per run.
The additive `MaximumFrames` template argument defaults to the existing 65536;
a consumer must explicitly match its native prepared limit.

The owner admits output capacity and known ranges before applying pending state
or calling native DSP. Its known protections include the owner and native tables,
controls, input Atom header/declared body and writable audio prefixes. Caller
allocations and lifetimes must actually cover the supplied ranges; integer checks
cannot establish that storage exists. Inputs and controls remain live and
immutable during processing. Audio outputs are mutually disjoint and disjoint
from metadata and both Atom buffers. On a rejected call, only independently safe
audio prefixes are silenced; backing tails and protected bytes remain untouched.

Every native record and complete serialized prefix is checked before any host
output is copied. Frames and equal-offset source order are preserved, the
Sequence uses explicit frame-time units, and payload padding is zero. Native On
and Off velocities must be exact widened f32 values in 0..1; short MIDI lowering
quantizes them to seven bits. There is no inferred identity or scoped-release
policy in the generic stager. The reference explicitly lowers channel/key-only
scoped release to ordinary MIDI Off; Level scales On velocity and retains Off
velocity. Defined unsupported input messages remain governed by the existing
note-only LV2 collector. This reference does not expose generic raw input.

## Refusal, ownership and diagnostics

Failed admission or staging invalidates prior readiness. A fresh per-call token
can publish at most once. A complete 16-byte empty Sequence is written on refusal
only when that exact write was independently admitted. Wrong type, misalignment,
known alias or a body shorter than eight bytes permits no Atom output write.
There is no replay of a previous staged prefix. An atomic AUDIO admission rejects
nested/concurrent runs before foreign reads, diagnostics, DSP or active output
mutation. This admission is not a general CONTROL scheduler: the caller stops and
joins AUDIO and excludes new run admission before connect, activate, deactivate,
restore, diagnostic reads or retirement. A busy check on those methods does not
replace that exclusion. Standard LV2 `threadSafeRestore` is not advertised.

`outputDiagnosticsStopped` reports separate attempted/returned native, state
apply and state reflect results, local error, native report and publication count.
An initial code of 1 is a never-returned sentinel; a code is meaningful only when
its returned flag is true. `lastNativeOutputStatus` retains the last **actually
returned** native process code. A violating callback throw does not manufacture
a new native status. Returned poison or local corrupt report/staging and caught
runtime exceptions refuse later processing until a fresh instance. Nonpoison
returned errors retain their original values. Native ABI callbacks otherwise
retain their documented nonthrowing ownership obligations.

Whole structural/control/note/time preflight precedes state transfer and DSP.
Failures after successful native processing cannot roll back consumed state,
controls or DSP history. Reflection or late malformed output refuses publication
and uses the independently admitted safe empty output; diagnostics distinguish
that from a rejected block before DSP. The reference's transient frame telemetry
is a validation oracle and is not persisted Level state.

## Validation and limits

Target-only evidence currently exercises the actual canonical Rust NoteRouting
archive and actual mono/stereo module, independent MIDI byte/timing/padding
oracles, complete recall and zero-frame processing, max 1024 output, aliases,
backing tails, combined input capacity, and 2000 prepared module calls without
ordinary C++ allocation/deallocation or URID mapper calls. Forwarded authoritative
native fixtures measure real frame history and distinguish no-DSP rejection from
post-DSP corruption; they check exact returned errors, nonreturned exceptions,
reentry and fresh publication. Stager tests exercise 10000 maximum/late-fault
cycles under the same ordinary C++ allocation counter. Normal and fast-math
fixtures check integer-seeded foreign scalar bytes; constructor/control metadata
continues the existing ordinary-IEEE contract. These counters do not measure all
allocator families, runtime stack peaks, deadlines or host behavior.

The maintained target-only CMake composition passes all12 validators, including
Gain and Performance regressions. The self-contained NoteRouting bundle passes
its6 mandatory validators and ad-hoc signature verification;6 Python tests cover
profile separation and every output validator/signing failure. Recorded canonical
public Rust archives are explicitly substituted into the real CMake/package
pipeline; that original receipt is not a fresh Cargo build. A separate root
replay now executes current-source Cargo for Gain, Performance and NoteRouting,
all twelve native validators, the maintained NoteRouting package, its six mandatory
validators and strict ad-hoc signature verification. Public workspace acceptance
and actual LV2 host validation remain separate. No LV2 host
validation, SysEx, UMP/MIDI 2, multiple event outputs, generated identity/expression,
auxiliary LV2 bus registration or editor is claimed. Native f64 remains separate
from this f32 LV2 profile. Existing Gain and Performance validators are required
regressions; a reference does not make the entire format framework complete.
