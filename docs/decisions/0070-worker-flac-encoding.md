# 0070: Explicit single-worker integer FLAC encoding

## Context

Compressed audio output needs a lossless reusable implementation with explicit
precision, allocation/thread policy and independent interoperability evidence.

## Decision

Add default-disabled flac-encode using pinned flacenc 0.5.1 (Apache-2.0), with
backend default features and parallel configuration disabled. Encode validated
whole signed PCM16/24 clips on the calling worker; bound input samples and returned
encoded bytes. No implicit float quantization, clipping, paths/devices or network
policy is added. This is complete-clip encoding, not a streaming recorder.

Normalize STREAMINFO min/max block bounds to the planned fixed 4096-frame size.
The backend otherwise includes short final blocks in its minimum. The FLAC
standard excludes the final block from that minimum and requires metadata bounds
at least 16 even for one-frame streams:
[RFC 9639 section 8.2](https://www.rfc-editor.org/rfc/rfc9639.html#section-8.2).
Audio frames/sample count/digest remain unchanged, with no padded samples.

## Consequences and validation

1..8 channels/1..96000 Hz and bounded i32 input are the deliberate wrapper subset.
Backend allocation/deadlines are separate from input/returned-output bounds.
Single-worker encoding may be slower than the backend's optional parallel mode.
New normal/build transitive dependencies are locked with their original licenses;
commercial/public release still needs the existing dependency review.

Acceptance roundtrips extrema/all channel counts/precisions, short and partial
blocks/rate edges, source preservation and rejected format/ranges. Independent
FFmpeg CRC-checked decoding exactly matches six synthetic PCM records, including
one-frame and 4097-frame streams. This adds FLAC only; other encoder formats and
native physical recording remain separately scoped.
