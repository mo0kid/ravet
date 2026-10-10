# 0171: Automatic read-ahead ownership and prepared playback

Status: implemented; scoped concurrent tests and lint passed.

## Context

Explicit WAV/compressed read-ahead pumps already preserve unqueued suffixes under
backpressure and replace generations only through stopped matching-endpoint seeks.
Consumers otherwise need to repeatedly implement worker ownership and a callback
transport that distinguishes pauses, temporary source shortages and drained EOF.

## Decision

Provide a control-owned `ReadAheadWorker<P>` over an explicit worker-only
`ReadAheadPump` contract, with adapters for `WaveReadAhead` and
`CompressedReadAhead`. [Decision 0173](0173-aiff-pcm-worker-adapters.md) adds the
AIFF adapter using the same worker contract. `new` prepares matching empty endpoints and a paused
`BufferedPlayback`; `start` explicitly creates the automatic named pump worker.
A ring-full or unavailable-source attempt uses a 2 ms worker park timeout. No
callback-side notification, hidden device selection or file-path policy is added.

The worker exclusively owns its pump while active. A shared transfer mutex is
used only during startup/exit/control operations and never during pumping or
callback rendering. It keeps the prepared pump owned if OS thread creation fails.
Stop/join is cooperative between pump calls; it does not cancel active backend
I/O or promise a join deadline. Drop joins and therefore belongs on control after
callback shutdown. Explicit shutdown returns the source and any original pump
error; a panicked worker loses its unwound source and reclamation returns Poisoned.
Panic and backend/thread failure report Failed, never successful EOF.

The callback owns play/pause and source-frame position. Render fills audio plus
silence using only prepared ring storage and atomics. Paused renders consume
nothing; running-worker underruns advance only by actually consumed frames.
Known EOF/failure/stopped silence is distinct from running-worker underrun.
Invalid/oversized output shapes silence output and preserve cursor/queue/play
state. Empty output preserves play/pause. End requires both complete worker
publication (including retained suffixes) and ring drain. Earlier queued audio
before a pump fault remains available once before playback stops.

Stopped seek requires the matching callback endpoint reclaimed after stopping its
user. Check pair identity before joining, seek only with stopped worker/endpoint,
then restart. Source seek rejection preserves queued/pending/cursor/play state and
resumes the original generation. Success clears old audio, updates source position
and preserves play/pause. A subsequent thread-restart failure retains the source
but cannot roll back a completed generation change. `stop_stopped` positions at
zero and pauses while the new worker may prefetch.

## Validation and limits

Tests exercise concurrent backpressure, repeated paused/active allocation-free
callbacks, exact pause/resume/EOF continuation, gated running underruns without
cursor advance, malformed output rejection, foreign/failed stopped seeks and
successful indexed-FLAC recall. Worker fault/panic fixtures establish prefix
retention, original-error/source reclamation and failure distinct from EOF.

Sources must implement matching ownership, bounded whole-frame pumping and failed
seek preservation. Framework capacities remain the existing ring/scratch limits
and 1..65,536 render frames; backend allocation/I/O remains source-specific.
Playback runs at the declared source rate and channel order with unchanged sample
bits. There is no resampling, crossfade, streamed loop, live callback seek, command
mailbox, physical clock/drift, recording or device/DAW real-time deadline claim.
Worker status is a concurrent diagnostic snapshot, not clock synchronization.

See the [audio-file guide](../audio-files.md).
