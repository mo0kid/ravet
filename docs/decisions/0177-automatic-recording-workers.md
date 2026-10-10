# 0177: Automatic recording workers with explicit finalization ownership

Status: implemented; scoped tests, lint and formatting passed.

## Context

Existing WAV/AIFF/FLAC recorders bound capture-ring drains and expose explicit
stopped-producer finalization. Consumers otherwise duplicate worker ownership,
empty-ring polling, error reclamation and capture-stop/finalization sequencing.

## Decision

Add `RecordingWorker<P>` over an explicit `RecordingPump` contract with adapters
for existing WAV/AIFF/FLAC recorders. Validate matching producer/consumer identity
on preparation and retain an owning private queue token for later preflight.
Preparation creates no thread; explicit start drives bounded recorder batches on a
named worker, polling with a 2 ms park timeout when empty. Capture callbacks retain
ordinary unique prepared `AudioProducer::push`, including accepted-prefix full-ring
policy. No callback allocation/lock/I/O/device/path/clock is introduced.

Extract a private shared cooperative source-worker owner used by recording and
read-ahead; preserve all read-ahead APIs and semantics. Transfer mutexes are only
used during startup/exit/control. OS spawn failure retains the prepared source.
Worker panic reports failure and loses the unwound source. Stop/join is cooperative
between pump calls and does not cancel backend I/O or promise deadline. Drop joins
on control, with no implicit drain or finalization.

`finish_stopped` requires the matching unique producer reclaimed after capture
user stop/join. Reject foreign identity before stopping. Join the worker, then
invoke the recorder's existing bounded final ring drain and consuming finish on
control. Publish Finalized only on source finish success; repeat start/finish
reject. Producer publication during finalization violates the ownership contract
and can reject or lose late frames.

Also support recorder-owned queue preparation and finalization after permanent
producer retirement. The stable exact owned consumer supplies queue identity;
unique AudioProducer Drop publishes atomic retirement, with no public resurrection
API. Acquire retirement synchronizes the last publication. Reject live/leaked
producers before stopping, then join and perform the bounded remaining drain.
This accepts PreparedCapture/open_input adapters that hide the producer inside
callback ownership, without requiring manual recorder recovery/finish. A stopped
or paused device is insufficient unless its producer is actually reclaimed or
retired; backend/device destruction timing remains an external obligation.

Retain original pump errors and the owned poisoned recorder. Shutdown reclaims
both without finalizing. Finish may return a saved pump error once while retaining
the source for shutdown; no duplicate error follows reclamation. A source
finalization failure consumes its sink and is terminal without a complete-file or
retry guarantee. Frame counts remain backend-specific: WAV/AIFF successful writes,
FLAC accepted encoder frames including pending block; neither implies finalized
recording.

## Validation and limits

Tests cover concurrent synthetic capture/exact output, accepted-prefix overload,
callback heap counters, stop/restart/final drain, matching identity, original
partial-write and invalid PCM errors, source reclamation, final flush failure,
panic-versus-success state, permanent producer-drop/queued-prefix proof, actual
hidden-producer PreparedCapture integration and AIFF PCM24 odd padding. Optional
FLAC capture tests cover explicit clipping versus rejection, nonfinite input and
total budget poison.
Read-ahead/automatic/WAV/AIFF/FLAC regressions cover shared-worker extraction.

Existing recorder scratch/ring/format/output bounds remain unchanged. Physical
capture/routing, device stop automation, long-session clock drift, resampling,
producer drop diagnostics and audio/device/join deadlines remain separate.
Custom sources own bounded pumps, matching queue and dequeue-poison/finish rules.
See the [audio-file guide](../audio-files.md).
