# 0212. Convert bounded read-ahead into explicit output-rate queue frames

- Status: Accepted implementation; isolated and public focused validation passed; shared integration gates follow.
- Work package: W199.

Complete-file preparation requires finite whole-output memory. Native read-ahead
publishes at the original reader rate. Add bounded streaming conversion using an
explicit output rate, existing SincResampler and existing unique prepared queue
ownership. This does not silently change the rate attached to original PCM.

ConvertedReadAhead owns a frame-zero PlaybackReader, explicit fresh-reader
factory, unique producer and bounded source/converter/output scratch. Construction
preflights metadata, source/output limits, batches, ring ownership and allocation
budget before reads or factory invocation; failure returns original owners. Copy
requires equal rates. Sinc remains explicit even at equal rates; its causal delay
is (taps-1)/2 source samples, including the half sample. Nominal duration and retained
finite tail differ; retained output count is floor((N+taps-2)*O/I)+1. No hidden
resampling policy, gain adjustment, startup/tail trimming or whole-output PCM copy
is added. Encoded reader/backend allocations keep their own separate bounds.

Limit original source duration to 16,777,216 frames and rates to 1..768,000Hz,
ratio 1/16..16, even taps 16..256, channels 1..64 and batches 1..4096. Working
converter/source/output arrays have a 32MiB preflight ceiling. Decoder limits on
interleaved samples remain separate. These source-frame bounds do not establish
arbitrary-u64 file playback or long-WAV support. Unknown duration preflights a
conservative caller source maximum and rejects actual reads exceeding it.

One pump performs at most one source read, one converter step per channel and one
publication. Pending output retains its exact suffix. A full queue cannot read or
consume new input. EOF requires source integrity confirmation, bounded zero
padding and retained tail generation/publication; audible EOF also drains queue.
Original operational errors belong to the first direct Result or worker exit;
subsequent pumping is poisoned. Recover original advanced reader, factory,
producer and exact counts without rollback, retry or hidden I/O.

Distinguish original-input read/submission and zero padding from converted-output
generation/publication in typed progress. Existing ReadAheadPump coordinates are
queue-source frames: converted implementations use target-rate output frames for
rate/cursor/seek and ReadAheadProgress, while native implementations retain their
original rate. Its additive pure bounded worker_progressed hook defaults to new
publication; conversion can advance input without generating a frame. A full
unchanged queue must still cooperate with worker backoff.

Stopped seeks replay a fresh frame-zero reader candidate to the requested absolute
converted output frame. Preflight known upper bound, generation and matching queue
ownership before factory I/O. Preserve old phase, source/output suffixes, queue and
generation on every failed candidate. Commit only after candidate success, with
retained suffix and EOF integrity at endpoints. The caller factory must reproduce
the same source/version; metadata does not authenticate content. Candidate factory
side effects cannot be rolled back. Replay is O(target) control work without a
cancellation/deadline guarantee, and candidate plus existing state temporarily
require two separately bounded working allocations.

BufferedPlaybackOutput owns the existing unique BufferedPlayback rather than
manufacturing another consumer. Actual PreparedOutput configuration validates
rate/channel/render bounds before rendering; preallocated planar bridging and
atomic monitor snapshots contain no callback reader/converter work. Reset pauses
and retains cursor/queue. Reclaimed stopped processor control can seek using its
matching worker. Native open_output hides its processor endpoint: this package
does not promise inaccessible endpoint reclamation, live physical seek/recovery,
device clock mapping/drift control, cross-thread command mailbox, cancellation,
physical-device support or callback deadline validation.

The resampler's f64 recurrence is audited against exact rational coordinates for
the bounded profile. Global accumulated phase error is below 9.5463e-7 source
samples, less than 1/768000 and the explicit one-input-frame guard. Finite backend
endpoint arithmetic and ideal zero-support/spectral behavior are distinct claims.
Numerical recurrence across seven challenging rates and actual 16,777,216-frame
terminal impulse confirm exact retained count/consumption within this ceiling;
no broader-duration guarantee follows from these tests.
