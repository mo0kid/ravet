# 0181: Bounded optional single-worker Ogg Vorbis encoding

Status: implemented; scoped synthetic and independent-decoder validation passed.

## Context

Existing compressed decoding and FLAC output leave a lossy Ogg encoder gap.
A useful reusable encoder needs bounded wrapper input/output, explicit quality
and stream identity, error/finalization behavior and worker ownership.

## Decision

Add the default-disabled `vorbis-encode` feature over exact vorbis_rs 0.5.6 with
default features disabled. Use its safe API; vendored BSD libogg/aoTuV/libvorbis
compile through cc with supplied bindings, without system codecs, bindgen or
random serial selection. Preserve complete reviewed backend notices and exact
published source provenance. No unsafe code or Send assertion is added in RAVET.

Expose complete `encode_vorbis` and incremental `VorbisWriter` for explicit
mono/stereo rates, finite quality and caller Ogg serial. Restrict finite input to
[-1,1], reject overload instead of introducing clipping. Bound complete input,
per-call input, total frames, fixed prepared planar batches and cumulative sink
bytes. Preflight the whole call before acceptance; poison after a codec/I/O/
output-budget fault, preserving irreversible earlier prefixes and original I/O
errors. Counts distinguish submitted input from actual accepted sink bytes.

Keep mono 8000..96000 Hz and stereo 16000..96000 Hz. The selected backend's 8000 Hz
stereo maximum-quality fixture had sustained second-channel loss (whole 7.26 dB,
interior 6.98 dB; first channel 46.25 dB) in independent native decoding. Direct
backend/reframed outputs agree; lag/gain fitting and window-edge exclusion do not
repair it. The safe builder exposes no channel-coupling control. Reject lower
stereo rates before output rather than add unsafe/vendor changes, resampling or
hidden quality policy. Preserve the diagnostic evidence separately from valid
supported-profile whole-stream SNR assertions.

The backend owns raw-pointer codec state and is not Send/Sync. Construct, use
and destroy it on one worker. Do not weaken the framework unsafe ban or pretend
it implements the existing movable automatic RecordingPump contract. A worker-
created capture factory/owner is a separate design. Codec allocations and I/O
remain outside callbacks without a total-backend-memory or deadline guarantee.

The backend automatically writes EOS in Drop. Wrap its sink in a local owning
gate, disable writes before wrapper Drop, and latch first sink/output-capacity
failure, including every strict page-parser or partial-page flush failure, so
its retries cannot modify output before outer wrapper control resumes. Only explicit nonempty finish writes
EOS, flushes and returns the selected sink. Failure consumes the sink; Drop
does not produce a successful recording or perform framework sink writes/flushes.
User sink destructors remain their own contract. Empty writes must not reach
the backend because it interprets zero samples as EOS.

The backend's default large pages put first/EOS audio packets together for short
streams, making initial delay indistinguishable from final padding in independent
decoders. Request small pages and apply bounded strict output framing: identify
completed Vorbis identification/comment/setup packets, preserve packet/lacing/
body/serial bytes, place the first complete priming audio packet alone at granule
zero, retain exact final granule, and rewrite following sequences and CRC.
Buffer at most one 65,307-byte physical page with fixed header scratch; no sync
search or arbitrary decoded-PCM trimming. Headers/priming must terminate within
one physical page; continued headers/mixed header-audio/spanning priming reject.
Transformed bytes count against the same budget. Explicit finish rejects partial
buffered pages or missing EOS rather than returning truncated output.

## Validation and limits

Independent Symphonia and explicitly invoked FFmpeg native Vorbis decoding test
synthetic rate/channel/frame alignment and lossy PCM/SNR. Independent page parsing
checks CRC, serial, BOS/EOS, sequence and exact final granule across short tails.
Tests also cover partitioned writes, whole-call preflight, count/budget poison,
original partial I/O and terminal flush errors, normal/error Drop sink counters
and a compile-fail non-Send transfer contract. Private parser faults prove
that malformed header/sequence/mixed-page/partial-flush/capacity failures retire
the destination before backend retry and Drop, including before construction.
Accepted-profile 8193-frame native FFmpeg fixtures measured whole-stream SNR
37.88..48.11 dB and agreed with independent Symphonia PCM within 1e-4. Exact
frame counts include 1/17-frame tails; backend packet vectors remain unchanged
and tested transformed output adds exactly one 27-byte priming-page header.

No exact PCM, perceptual guarantee, MP3 encoder, automatic capture worker,
resampling, arbitrary multichannel speaker map, chained/multiplexed writer,
physical recording, cross-target toolchain or deadline claim. See the
[audio-file guide](../audio-files.md).
