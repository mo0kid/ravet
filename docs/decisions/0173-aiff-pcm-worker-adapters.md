# 0173: AIFF worker adapters with shared PCM handoff mechanics

Status: implemented; scoped stream/worker/transport tests and lint passed.

## Context

Bounded AIFF streams supply the same worker frame reads/writes and stopped logical
seeks as WAV, but consumers previously needed to assemble ring buffering and
recording pumps themselves. Adding format wrappers should preserve existing WAV
and automatic playback semantics without duplicating backpressure/poison/drain
logic.

## Decision

Extract private prepared PCM read-ahead/recorder mechanics with worker-only reader
and writer traits. WAV and AIFF wrappers keep concrete format/reader/writer types
and delegate their bounded handoff operations. Public WAV methods, defaults,
capacities, output, failure and ownership contracts remain unchanged.

Add `AiffReadAhead` with matching-channel preparation, scratch within reader
capacity, at most one read per pump and retained suffix under ring backpressure.
Source EOF remains distinct from pending/queued audio. Read I/O rejection preserves
logical cursor/publication for explicit retry. Stopped seek requires the matching
consumer, positions before clearing and discards the old generation only after
source success. Implement the existing `ReadAheadPump` contract so explicit
`ReadAheadWorker` ownership and `BufferedPlayback` also work with AIFF.

Add `AiffRecorder` with matching-channel validation before header output to an
initially empty seekable sink. Pump drains at most prepared scratch capacity;
empty calls perform no file I/O. Preserve AIFF PCM16/24/32 conversion and writer
FORM/frame-count limits. Any write/validation failure after dequeue poisons later
pump/finish because source frames cannot be restored; partial output is not a
complete recording. Finish requires stopped producer ownership and drains at most
one ring capacity before explicit AIFF count/length/padding/flush finalization.
Drop neither drains nor finalizes.

## Validation and limits

Tests cover all three PCM precisions at 1/2/8 channels, exact sample order and
packet-independent backpressure, foreign/failed/successful stopped seek generation
semantics, reader I/O retry, callback transfer without allocation, bounded recorder
final drains and odd mono PCM24 padding. Nonfinite PCM and partial writer faults
exercise dequeue poison. Actual automatic AIFF playback/recall and existing WAV,
compressed and automatic fixtures cover shared-mechanics regressions.

Scratch remains 1..65,536 frames and ring capacities remain the existing prepared
power-of-two bounds. Stream I/O/preparation/destruction stay on control/workers;
no implicit device/path/rate conversion/physical recording/clock/deadline claim is
added. AIFF-C, other sample widths, metadata/loop markers and large-container formats
remain outside these adapters. See the [audio-file guide](../audio-files.md).
