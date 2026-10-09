# 0043 — bounded AIFF streams

## Decision

Implement reusable uncompressed AIFF PCM streams in the optional audio-file crate.
Preserve explicit worker I/O, prepared frame capacities and WAV-style retry and
poison semantics. Decode extended80 integer sample rates exactly; reject unsupported
precision/container/alignment instead of silently rounding or guessing. Seekable
input supplies an actual extent so malformed FORM/chunk lengths reject before reads.

## Consequences and scope

The new reader/writer owns its format-specific big-endian/chunk framing. It shares
errors and the interleaved f32/ring boundary with WAV, without replacing the existing
WAV-specific worker pumps. AIFF-C/compressed codecs and automatic transport remain
separate work. No new dependency, file discovery, hardware or callback I/O is added.

## Validation

Independent reordered-chunk/offset fixtures, every truncated prefix, malformed
rates/counts/duplicates, width/rate edge roundtrips, padding, read retry and partial
write poisoning pass. macOS Core Audio recognizes the synthetic PCM24 output and
converts 1024 stereo frames to exact expected PCM16 samples. This is file evidence.
