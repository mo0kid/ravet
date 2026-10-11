# ADR 0257: Six-stage native modulated feedback phaser

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

## Context

First-order allpass and chorus primitives do not provide a complete phaser
with six stages, a controlled frequency sweep, signed delayed feedback and
independent channel histories. Static transfer facts must remain distinct from
floating execution and modulation/headroom claims.

## Decision

Own six existing native TPT allpass histories per channel, previous wet
feedback and a shared base LFO phase. Explicit controls define a log-frequency
four-octave maximum half-span, signed feedback, wet endpoints and adjacent
channel offsets. Admit the complete original floating sweep inside normalized
[1e-6,.49] at8000..768000Hz, with native endpoint representability before mutation.
Do not clamp/repair a callback coefficient. Private already-admitted helpers
share exact public first-order design expressions; public validation stays
unchanged. F64 phase/transcendentals are explicit for both native variants;
recursive/sample arithmetic is native. Feedback has an explicit one-sample
delay, with no implicit algebraic solve/oversampling/normalization/saturation.

One frame advances base phase once and each channel's six states/feedback once.
Dry endpoints continue wet history, with endpoint branches avoiding0×poison
leakage into dry output. Complete prefix admission precedes every state/output
mutation; suffixes stay untouched and empty calls retain history. Control updates
retain phase/history; reset and validated reset-phase clear them. Fixed storage
requires no heap/lock/I/O, but callback transcendentals and native deadline remain
explicit unqualified workload.

## Evidence and limits

Full floating sweep/endpoints/interior coefficient equivalence, all channel1..64
nodal modulation/feedback, direct polynomial cascade, partition/late rejection,
phase/offset/control retention/poison and heap tests qualify this family. Independent
static actual-coefficient rational/analytic-notch and finite matrix-power tail
oracles avoid mistaking a feedback-only tail estimate for the seven-state loop.
Native32 static rounding is observed at nine cutoff/feedback profiles, six
frequencies and131072frames/profile: maximum time1.104e-6, absolute complex
spectrum1.470e-4 and normalized error/(1+|H|)3.062e-6. Initial3e-6 fixture threshold
failure is preserved; full diagnostic supports the selected observational4e-6
fixture. No source compensation or universal analytic error certificate follows.
Native64 tighter comparison remains separate. Actual native32 impulse cancellation
and actual native three-channel Graph/FIR consumers further qualify composition;
the f32/f64 FIR bridge is explicit.

Ideal fixed-coefficient allpass/small-gain stability does not certify arbitrary
modulation, floating overflow/headroom, inaudibility, alias-free retunes or ideal
oscillator time. Infinite recursive tails, nonfinite poison, denormal mode,
constant latency/host reports, limiting/SIMD/deadlines and physical support remain
caller-owned or independently unqualified. Public first-order, chorus and other
existing defaults are preserved.
