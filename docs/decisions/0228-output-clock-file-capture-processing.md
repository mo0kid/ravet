# 0228: Compose buffered file transport with output-clock captured input

Status: accepted design; target-only implementation and focused validation,
public promotion/grouped validation pending. Work package: W215.

## Context and decision

The clocked duplex adapter already converts independent capture into the output
clock using prepared sinc/FIFO feedback. Buffered file output already owns its
unique callback consumer, but has no reusable downstream file+capture processor
composition or file transport mapping. A file queue measures disk/worker
availability, not an independent clock; feeding its occupancy into drift
correction would change file playback in response to backpressure.

Add an explicit buffered file duplex processor wrapper under the existing optional
resampled-playback feature. Own the original BufferedPlayback and f32 Processor.
Prepare child inputs as file channels followed by captured channels, at the file
queue/output rate. Outer captured/output counts are1..64 and combined child inputs
<=64; events, auxiliary buses and f64 are outside this profile. All scratch is
reserved before child preparation and bounded by32MiB, separately from source,
converter, FIFO, reader/ring and allocator storage. No new dependencies/defaults.

Use the existing PreparedClockedDuplex consumer and its clock configuration,
priming, underflow/reset and recovery contracts unchanged. Do not infer shared
clock origins from native timestamps or synchronize initial capture/file starts.
The wrapper is also a portable Processor over caller-supplied output-rate capture.

Render file PCM once per valid nonzero block and divide its consumed prefix from
a stalled zero-file suffix using at most two actual child calls. Capture/DSP run
in both. Child transport is the file OUTPUT-frame position and prefix playing
state, not original decoder coordinates, parent device frame count or hardware
time. u64 positions abovei64MAX become None in Transport without wrap/clamp.
Fresh preparation/stopped jumps publish one discontinuity; zero blocks preserve
state. Beat/tempo/details are not inferred.

Retain actual committed source counts when a later child segment fails. Such
faults map to outer BackendFailure, while first_error preserves the original
child ProcessError; forwarding a validation-like error would falsely promise
atomic rejection after source/earlier child history already advanced. Silence the
whole outer block and prohibit new file consumption/child calls until explicit
reset/reprepare. Reset pauses without rewinding queue/cursor and resets child;
prepare alone does not consume/reset file. A failed source drains its prefix with
ended=false and preserves live capture; source errors belong to worker shutdown.

Keep typed file/device/count telemetry and relaxed cross-callback monitor limits.
Valid device-render frames advance even during poisoned silence. File EOF does not
claim audible/DSP-tail completion. Child latency/tail exclude queue/converter delay.
Private Option owners let direct stopped into_parts deactivate/extract once; no
seek, read, join or hidden restoration. Explicit audio session hooks forward the
existing caller-owned lifecycle. Exceptional foreign panics are not normal receipts.

## Evidence and limits

Target tests use actual WaveReader/IntegerSinc/indexed worker and canonical clocked
duplex conversion, plus deterministic paused/backpressured public WAV pumps.
Independent integer capture production over±500ppm/44.1→48k profiles covers6000
variable64/128-frame callbacks per sign. Exact file-plus-capture PCM, bounded
correction/occupancy, zero capture underflow/overflow and heap-free rendering hold
for those synthetic profiles. Stereo file/capture grouping, finite EOF, running
source underrun, real NaN WAV worker failure, stopped failed/successful indexed
seek and reprepare, invalid full blocks/backing tails, once-only owners, original
child faults/poison, session forwarding zero-output layout rejection and32MiB preflight are independently tested.

Native clocked closure reclamation/live seek remains unavailable. Recovery must
explicitly rebuild sources and select restored state. No real hardware run,
shared timestamp alignment, physical drift/recovery, external MIDI sync, long-session
latency or deadline proof follows. Existing controller stability remains qualified
to configuration/fixture profiles. This closes the named reusable processing
composition, not every device/clock capability.
