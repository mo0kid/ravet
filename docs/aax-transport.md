# Native AAX transport capture

`RavetAaxTransport.hpp` captures individually available public SDK transport fields
into an immutable native snapshot. The owned native Gain and Performance models
use the versioned `AAX_CEffectParameters::Transport()` proxy created by actual SDK
initialization. They borrow it after checked prepare, detach before stopped SDK
retirement, and never call the unversioned MIDI-node transport method. CONTROL
serializes initialization, attachment and retirement; stop/join AUDIO first.

The collector queries playing, tempo, meter, ticks, ticks-per-quarter and configured
loop once per callback, then native sample location only when playing succeeds
true. The SDK defines this sample location during playback at the beginning of
current output buffers. Known stopped or unknown playing supplies no invented
sample position or timeline selection. Each getter is an independent observation;
this is not a coherent host tuple, tempo map or transport-triggered DSP reset.

The snapshot retains attempted/returned/available masks and original `AAX_Result`
statuses. Interpret a status only when its returned bit is set: a throwing getter
has an attempted bit and a distinct local fault, without a fabricated success or
SDK error. Errors make only their own fields unavailable. Missing interfaces can
still have a valid SDK proxy returning errors; all unavailable fields forward
`nullptr` to native processing. A missing proxy attempts no getter.

Successful malformed data or invalid native projection rejects the entire capture
and leaves its previous snapshot untouched. A separate result retains the current
attempted/returned masks, original statuses and local fault/getter identity.
Tempo must be positive finite, meter components 1..65535, and a successful tick
divisor positive even if its dependent fields are unavailable. Tempo starts with
an integer-bit NaN sentinel; meter/divisor start zero. Successful typed getters
must honor output writes: initialized bool=false/i64=0 cannot detect every
unwritten field or recover an invalid typed object representation.

Native sample positions remain exact signed i64. Raw tick positions, loop endpoints
and the u32 quarter-note tick divisor retain exact sidecars. Beat positions use
explicitly rounded f64 integer conversion/division across the full i64 domain;
there is no artificial ±2^53 raw cutoff or exact-fraction claim. Active configured
loops require exact raw start<end. When beats/divisor are known, the rounded native
endpoints must remain finite and strictly increasing; collapsed endpoints reject
rather than silently losing the range. Without known beats, the sidecar retains
the configured loop and raw endpoints, with no invented native loop endpoints.

Configured loop playback is not guaranteed actual execution or Loop Record.
Native DETAILS has no per-flag availability: unknown recording, discontinuity and
pre-roll remain zero/default placeholders even when signature or loop supplies
DETAILS. Consumers needing the distinction must interpret the getter sidecar.
No bar position, sample loop, timecode, timeline selection, inferred recording,
latency adjustment or extrapolation is provided.

The processor preflights the entire audio map, MIDI stream where applicable and
combined event capacity before collecting transport. Capture precedes complete
state apply, queue consumption and native DSP. Invalid transport returns local
process input status1, silences supported output prefixes while preserving backing
tails and known metadata, and retains accepted pending intent and DSP history.
Earlier map/MIDI/capacity rejection leaves the previous capture result untouched.
The stopped diagnostic methods therefore describe the latest **collection attempt**,
not necessarily the latest process callback; the existing atomic latest process
status remains separate. Read diagnostic copies only after AUDIO stops/joins.

`NativeAaxProcessor` and `NativeAaxEffectParameters` append `CaptureTransport=false`
to their existing template defaults; explicit native reference aliases opt in.
Stock Gain regression keeps its separate SDK wrapper path. Exact native provider,
cache and codec provenance is unchanged. Helpers use fixed stack/owner storage and
no helper heap or locks; foreign SDK getter allocation, blocking, lifetime/write
obligations and actual native/Rust allocations remain separate.

Local evidence includes strict normal/fast-math collection with
actual Rust native acceptance, full-i64/raw precision and original error/throw
cases,10000 allocation-counted full/late-fault capture cycles, plus actual SDK
Initialize proxies and independent Gain smoothing after late malformed transport
with pending complete restore. Fresh isolated external-SDK CMake builds pass eight normal/fast collector/model
contracts. Actual registered Gain and Performance callbacks consume initialized
transport; the packaged Performance module passes ACF exports/SDK version and
registered algorithm checks, including queued restore/voice-phase retention. Normal public native Gain22/22, Performance4/4 and stock14/14 package
contracts, mandatory actual registered algorithms/exports/version and eight Python
pipeline tests pass. No Pro Tools, production PACE signing,
whole-SDK fast-math, other-platform or real-time deadline claim follows from local
synthetic-host tests.
