# 0045 — optional worker compressed decoding

## Decision

Use pinned Symphonia 0.5.5 behind an off-by-default audio-file feature for actual
FLAC, MP3 and Ogg/Vorbis worker decoding. Enable only selected codecs/containers.
Retain the backend's MPL-2.0 licence and consumer distribution audit boundary;
no backend source is copied or relicensed. Public fixtures contain generated tones.

## Consequences and scope

Input/output/packet counts and metadata options are bounded explicitly. Decode
allocations/time inside the dependency remain backend-owned. Strict whole-result
failure, stable signal specification and finite samples preserve prepared clip
integration without callback decoding. Enable gapless trim and check declared
frames/integrity where available. Unknown-duration end-of-stream is not a universal
truncation detector. No format writer, stream seek or audio deadline claim follows.

## Validation

Three encoded stereo fixtures retain rate/count/declared length, exact FLAC PCM
and lossy tone quality. Capacity, malformed input and half-truncated declared files
reject. Independent FFmpeg Float32 comparisons agree exactly for FLAC and near
floating-point precision for MP3; Vorbis overlaps agree but FFmpeg returns 3968
rather than the declared 4096 frames. Preserve this observed difference instead
of claiming complete gapless conformance. Runtime CI now configures all-feature
checks, which have not been executed on remote runners.
