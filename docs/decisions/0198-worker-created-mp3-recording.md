# 0198: Worker-created MP3 capture with a shared local-codec controller

Status: Implemented; focused validation recorded below. Shared workspace gates follow this package.

## Context

The bounded MP3 writer required callers to manually pump an owned consumer and
invoke its seekable explicit finalization. Prepared input hides its unique
producer in the capture adapter, so a framework controller must accept the
existing permanent producer-retirement proof and reclaim partial sink/source
ownership after errors. The Vorbis controller already provided these ownership,
control and drain semantics for a genuinely non-Send codec.

## Decision

Extract those mechanics into a private factory/session controller. Only Send
sink/preparation/results cross threads; format is Copy + Send and codec session
has no Send bound. Initialize, pump, explicit finalization, abort and codec Drop
stay on the creating worker. Keep every public Vorbis type/signature unchanged.
Add an MP3 wrapper with independent status/completion/exit types and diagnostics
for accepted/dequeued frames and physical/file-region bytes.

Prepare validates configuration/channel/counts and source scratch before codec,
thread or destination actions. Explicit start acknowledges spawn; failed spawn
retains preparation for retry/abort. Pause/resume waits for worker acknowledgement
rather than racing command flags. Matching stopped or permanently retired
producer proof authorizes a bounded final ring drain. Foreign/live/leaked proof
preserves current state. Control waits and I/O have no deadline/cancellation
promise. Capturing while claiming a stopped producer violates caller lifecycle.

A private-constructed public completion enum separates Finalized, Aborted and
Failed(original error). Outer Ok proves reclaimed ownership only. The private
MP3 writer reclaim paths preserve the first original owned operational error and
partial sink/counts without changing public writer consuming-failure APIs. A
borrowed-error pump marker cannot replace the stored original error. Rejections
stop further queue consumption; failed controller finish reclaims without
additional drain or I/O. Healthy finish retains writer Info/LAME validation,
physical-write budget and explicit seek/rewrite/restore/flush contract.

Abort/Drop never implicitly finalize, drain, seek, rewrite or flush. Caller sink
Drop retains its own contract. Panic loses unwound local ownership and reports
Failed/Poisoned. A default-disabled device forwarding feature exercises the actual
PreparedCapture hidden-producer lifecycle without changing default codec graphs.
Writer format, loss/gain/gapless semantics, primary backend and independent LGPL
requirements stay applicable. No physical-device, target, perceptual, memory or
real-time deadline support is inferred from synthetic tests.

## Evidence and limits

The isolated all-feature library, manual writer/fault and both worker gates passed
49 tests; MP3-only gates passed 24 tests and Vorbis-only gates passed 17 tests.
Warning-denied all-target Clippy passed the all-feature and individual codec
profiles, strict all-feature Rustdoc passed, and all four doctests passed,
including the existing Vorbis non-Send compile-fail contract. A capacity-fault
fixture was corrected to admit a concurrent Failed pause acknowledgement after a
diagnostic dequeue observation, while retaining all original fault/count/suffix
assertions. Tracked focused validation then passed the same 49 audio-file tests, four actual
PreparedCapture tests, warning-denied all-target audio-file/device Clippy and
four doctests (57 focused tests/doc-tests total). Scoped formatting/diff checks
also passed. Broad workspace gates follow this package.

Tests cover independent exact MP3 file timing/PCM, concurrent bounded callback
publication without heap work, repeated pause/resume, foreign/live/stopped/retired
proof, untouched prepared abort, failed spawn/retry, startup seek recovery,
malformed source and total/physical output counts/suffix, partial write/metadata
seek/final flush original errors, file-prefix/end restoration, worker-only sink
actions, abort/Drop gates and truthful panic loss. Actual PreparedCapture tests
retain hidden producer retirement and counted overload. Physical CPAL shutdown,
capture timing, target portability and deadline guarantees remain unvalidated.
