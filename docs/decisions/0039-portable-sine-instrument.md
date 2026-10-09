# 0039 — Integrated portable sine instrument

Date: 9 October 2026. Status: implemented within stated limits.

Voice and envelope APIs need an integrated consumer to establish lifecycle
behavior. Add a bounded no-input f32 sine processor to the instrument crate,
using its opt-in single-zone receiver and signal ADSR. No format/device/UI
dependency is introduced. Sine phase is continuous under bend retuning but
resets on new/stolen notes; note-off starts one release, sustain delays release,
and completed envelopes return slots. Zone reset kills their envelopes.

Fixed ADSR and level/N mixing make behavior reproducible. Stereo duplicates
mono. Seven-bit note quantization and 49%-of-rate frequency clamping are explicit
reference policies. Pressure/timbre stay available but unmapped; transport and
unknown parameters are ignored. Full validation before mutation preserves the
portable fault contract. Declared infinite tail reflects indefinite held notes.

Analytic attack/sine fixtures, partition equality, release completion, sustain/
stealing, zero-frame events and fault tests provide offline evidence. Prepared
10,000 workloads report no Rust heap activity. An executable renders synthetic
stereo audio to memory. This establishes portable synthesis, not a native
instrument export, host state/editor contract or physical deadline.
