# 0265: Bounded native generated output for LV2

Status: integrated prototype source; fresh native/module/package replay qualified, public workspace acceptance separate.

## Decision

Add a reusable external-SDK `Lv2MidiOutput` stager and an opt-in generated-output
profile to `NativeLv2Processor`. Append default-zero capacity and the existing
65536-frame default without changing Gain or Performance ports or native calls.
Consume the actual canonical NoteRouting native entry in a separately selected
public local bundle. Preserve exact native companion provenance introduced by
[0261](0261-lv2-exact-origin-admission.md); additive overlaps must be reviewed together.

Use CONTROL-mapped URIDs, 8-byte aligned Atom storage, declared Chunk body
capacity and full configured worst-case admission before pending-state or DSP
consumption. Stage the entire validated event prefix before one host copy. A
fresh private token permits one publication or independently admitted complete
16-byte empty Sequence. Failure invalidates readiness rather than replaying old
bytes. Protect known metadata and Atom/audio ranges before writes and silence
only independently safe supported audio prefixes.

Use one lock-free AUDIO admission flag to reject processing reentry before any
active callback mutation. CONTROL still requires stopped/joined caller exclusion;
the flag is not a general lifecycle lock or thread-safe LV2 restore declaration.
Retain actual returned native/state codes separately from local faults and
nonreturned exceptions. Document successful DSP history/state consumption before
late output/reflection failure rather than claiming transactional history rollback.

## Rationale and evidence

The LV2 Atom specification distinguishes body capacity from RDF total allocation
size. Its example output ports are presented as Chunk storage and rewritten to
Sequences. Fixed maximum capacity allows complete early admission and bounded
staging. A note-routing consumer proves actual native output adoption instead of
counting detached table fixtures as a reusable format integration.

Local target fixtures exercise canonical Rust history, state, zero-frame notes,
late malformed input/output, backend refusal/throw/poison, processing reentry,
capacity and aliases, independent actual module MIDI/timing/padding and maximum
output. The maintained target-only CMake composition passes12 validators, including
Gain and Performance regressions. The self-contained NoteRouting bundle passes
all6 mandatory validators and ad-hoc signature verification;6 Python tests
exercise profile separation and every output validator/signing failure. This
receipt explicitly substitutes recorded canonical public Rust archives into the
otherwise real build/package pipeline; it is not a fresh Cargo build or public
promotion. A separate root replay now executes fresh current-source Cargo for all
three canonical native references, all twelve native validators and the maintained
NoteRouting package with actual Cargo, six mandatory validators and strict
ad-hoc signature verification. Preserve initial invalid constant-NaN
fixture observations separately: integer-seeded before/after byte checks reproduce
correct rejection in both normal and fast compilers, so no common finite-classifier
production defect is established from that invalid fixture.

## Limits

The stager accepts canonical short MIDI output with native identity limitations.
No SysEx, UMP/MIDI 2, output expressions, extra event ports, native f64 LV2 audio,
auxiliary host routing, editor, DAW validation or real-time deadline support is
inferred. Reference note-only input remains unchanged. Fixed output scratch does
not assert zero stack usage by input/transport collectors or all-allocation-family
instrumentation. See [the guide](../lv2-output.md) for exact caller and failure rules.
