# 0074: Incremental FLAC worker output

## Context

Complete-clip encoding does not provide an incremental recording sink. Recording
needs bounded retained blocks, explicit finalization and truthful partial failures.

## Decision

Add a worker-owned Write+Seek FLAC writer for an empty caller-chosen sink. Retain
one interleaved 4096-frame pending block, one backend buffer and one serialized
frame. Emit fixed blocks, keeping a short block until finish. Reuse backend sample
MD5 context and frame statistics; rewrite the fixed-size header and flush on finish.
Require explicit total-frame/output budgets. Reject invalid input before accepting
it; poison after encoding/output failure, with no rollback or hidden retry.

A high-dynamic-range five-channel PCM24 fixture exposed a backend prediction
frame count near 192 MiB. Pre-serialization size checks prevented that output.
Fallback to constant/verbatim encoding whenever prediction exceeds raw PCM plus
128 bytes, preserving samples and fixed offsets. Complete-clip encoding now uses
this same block implementation, also avoiding excessive whole-frame serialization.
Initialize actual minimum frame size from the first emitted frame, rather than
letting placeholder unknown size zero become the finalized minimum.

## Consequences and validation

No earlier encoded/audio frames are retained by the wrapper. Backend allocations
and execution time remain separate; the writer allocates and performs blocking I/O
on its selected worker. Float quantization, rings/devices/clocks and path policy
are not chosen. Drop leaves partial output; successful finish establishes a complete
nonempty stream. Failed I/O may leave an irreversible prefix or partial header.

Tests cover all eight channels/both precisions/partitioning/extrema and block tails,
input rejection preservation, byte/frame budgets, partial writes and final seek/
flush faults. Independent FFmpeg decoding exactly agrees with seven actual
incremental files, including the pathological prediction fallback fixture.
