# 0164: Transactional native performance synthesis

Status: implemented; local tests, lint and documentation validation passed.

## Decision

Add an independent mono/stereo native f32/f64 sine consumer for normalized and
identified performance. Keep the legacy MIDI/MPE sine's quantization/pedal/program
policy unchanged. New/retriggered/stolen notes reset phase and ADSR; expression
changes retain phase. Volume/expression multiply exact stored velocity, tuning
adds semitones, stereo uses explicit linear balance and mono ignores pan. Vibrato
is a six Hz sine up to one semitone with its rate bounded below Nyquist. Oscillator
frequency is capped at 49% of sample rate. Brightness/pressure remain exact readable
state without altering this reference's timbre. Level uses the existing stable
ID/schema, default .2, and scales the mix by level/voice capacity.

Prepare private f64 stereo scratch and bounded completion storage on control.
Validate the complete generic block before rendering into scratch with a fixed
copy of DSP/voice/global state. Commit audio/completions/state only after the
entire candidate succeeds. A late identity conflict or completion overflow
therefore preserves live history and silences rejected output. Candidate rendering
uses actual ADSR completion before later events, allowing an ID's new address
after in-block ownership ends. An event-only simulator would conservatively reject
that valid case. Fixed state copy/drop performs no heap actions.

Positive prepared output capacity explicitly enables original-target completion
for steals/chokes and envelope finish; zero capacity suppresses notifications,
not internal lifetime. Input order and sample/slot order determine delivery, with
no hidden retry or substitution for release. Zero-frame events apply without
advancing DSP time. Level edits clamp while explicit complete physical setters
reject out-of-range values. Other parameters/MIDI/UMP/incoming completion and
transport have no synthesis semantics here and are ignored after validation.

## Validation and limits

Both precision fixtures cover analytic unquantized attack/tuning/amplitude/pan,
partition equality, timed mute/unmute and pressure/brightness retention, release
and original-target completions, zero-frame/-zero On, late malformed/identity/
output-overflow atomic rejection, continued phase/envelopes, actual in-block ID
reuse, optional notification suppression, extreme valid rates and invalid prepare,
tails/reset/deactivate, maximum128-voice completion workloads and counted heap
actions. Preparation bounds 65536 frames/1024 input and output events, accepts no
auxiliary buses, and may allocate. No format/native facade, state codec, editor,
physical device/DAW/deadline/other-platform or click-free stealing claim follows.
