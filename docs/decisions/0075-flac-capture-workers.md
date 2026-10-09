# 0075: Explicit FLAC capture workers

## Context

Incremental signed PCM writing needs an explicit policy to consume prepared f32
capture rings; neither device callbacks nor file writers should choose overload
handling or implicitly schedule recording.

## Decision

Provide a worker-owned FlacRecorder with prepared float/PCM scratch, selected
reject/clip overload policy and explicit writer budgets. Quantization rounds
nearest, ties away from zero, maps normalized endpoints to signed extrema and
uses no dither. Reject all nonfinite values before batch conversion/output.
A dequeue error poisons because ring data cannot be returned atomically. Preserve
encoder accepted-prefix reporting and producer-side backpressure policy.

Finish requires stopped/joined producer ownership, drains at most one ring
capacity and consumes the writer. No device, path, capture permission, clock or
hidden thread is selected. Drop does not perform finalization/file I/O.

## Consequences and validation

Clipping is consumer-selected rather than implicit. Rejected batches exclude
encoder acceptance but have already left the ring. Writer errors can leave an
irreversible accepted/output prefix. Backend allocation/deadline scope remains
separate from prepared producer handoff.

Four test groups cover quantization endpoints/ties/overload, stopped tail drain,
nonfinite and capacity poison, partial file output, and 8193 exact threaded PCM24
frames with measured heap-free producer handoff. A worker completion gate ensures
producer teardown before finalization. An explicit synthetic example writes two
actual PCM16/24 FLAC files matching independent FFmpeg decode byte for byte.
Physical recording and native sample-clock/device recovery remain unvalidated.
