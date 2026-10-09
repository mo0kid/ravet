# 0125: Buffered worker input-clock conversion

Status: prototype within explicit queue ownership and bounded conversion policy.

## Decision

Reuse the prepared clocked-duplex sinc/controller implementation with a private
channel-preserving relay. Convert raw capture to a separately configured graph-
rate output queue on an explicit worker. Match queue channels, nominal integer
rates and existing raw FIFO/target/sinc bounds at construction. Allocate one fixed
interleaved output block in addition to converter/internal/source/destination
queue memory. Destination backpressure retains exact suffixes without repeating
conversion or advancing raw/controller state.

Preserve existing target-occupancy priming and underflow/reprime policy. Report
fresh converted/published/pending frames separately; converted blocks may include
priming or shortage zeros. Expose existing independent converter/clock counters,
without clock timestamps or inferred transport/latency. Thread pacing, capture,
rate selection, graph/event routing and endpoint shutdown remain caller policy.
No hidden resampler/controller reset, wait, device, GUI or recovery scheduler.

## Evidence and limits

Portable tests cover priming, full/partial retained publication without reconversion,
channel/rate/FIFO rejection and 1000 heap-free 96k-to-48k stereo conversion rounds.
The public loaded native gain graph probe can insert this stage, checking 1000
fixed graph blocks, stereo relationships, settled constant levels, queue suffixes,
zero heap actions and owner retirement. This is synthetic offline rate conversion,
not independent physical clock/drift alignment or a deadline/long-session/hotplug/
platform guarantee. The short existing physical clocked-duplex observations do
not establish this separate graph-worker pipeline.
