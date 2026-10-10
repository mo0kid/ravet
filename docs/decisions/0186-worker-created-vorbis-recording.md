# 0186: Worker-created Vorbis capture ownership and explicit recovery

Status: implemented; focused worker/writer and PreparedCapture tests, lint, and API documentation tests passed. Physical device shutdown and recording deadlines remain unvalidated.

## Context

The optional Vorbis writer is genuinely !Send and cannot be moved into the
existing automatic RecordingPump. Safe capture integration must create, use,
finalize and drop the codec on one worker while preserving partial sink/source
ownership after operational faults.

## Decision

Add a dedicated VorbisRecordingWorker over transferable Send preparation and
results only. Keep codec state local; no unsafe Send assertion, generic movable
recorder claim, callback mutex/I/O/allocation or hidden capture policy. Validate
format/queue/budgets and prepare bounded source scratch before explicit start;
worker alone writes codec headers. OS spawn failure retains preparation.

Use acknowledged pause/resume with atomic commands/status, ensuring no ring
consumption after successful pause until resume/finalize. Resume acknowledgement
prevents a stale Paused status from satisfying a following pause while a resumed
batch is pending. Control can block on active sink I/O and has no deadline.

Preflight matching reclaimed producer or permanent retirement before finalizing.
Drain at most the stopped fixed-ring tail, then explicit EOS/page-completeness/
flush on the codec owner. Deliver recovered sink and consumer even after startup,
pump or finalization errors using private-constructed completion evidence:
Finalized, Aborted, or Failed(original error). An outer Ok proves recovery, not
file completeness. Diagnostic accepted/dequeued/written counts remain distinct.
Failed pumping stops dequeue and retains the original error until outcome.

Private initialization/finish/abort recovery helpers preserve the existing public
VorbisWriter consuming-failure signatures. Gate Drop before extracting a sink;
abort/ordinary Drop never final-drain/EOS/flush. Panic unwinds thread-local owners
and reports Failed/Poisoned, never synthetic sink recovery. Transfer locks occur
only around preparation/result ownership, outside callbacks and normal pumping.

Forward the optional encoder through default-disabled ravet-device
vorbis-recording for focused PreparedCapture hidden-producer integration.

## Validation and limits

Focused synthetic tests cover concurrent bounded capture/backpressure, acknowledged
pause/resume, exact identity and permanent-retirement preflight, stopped tail drain,
independent decoded PCM/timing, single codec-owning sink thread, callback heap
counts, original startup/pump/flush faults and partial ownership, abort/Drop,
panic and injected spawn failure. Writer regression tests preserve consuming
new/finish behavior and !Send compilation. No physical device/CPAL-shutdown timing,
real-time deadline, perceptual/bitexact PCM or backend-memory ceiling claim.
