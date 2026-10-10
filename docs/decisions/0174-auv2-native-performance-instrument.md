# 0174: AUv2 native performance MusicDevice reference

Status: prototype; actual packaged component contracts and default Gain bundle regression passed.

The public performance component reuses the validated native performance VST3
processor and external Apple/Steinberg AU SDK layers. A separate RAVET-owned
MusicDevice subclass/factory provides the format-specific scheduling and state
contract. Ordinary SDK MIDI key-derived identities would conflict across channels
with native full-identity ownership; ordinary MIDI therefore uses absent identity
and channel/key-scoped release. The SDK's wrapping small StartNote counter is
replaced with an explicit nonwrapping bounded generation/group/key identity policy.
Fractional pitch and floating velocity reach native tuning/velocity without MIDI
quantization. The selected profile remains legacy short notes rather than SysEx,
MIDI2, pedals, pitch bend or general expression/program support.
Full-width MIDIEvent words are validated before the SDK can narrow them to bytes;
defined non-note short messages are ignored and undefined/malformed/system-exclusive
input rejects without altering already accepted queued notes.

The fixed note queue holds at most 1,024 SDK events, with expanded native events,
parameter operations and one reserved complete-state slot counted before enqueue.
Accepted operations retain order; invalid callbacks reject before queue mutation
or ID output. Active-slice timing is checked before native DSP. Reset clears voice
history and pending notes but does not reuse opaque identities. These conservative
bounds and the instance-wide 1,048,575 generation limit are explicit reference
policy, not a universal AU identity service.

The external wrapper ignores underlying state-decoder failures and independently
saves an asynchronously refreshed controller stream. The owned adapter validates
both canonical native streams and AU dictionary field types/identity values before SDK restore,
and uses processor Level as the authoritative saved stream for both sides.
ClassInfo and ClassInfoFromDocument callbacks share the preflight. State excludes
voices/history. This is separate from the Gain AU host-bypass preservation policy.

AUBase can reject a request before virtual DSP and skips final copying on virtual
render failure. The owned factory render selector therefore delegates to AUBase,
then silences exact supported live caller buffer prefixes on any rejection, bounded
by request/preparation/65,536 frames and supplied byte sizes. It preserves tails
and skips invalid sample ranges; complete silence flags require every requested
supported sample to be cleared. The SDK's component instance storage layout is
used directly for dispatch. An initial callback-layout error crashed the probe;
a corrected dispatch and actual rejected-buffer regressions now pass. SDK sources
remain unmodified and outside the repository.

AUBase also handles maximum frames before the virtual property callback. The
owned property selector rejects capacities outside 1..65,536 before delegating
to its normal guarded SDK dispatch, preventing allocation or capacity mutation.
Stream-format validation restricts the reference to finite-rate mono/stereo f32
output. Actual component tests verify malformed capacities retain the previous
value and malformed formats reject before configuration.

`--plugin performance` extends the existing AU bundling entrypoint with Gain as
its default. The focused AU runtime reuses the shared VST3 pipeline and packages
only its selected public module, component and licence notices. No host directory
is installed. Local ad hoc signing is a development operation. Actual native
module/component probes exercise timing, velocity, fractional identity, channel
separation, state, capacity, faults, tails and repeated voice cycles. Those results
do not establish DAW/auval, sandbox, devices, other platforms/architectures,
real-time deadlines or production distribution support.
