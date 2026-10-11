# 0218: Native AAX short-note instrument profile

Status: implemented prototype; strict normal/fast-math note/owner fixtures, real
SDK model, actual registered instrument module, isolated4/4 and normal public4/4
packages pass. Preserved native Gain18/18, stock Gain14/14 and eight Python pipeline
tests pass; actual packaged exports/SDK0x20801345980/algorithm evidence is local.

Extend the checked owned AAX model with an explicit compile-time no-input
instrument profile, preserving Gain template defaults, Context layout, native IDs
and separate stock regression. The actual Performance native Level-only schema has
no bypass; do not invent one. SDK controller input stem must be None, output must
match selected mono/stereo, and public descriptors declare software-generator,
LocalInput MIDI node/mask0xffff, no bypass/multi-mono/AudioSuite. Select the exact
cache returned by the originating audio-state companion; standalone Performance
cache export has a different info callback address in the inspected local archive
and is correctly rejected by the exact owner provenance check.

Use a reusable bounded short-MIDI collector over immutable borrowed SDK packet
streams. Whole shape/timestamp/status/length/data/capacity/range preflight precedes
any publication. Maximum1024 raw packets and frames1024; defined short non-notes
validate then omit. Reject running status, undefined statuses and all SysEx/long
messages including valid AAX fragmentation. Preserve exact relative timestamps
and equal-offset source order; Boolean immediate flags do not override them.
NoteOn positive velocity is exact widened f32(byte/127); velocity zero maps to
channel+key scoped NoteOff with no invented note identity. No unversioned transport
access or expression/program semantics follows from ignored messages.

Validate output map and raw MIDI, then effective trailing queued controls plus
notes<=1024 BEFORE complete-state apply or dequeue/native DSP. Block-start global
controls precede equal-offset notes; newest complete restore coalesces prior global
commands as the accepted owner. Known failures retain accepted Level and voice
history, silence supported prefixes and preserve tails. Borrowed MIDI/header/
owner/pointer metadata cannot be cleared as samples. Missing node/stream rejects;
valid empty stream is current-voice processing. Zero frames with any queued intent
or notes reject before mutation. Provider errors fail-close and retain exact AUDIO
status independently of stopped lifecycle. No native completion output requested.

CONTROL serializes SDK, codec and stopped lifecycle; AUDIO exclusively owns DSP.
Stop/join before lifecycle or retirement. Fixed staging storage has no C++ heap
path but uses bounded stack scratch; caller stack capacity, foreign node callbacks,
SDK/Rust allocator behavior and whole memory/deadline remain separate obligations.
Foreign ranges/contexts remain trusted/live and capacity-correct; no isolation or
pointer-provenance guarantee. SDK partial foreign side effects have no rollback.

Complete native RvS1 Level state includes accepted pending intent; voices/MIDI
performance are transient. New exact reference IDs are distinct from Gain, not
commercial identifiers or production/released-session compatibility promises.
Package --plugin performance uses native profile only, preserves default Gain and
explicit stock paths, local notices/ad-hoc signature and mandatory actual binary
checks. VST3 exports retained for external entry mechanics never infer AAX profile.

Acceptance separates strict helpers and real SDK model from actual exported
registered factory→SDK Initialize→MIDI algorithm, independent analytic note timing
and separately initialized successful voice-phase oracle after late failures.
Full CMake/public package and preserved Gain native/stock checks are separate.
No f64/aux/SysEx/UMP/controller-expression/program/transport/generated output/
latency-tail/editor/AudioSuite/Pro Tools/PACE/other-platform/deadline claim.
