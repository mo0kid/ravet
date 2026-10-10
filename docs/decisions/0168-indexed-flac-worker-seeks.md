# 0168: Explicit verified native-FLAC packet indexes

Status: implemented; scoped local tests and lint passed.

## Context

The lazy compressed reader intentionally replays from the beginning for exact
seeks across formats with different timestamp, reservoir and preroll policies.
Interactive native FLAC positioning can avoid decoding all preceding audio while
retaining the existing read-ahead and stopped-generation contracts.

## Decision

Add `CompressedReader::new_indexed_flac` as an explicit worker-only mode. Require
native FLAC and a known nonzero frame count; use the pinned Symphonia 0.5.5 backend
to decode the complete stream once. Check each packet's track, contiguous frame
timestamp, untrimmed decoded duration, finite samples and stable specification;
check declared total and available decoder MD5 at the end. Retain only immutable
compressed packets, codec parameters and exact frame offsets in an `Arc` index.
The original container copy and complete decoded PCM are not retained.

Packet count is limited to 65,536, cumulative packet payload to 64 MiB, encoded
input to 64 MiB, caller frames to 16,777,216, total interleaved samples to
16,777,216 and decoded packet capacity to 65,536 frames. These constrain framework
storage/work; backend allocations and transient preparation memory remain separate.
Metadata/visual limits retain the existing 64 KiB/zero configuration.

Candidate seeks binary-search packet offsets, construct a fresh decoder and
decode only the containing FLAC packet. The pinned decoder explicitly stores no
state between packets; this property must be reviewed on any backend change.
EOF candidates decode no packet. Decode verification is disabled for suffixes
because the original stream's MD5 cannot describe a suffix; the immutable packet
index was already verified as a complete stream. Publish the candidate only on
success, preserving failed-seek cursor/decoder/poison state. Reads retain the
existing short-read-tail, silence/poison-on-fault and frame-capacity contracts.

Default FLAC/MP3/Vorbis construction and replay behavior remain unchanged.
`CompressedReadAhead` accepts either reader and clears its old generation only
after successful exact positioning with reclaimed matching endpoints.

## Validation and limits

Tests compare exact indexed samples to an independent PCM16/24 integer oracle
for 1/2/8 channels, crossing fixed packet boundaries, first/last frames and EOF.
A test-only decoder-call counter measures one packet per arbitrary seek across
33 packets, zero for EOF/invalid seeks, and shared index identity. Additional
fixtures exercise malformed duration/digest/truncation, unsupported inputs,
failed-seek/output-tail preservation, stopped-ring generation replacement and
allocation-free consumer handoff. Existing three-format replay/read-ahead tests
remain required regressions.

No MP3/Vorbis/Ogg-FLAC indexed seek, unknown-duration FLAC, file-backed indexing,
automatic worker/device clock, live streamed seek/loop, crossfade or physical
recording/deadline claim is added. Full-stream validation moves decode cost to
construction; it is intentionally an explicit trade-off rather than a default.

Primary backend APIs are the pinned [Symphonia packet definition](https://docs.rs/symphonia-core/0.5.5/symphonia_core/formats/struct.Packet.html)
and `symphonia-bundle-flac` 0.5.5 decoder implementation, reviewed from the installed
crate source. See the [audio-file guide](../audio-files.md).
