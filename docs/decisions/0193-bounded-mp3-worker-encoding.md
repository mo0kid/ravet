# 0193: Bounded optional local MPEG Layer III encoding

Status: implemented; independent codec/parser/PCM and sink-ownership tests passed
in isolated and tracked focused validation. Physical capture and other
toolchains remain unvalidated.

## Context

Local MP3 decoding exists, but callers need reusable bounded file encoding and
an explicit finalization contract that preserves sink/error ownership. A
standalone MP3 file requires finalized delay/padding metadata, which can be
ignored by stream players even when file-aware decoders honor it.

## Decision

Pin default-disabled mp3lame-encoder 0.2.5 plus mp3lame-sys 0.1.11, bundling the
unmodified LAME 3.100 encoder without decoder/frontend features. Use only safe
provided f32 mono/interleaved and prepared Vec APIs; add no framework unsafe,
process-global codec settings, external executable or implicit system library.
Record complete independent LGPL/component notices and exact archive provenance.

Initially support explicit 192/256/320 kbps CBR at highest quality, mono/stereo
and exact 32/44.1/48 kHz MPEG1 profiles. Source must be finite normalized [-1, 1];
whole caller-call preflight precedes codec mutation. Do not silently resample,
remap, add metadata, normalize or compensate gain. The backend's 192 kbps preset
scales by 0.97; 256/320 presets use 1.0. Document measured raw and gain-fitted
results separately.

Use prepared 4,096-frame encode batches with 12,320-byte maximum output scratch,
4 KiB tag scratch, 65,536-frame call limit, 16,777,216-frame total limit and
64 MiB actual write budget; complete encoding accepts at most 4,194,304 samples.
Backend-owned working memory remains outside adapter bounds.

Require current sink origin at EOF in Write+Seek incremental output. Successful
non-EOF rejection restores origin; actual seek error returns original error and
sink immediately, with no hidden retry/restoration. Caller owns prefix/truncation,
concurrent access and downstream sink destructor policy.

Explicit standalone FlushGap obtains the finalized reserved first Info/Xing
frame. Parse its MPEG profile, identity, pinned LAME version, tag CRC, signaled
frame count, bytes, encoder delay and padding. Verify source-frame and actual
file-region relation, then replace rather than prepend the original reserved
frame, restore exact end and flush. Count physical rewritten bytes in the budget;
file-region length differs. Gapless-aware completed-file decoding gives exact
source frame counts; arbitrary stream playback may retain padding.

Separate retryable owned preflight rejection from retained borrowed original
operational error. Operational faults poison further encoding and sink I/O.
Constructor/finalizer return private-constructed failure with recovered partial
sink/original error/source and file/write counts. Accepted source can advance
before a partial sink fault. Abort/Drop do not encode, seek, write, flush or
finalize; convenience abort intentionally discards diagnostics. Poisoned finish
recovers diagnostics and original error without I/O. Codec destruction only
closes its own state, without touching the sink.

## Evidence and limits

Primary API/source audit and isolated build, parser/timing/PCM/sink fault tests
passed. All 72 mono/stereo/rate/bitrate/tiny-tail profiles yield exact counts in
Symphonia and explicit FFmpeg native completed-file decoding; maximum decoder
PCM difference is 3.359e-5. Normal 192 kbps raw per-channel SNR is 30.404–30.474 dB,
256/320 raw SNR 52.027–71.121 dB; gain-fitted residual is at least 50.802 dB.
File-versus-stream padding and the backend's 192 kbps preset attenuation remain
explicitly documented.

No physical capture, host validation, other target/toolchain, subjective
listening, deadline or total codec memory guarantee follows. Optional components
retain their own LGPL obligations; RAVET commercial agreements do not relicense
them or certify distribution/patent compliance. Automatic capture integration,
ID3/VBR and other rates/bitrates remain separate work.
