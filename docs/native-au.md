# Native AUv2 consumer helpers

The optional C++17 Apple-SDK consumers in `RavetAuParameters.hpp` and
`RavetAuState.hpp` adopt existing native schema-cache and complete-state entries.
They do not own an AU component, processor, render loop or SDK scheduler. The
public performance AUv2 wrapper uses them, including actual-component validation.
Additional headers map prepared planar audio and collect validated block-start
host transport, with their independent ownership and availability limits below.
Apple/Steinberg SDK implementations remain outside this repository.

```cpp
ravet::au::NativeAuParameters<128> schema(ravet_native_cached_parameters_v1(32));
ravet::au::NativeAuState state(ravet_native_state_v1(32), schema.parametersEntry());
AudioUnitParameterValue canonical = 0.f;
if (schema.valid() && state.valid() && schema.control(id, value, offset, canonical)) {
    // Consumer checks its active frame/event budget, then queues canonical intent.
}
```

Construction/destruction, metadata and text calls require serialized CONTROL
ownership. Keep exact native tables, caches and their originating library loaded
through all calls and retirement. Construction validates the complete bounded
schema and owns its immutable cache and CF strings. The template bound is 1..1024;
schemas contain 1..that bound. IDs preserve full UInt32, including UINT32_MAX.
VST3-backed consumers retain that format's separate reserved-ID restriction.
Metadata is global normalized f32 with native normalized defaults, readable,
writable/read-only and non-real-time flags. Full UTF8 names/units become CFString
without String128 or legacy-name clipping. Bypass schemas require 0..1/one interval
and advertise Boolean; other two-choice ranges are not inferred to be Boolean.
Returned info names/units are retained for the caller: CFNameRelease is set, and
unitName when present also requires release. Returned strings remain valid after
helper retirement. No groups, choice labels, parameter registration or host
bypass-property routing is inferred.

`list` accepts null IDs/capacity 0 for count query. Complete capacity preflight
preserves array/count on failure, successful tails survive. `control` validates a
declared writable ID, finite normalized 0..1 and offset <65536, then uses cached
physical mapping and renormalization to canonicalize native log/step/ties-up
intent. It preserves output on failure and does not allocate, call consumer
providers, borrow DSP or mutate SDK storage/queues. Consumer active-frame timing,
shared event capacity, notifications and final setter mutation remain explicit.
These helpers currently require 32-bit native companions. AU parameter f32 values
do not acquire double precision from separate framework f64 DSP support.

CONTROL-only `format` uses canonical physical f32 text with shortest round-trip
numeric formatting; bypass uses On/Off. `parse` accepts complete <=128 ASCII code
units, surrounding ASCII whitespace and a single leading plus. It rejects
embedded NUL, Unicode/locale input, trailing junk, double signs, nonfinite and
physical out-of-range values, then applies native mapping/snapping. Unknown IDs,
failed mapping/CF allocation and malformed input leave caller outputs unchanged.
No formatted units or enum labels are appended or accepted. Text/metadata CF
operations can allocate and have no audio-callback deadline claim.

`NativeAuState` owns an isolated native publication object for CONTROL-only
validation. Its exact state companion must name the schema's same parameter
table, sample precision and count; f32 state bytes are bounded to 8208. Decoding
changes this private validator, never a live DSP handle/publication. `validate`
checks a complete AU dictionary: two valid native streams plus typed exact AU
version 0/subtype/manufacturer, and rejects partial-preset dictionaries. Explicit
consumer codec migration is accepted. Malformed state cannot mutate live state
before the caller delegates to SDK restore. Separate valid processor/controller
streams are allowed; voices/transient history follow the consumer codec.
`cohere` validates saved processor data before assigning it to controller state,
preventing persistence of an asynchronous stale controller cache. Malformed
processor data leaves the dictionary unchanged. No rollback of a later SDK/backend
failure, arbitrary stopped/live-state ownership or host bypass policy is promised.

AUBase intercepts parameter list/info and some built-in properties before virtual
GetProperty/SetProperty. Consumers must override the appropriate callbacks and
retain normal SDK dispatch buffer-size/instance-guard behavior. AUBase may
serialize a zero-initialized metadata structure after a failed parameter-info
query: helper output atomicity does not promise every SDK property buffer remains
unchanged on failure. Maximum-frames
preflight remains at the public performance wrapper's owned factory selector.
The performance reference delegates schema/list/info/control/text/state to these
helpers while preserving its existing note/render/lifecycle contract.

The native-helper fixture uses actual Gain/Performance native tables plus bounded
synthetic malformed schemas, full IDs, logarithmic/stepped mapping, a supplementary
Unicode name beyond String128, read-only roles, exact CF ownership, complete text
and invalid companion retirement. The actual AudioComponent probe exercises the
adopted list/name/text/control/state boundaries and existing timed mono/stereo
200-cycle rendering. Gain component contracts remain regression evidence. These
local macOS arm64 checks do not establish DAW/auval, sandbox/device, another
architecture/platform, all Rust/SDK allocation behavior or real-time support.
See [performance AU](performance-au.md) and
[decision 0182](decisions/0182-auv2-native-schema-control-state.md).

## Prepared audio bus mapping

`RavetAuAudio.hpp` adds `NativeAuAudioBusMap` and caller-owned
`NativeAuAudioBlock` fixed storage for planar f32 pointers. Configure the mapper
on stopped CONTROL with the same explicit native prepared configuration. A
rejected configuration retains its prior valid layout. It supports 1..9 logical
main/auxiliary bus pairs, each direction 0..64 channels, with at least one nonzero
direction per active pair; frames are 1..65,536. Version/reserved/unused layouts,
event bounds and finite positive rate also receive preflight. This maps buffers;
it does not prepare or own a native processor.

Native logical order maps separately to **compact** AU input/output element
order. A zero-channel direction consumes no AU element. For example, native
main input/output, input-only detector, output-only auxiliary and mixed auxiliary
map to three compact inputs and three compact outputs, while retaining all four
native bus positions. A consumer must explicitly register/configure its AU
element counts to this contract; bus-index arrays are not guessed to be compact
arrays, and host routing/discovery is not inferred.

`map` validates the complete supplied input/output pointer arrays and
AudioBufferLists before publishing a native block. Directional counts and list
channel counts must match the prepared layout. Every planar AudioBuffer has
exactly one channel; a single interleaved multichannel buffer rejects. Positive
frames require sufficient declared byte capacity, nonnull aligned sample pointers
and nonoverflowing address ranges. Inputs may alias outputs across any bus;
all output active prefixes must be mutually disjoint. Partial/full late-bus
overlap and malformed late inputs reject without changing prior mapped pointers
or samples. A successful map only materializes pointers and preserves samples
and backing tails.

At zero frames, exact supplied lists still require their planar metadata shape,
but sample pointers/capacities need not cover audio. Explicit omitted audio uses
zero supplied input and output counts; native declared channel counts remain with
null native sample arrays. Wrong nonzero counts reject even at zero frames.
Prepared mapping uses bounded scratch and sorting without heap, CF calls, locks
or I/O; native/consumer processing retains its own allocation/deadline obligations.
Foreign metadata/pointer arrays/sample ranges must be live, aligned, large enough
and disjoint from writable mapped storage; metadata must not overlap writable
samples. Address preflight cannot establish foreign memory ownership.

Keep the mapped block alive and exclusively owned throughout backend processing.
Its `buses()` views reference its own fixed pointer arrays; block copies retarget
those views to the copied storage. Do not remap while any processor borrows the
views. Mapping does not silence faults or notify a host. The performance wrapper
consumes it inside virtual Render before moving pending SDK parameter changes;
its existing caller render selector owns supported-prefix silence when AUBase or
native processing rejects.
The instrument also preflights raw supported caller output lists before AUBase
can substitute internal buffers or copy rendered audio. Alias rejection therefore
preserves live phase/envelope and previously queued accepted notes. Unsupported
or excessive requests retain the documented AUBase rejection/cleanup boundary.

The native audio fixture covers four ordered main/auxiliary directions, maximum
9-by-64 output mapping, full late-buffer faults and prior-map retention, copy
ownership, zero-frame policy and actual native Gain in-place/performance analytic
rendering and 2,000 maximum accepted/late-fault map cycles with zero counted C++
new/delete actions. This counter does not establish all backend/SDK/Rust heap
behavior. The actual packaged component retains its mono/stereo timed/control/
state/fault/200-cycle tests. These checks establish no generic AU host auxiliary
routing, f64 AU buffers, format negotiation, DAW/auval/device/sandbox or deadline
support. See [decision 0185](decisions/0185-auv2-prepared-native-audio-buses.md).

## Block-start host transport

`RavetAuTransport.hpp` collects a `NativeAuTransportSnapshot` directly from
Apple's `HostCallbackInfo` during the host's serialized AudioUnitRender. The
snapshot exposes read-only canonical native fields through `transport()` and
successful callback kinds through `callbacks()`. Missing callbacks and callbacks
returning an error produce unavailable tuples; successful independent tuples
remain available. TransportState2 takes precedence over legacy TransportState,
with no fallback after an error. Every supported supplied callback is called at
most once. Host callback/user-data lifetime and coherence across separate callbacks
remain host responsibilities; callbacks may allocate or block.

The collector initializes requested return fields to invalid scalar/Boolean
sentinels before invoking the host. Integer-bit initialization and IEEE finite
classification preserve these checks under fast-math. A callback returning
success with missing or malformed required values rejects the entire collection
without changing the previous output snapshot. Beat/tempo requires finite beat
and finite positive tempo. Signature requests only numerator/denominator: both
must be nonzero and fit u16, and the floating numerator must be integral before
conversion. AU clock/bar-start fields do not provide the exact native bar-number
pair, so the helper does not request or infer them.

Transport samples must be finite, exactly integral and in `[-2^63, +2^63)` before
integer conversion. Exact `-2^63` and the last representable Float64 below `+2^63`
are accepted; `+2^63`, the next value below `-2^63`, fractional and nonfinite
coordinates reject. Host Float64 precision is inherited, without recovery of
integers already rounded by the host. Playing, changed/discontinuity and cycling
are strict 0/1 Booleans; recording is known only from TransportState2. Active loop
bounds must be finite and strictly ordered; they become native beat-loop fields
only when a beat coordinate is also known. Inactive irrelevant loop bounds are
ignored. Native signature requires DETAILS in the canonical ABI, so signature-only
snapshots also carry DETAILS with unknown/default flags. Native DETAILS has no
per-flag availability: zero pre-roll for all callback profiles and zero recording
for legacy transport are unknown/default placeholders,
not host-known false values. Consumers needing the distinction must interpret the
`callbacks()` sidecar: only TransportState2 supplies recording, and neither
callback supplies pre-roll or native sample-loop coordinates. Do not treat every
false DETAILS flag as a host-known false value. No extrapolation, inferred
bar number or automatic voice reset follows a discontinuity.

The optional external-SDK `RavetAuVst3Transport.hpp` projects a successfully
collected snapshot into a caller-owned `ProcessContext`. A nonnull VST3 context
implies known playing and sample coordinates; the projector therefore returns
null unless both are actually known, leaving the prior context unchanged. Partial
beat/tempo/signature data remains available in the direct native snapshot but is
explicitly omitted from that bridge. Complete snapshots project music, tempo,
signature, playing, actual recording and available active beat-loop data.
Discontinuity and exact per-callback availability have no VST3 representation and
are omitted. Finite positive prepared sample rate is required; no system clock,
bar number, sample-loop or pre-roll is invented. Collection and projection use
fixed local storage without their own heap activity.

Performance AU collects and projects after audio/note preflight and before
moving pending SDK parameter intent or invoking native processing. Malformed
successful host callbacks reject with the existing supported-prefix silence,
preserving accepted pending notes/controls and native voice history for a later
render. Callback errors or absence are valid unknown transport. AUBase owns its
timestamp render cache, including caching a failed render timestamp: recovery
uses a new render timestamp; the helper does not promise same-timestamp retry.
The performance DSP currently ignores transport, so fixture evidence establishes
the boundary and retained scheduling, not transport-driven musical behavior.

Existing native performance zero-frame processing supplies authoritative Rust
codec acceptance for complete, signature-only, beat/tempo-only, legacy-only and
unknown collected snapshots. Corrupting signature dependencies, reserved fields
or unused coordinates rejects through that same ABI; no new native API is added.
The direct transport fixture covers independently available/error/absent callback
tuples, initialized missing returns, legacy/State2 precedence, native availability,
safe SDK omission, integer/fractional/nonfinite boundaries, signature/Boolean/
loop faults and 4,000 accepted plus rejected collections/projections with zero
counted ordinary C++ new/delete actions, also under optimized fast-math. Actual
packaged-component tests set typed host callbacks and check rejection silence,
queued timed MIDI/Level recovery, initialized returns and callback counts in both
mono and stereo. These are local macOS arm64 synthetic checks, with no DAW,
cross-callback host coherence, all-host/backend allocation or deadline claim.
See [decision 0188](decisions/0188-auv2-block-start-transport.md).


## Bounded native MIDI output

The SDK-independent C++17 `RavetNativeMidiOutput.hpp` converts canonical native
MIDI short messages and ordinary nonidentity NoteOn/NoteOff records into
`NativeMidiMessage`. `nativeMidiOutput` preflights one record and its active-slice
offset before publishing output; failed conversion retains the previous output.
Raw MIDI bytes remain exact, with the selected complete 1..3-byte defined channel
and system statuses, exact declared length, seven-bit data and zero unused fields.
Running status, undefined statuses, SysEx and long/pointer payloads reject. Raw
MIDI's unused scalar must be positive zero. Note velocities must be finite,
normalized 0..1 and exactly widened f32, preserving valid signed zero before
quantization. Channel/key are bounded before conversion. Unused IDs, scope,
detail, parameters and words must be canonical zero.

Notes quantize to nearest MIDI7 with ties upward. Positive NoteOn velocity clamps
to at least 1; exact-zero NoteOn remains wire velocity 0, which acquires MIDI1's
NoteOff convention at the receiver. NoteOff permits 0. This matches the existing
CLAP MIDI output policy and does not preserve zero-velocity portable On semantics
through MIDI1. Raw MIDI's wire NoteOn velocity 0 remains exact. Identity/scoped
notes, completion/choke, expression, parameter/gesture and UMP records have no
mapping and reject. NoteEnd is never invented as a MIDI NoteOff. The converter
owns neither processor output storage, endpoint routing nor whole-list delivery.

The external Apple/CoreMIDI `RavetAuMidiOutput.hpp` adds
`NativeAuMidiOutput<MaximumEvents = 1024>` fixed staging. The bound is 1..1024;
frames are 0..65536. It validates the whole canonical source prefix, ordered
offsets and capacity before publishing one complete legacy `MIDIPacketList`,
using CoreMIDI's packet layout and architecture alignment. Offset 0 is required
for zero-frame events; source order for ties survives even when CoreMIDI coalesces
same-offset messages into a packet. The packet list stays below CoreMIDI's 64 KiB
limit. Maximum staging declares about 28 KiB of local arrays; compiler, SDK and host
callback stack remains separate. No recursive flush, heap, locks, URI mapping or
logging belongs to the helper.
Legacy MIDI1 packet functions are separate from the newer UMP/EventList callback.

Every nonreentrant `stage` attempt invalidates delivery readiness first. Validation
failure preserves prior packet bytes/count for observation but cannot deliver
stale output from an earlier block. Each `deliver` attempt consumes readiness,
including missing callback or timestamp. Empty output succeeds without callback;
nonempty output requires an explicit host callback, render timestamp and MIDI
output index. Exactly one callback borrows the complete list. The result reports
attempted event count and callback OSStatus, never an invented host acceptance
count after refusal. Refusal may have irreversible host prefix effects; the
helper performs no retry, audio/DSP rollback or logging.

Staging and nested delivery reject while a callback borrows the packets, keeping
their bytes stable. Copy/move is disabled. Keep the owner alive and exclusively
owned through collection/delivery, and never destroy it from the callback.
Callbacks must not throw and may allocate or block. Callback/user-data and input
ranges must remain live; input/time/metadata must not alias writable storage.
Stop callbacks and serialize control when changing callback or routing metadata.
Foreign range ownership, native output lifetime, parameter/state scheduling and
backend failure atomicity remain consumer duties. A failed backend can already
have advanced DSP before output staging; no generic history rollback is inferred.

AU hosts install an `AUMIDIOutputCallbackStruct` and query a retained CFArray of
output names. Callback packet timestamps are **sample offsets** from the exact
AudioUnitRender timestamp, with an independently chosen MIDI output index;
audio-bus numbers are unrelated. The helper supplies no AU properties or output
registration, protocol negotiation or name/index agreement. Consumers own those
callbacks and must match their advertised metadata. Local AudioToolbox dispatch
normalizes legacy MIDI-output setter sizes/elements, installs a protocol adapter
and may handle subsequent setters without invoking the owned SDK setter. Exact
original-size/scope/element or stopped-state rejection at a raw SDK selector does
not establish facade input or mutation atomicity. The fixture separates those
boundaries and uses only valid host callback lifetimes through Render.

The public Performance AU continues to expose no MIDI output and suppress native
completion notifications; it does not acquire a generated-output port from this
helper. A separate synthetic in-process AudioComponent fixture owns output-name
and callback properties, invokes the actual public native processor, then delivers
a test-consumer generated queue through the helper during real SDK Render. This
proves callback/property/render ABI, retained CF metadata and explicit refusal/fault
policy, without claiming that Performance's Rust DSP generated those MIDI records
or that a new public MIDI-output factory is installed/distributed. Real native
Performance NoteEnd output is tested separately and rejects rather than becoming
a wire note message.

Strict normal and optimized fast-math fixtures cross-check canonical supported
records against authoritative Rust native processing and cover foreign nonfinite,
lossy f32, unused fields/status/length, timing/order/capacity and late faults.
Two thousand maximum accepted and late-fault packet-delivery cycles use zero
counted ordinary C++ heap actions. Reentrant stage/delivery cannot alter borrowed
bytes; fresh faults, success, host refusal and missing callbacks cannot replay
prior output. The actual synthetic component verifies 1024 messages in one exact
render callback, explicit MIDI7 values/offsets, successful silent native audio,
refusal receipts/no retry, pre-DSP late faults and recovery, and raw SDK versus
facade setter behavior. These local macOS arm64 checks establish no actual DAW
MIDI routing, devices, UMP/SysEx, production factory, all CoreMIDI/backend heap
behavior or real-time deadline. See
[decision 0197](decisions/0197-auv2-native-midi-output.md).

## Prepared audio precision and complete formats

`RavetAuAudio.hpp` provides explicit `NativeAuAudioBlockFor<Sample>` and
`NativeAuAudioBusMapFor<Sample>` for `float` or `double`. The original
`NativeAuAudioBlock` and `NativeAuAudioBusMap` names retain their f32 behavior;
`NativeAuAudioBlock64` and `NativeAuAudioBusMap64` select f64. The exact native
processor table and handle must use that precision. No sample conversion or
precision inference occurs. f64 mapping checks eight-byte capacities/alignment
and retains the same main/aux ordering, compact independent AU element indices,
whole preflight, output disjointness, zero-frame and copied-block ownership rules.

`RavetAuAudioFormat.hpp` adds `NativeAuAudioFormat` and `NativeAuAudioFormat64`.
Configure the complete profile on stopped control from the exact originating
native entry, native configuration and compact input/output ASBD arrays. Every
format must match its declared channels and the same finite positive sample rate:
native-endian packed floating PCM, one frame per packet, exact chosen sample bits
and bytes per frame/packet, reserved zero and noninterleaved channels. Mono may
omit the noninterleaved flag. Extra flags, incomplete directional counts,
misaligned/overflowing arrays and mismatched native table metadata/precision reject
before publication. Late rejection retains all accepted formats/configuration/map.
The table check validates structural metadata and required functions, without
proving arbitrary foreign-table origin or pointer lifetime.

Use a separate candidate for proposed element changes. Validate every directional
format before calling the consumer's SDK mutations, then publish the candidate
only after consumer success. The helper does not register elements, allocate SDK
buffers, notify hosts or roll back arbitrary SDK/backend mutations. In particular,
Apple AUBase copies the first 36 ASBD bytes and discards original reserved/padding
bytes; direct helper checks do not establish original-facade reserved/exact-size
rejection. An unchanged initialized ASBD may be accepted as a no-op before the
SDK consults format writability. Stop callbacks before changes/reprepare/state
control and preserve exact library/table/handle pairing. The processor entry
returned by a parameter companion is the authoritative origin for its handle;
address equality with a separate exported table must not be presumed.

Local strict normal/fast-math fixtures exercise both precisions, actual Gain64
values that f32 cannot retain, Performance64 timed analytic notes, late format
faults, maximum 576-output maps, copy retargeting and 2000 accepted/late-fault maps
with zero counted ordinary C++ heap operations. A separate actual in-process
fixed-f64 MusicDevice component validates stopped mono/stereo negotiation,
initialized changed-format rejection/unchanged no-op, native f64 AudioUnitRender,
pre-DSP caller faults with supported silence/tails/recovery and stopped complete
Level state recall. Its complete Level remains f32, voices/history are excluded,
and it adds no normalized AU parameter UI or cross-precision migration. These
fixtures do not register main/aux host routing or create a distributed f64 factory.

The public Steinberg-based Performance AU still prepares `kSample32` and uses
`channelBuffers32`; it remains f32. Migrating that wrapper requires explicit
precision-aware setup/render/state changes. No DAW/device, production host,
architecture beyond local macOS arm64, all-backend heap or deadline support follows
from these offline tests. See [ADR 0199](decisions/0199-auv2-native-f64-formats.md).
