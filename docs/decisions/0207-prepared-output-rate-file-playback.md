# 0207. Prepare bounded file clips at an explicit output rate

- Status: Implemented; focused public integration validation passed.
- Work package: W194.

Existing MemoryPlayback owns a finite clip and requires consumers to run at its
metadata rate. Existing file read-ahead preserves source-rate frames. Standalone
applications need a reusable, explicit file-to-selected-output-rate path rather
than treating those frames as if they had a different rate.

Prepare a complete bounded output-rate clip off audio using existing reader and
SincResampler contracts. Choose exact same-rate Copy or explicit Sinc filtering;
retain fractional causal delay and finite tail rather than inventing a rounded
trim/flush policy. Compute nominal duration and retained support separately with
integer arithmetic. Preserve decoder gapless source semantics and exact committed
source/submission/padding/output progress.

The source is failure-owned: no seek rollback or retry after reads/EOF-integrity
failures, and no complete clip claim for an output prefix. Preflight formats,
configuration, known duration and memory/output budgets before reads; unknown
duration uses conservative caller maximum preflight. Transfer the existing bounded
output Vec into MemoryPlayback without a second whole-clip copy.

Add a default-disabled own-crate optional ravet-signal dependency and default-disabled
device forwarding feature, with no third-party dependency/version. FilePlaybackOutput
implements the actual existing PreparedOutput/open_output Processor path, rejects
mismatches during prepare and keeps file/converter work outside callbacks. Existing
exclusive transport semantics apply; no cross-thread command queue is added.

Validation must include independent exact rational duration/support, arbitrary batch
partitions, half-sample impulse delay, analytic rate/pitch/channel reference, actual
WAV/AIFF/compressed gapless sources, original reader/error/prefix recovery including
codec EOF-integrity failure, and actual PreparedOutput variable callbacks/zero
allocation/configuration/lifecycle/transport checks. Evidence does not establish
physical-device support, live drift/clock mapping or callback deadlines.

Focused isolated validation passed ten file-playback groups, three existing memory
transport groups, five actual PreparedOutput groups and five Rustdoc tests, plus
both warning-denied all-feature/all-target Clippy checks and Rustdocs. Tiny1/2/17/31/257
frames, taps16/64/256, ratio1/16..16, arbitrary source partitions, fractional impulse
delay, analytic stereo tone reference >45dB interior SNR, actual WAV/AIFF and
gapless FLAC/Vorbis/MP3 source1/17/8193, original EOF-integrity cursor/prefix recovery,
configuration/lifecycle/transport and allocation-free callbacks are covered.

Tracked public validation repeated all ten file-playback, three memory transport,
five actual PreparedOutput and five Rustdoc groups (23 tests/doc-tests), with
warning-denied all-feature/all-target Clippy for both packages. The default-off
normal audio-file dependency graph contains only ravet-audio-file itself; optional
resampling adds no codec/default dependency. Scoped Rust formatting and diff checks
passed. Shared workspace/default/all-feature documentation gates belong to the
integration snapshot recorded in WORK_STATUS.md.
