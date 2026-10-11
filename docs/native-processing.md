# Prepared native planar processing

`ravet-native` supplies `NativeAudioAdapter<S>` for reusable native f32/f64
processing, independently of SDKs, operating systems and products. Consumer
format shims retain state codecs, format metadata, registration, editor ownership
and format-specific delivery. This crate stays outside the portable umbrella;
its foreign pointer operations live in one explicit unsafe module. The public
Gain ABI uses this adapter for both precisions while preserving its C signatures.

## Preparation and ownership

`NativeAudioAdapter::prepare(&mut processor, config)` deactivates a stopped
processor, checks bounds, allocates owned scratch/generated-event storage and
prepares that same processor. Rejection leaves it inactive. Use one main bus
plus at most eight ordered auxiliaries, each with up to 64 channels per direction,
including input/output-only buses. Maximum frames are 65536 and event capacities
1024 per direction. Owned sample/generated-event payload is at most 64 MiB,
excluding control metadata and consumer processor/input-event storage. Frame/event
caps check before byte arithmetic or allocation. Processor support may be narrower.

Preparation/drop/deactivation belong after callbacks stop. Rust control APIs do
not contain panics; consumer foreign control wrappers own that containment. Pass
the same exclusively owned processor to every processing/reset call. The adapter
owns no processor, state publisher, device or worker and cannot enforce consumer
callback scheduling through its type.

## Raw native processing

`NativePlanarBus<S>` describes channel counts and borrowed planar pointer arrays.
Its matching C layouts are in `include/RavetNativeAudio.h`; the header defines
no product symbols or registration API. `process_native` accepts buses in exact
preparation order, active frames, borrowed portable events and block-start
`Transport`. Zero frames permit null sample/pointer arrays but still require the
prepared bus counts and valid zero-offset events.

The unsafe caller supplies live aligned ranges and serialized ownership. Metadata,
events, processor and adapter storage cannot overlap writable samples. Sample
inputs may alias outputs across any channels/buses. The adapter validates shapes,
pointers and disjoint output ranges, then copies **all** inputs before any host
output write. Private preallocated planes form safe main/auxiliary views. Complete
portable event validation precedes processor invocation, including valid prefixes
followed by invalid events. Supplied transport is passed exactly, including unknown
playing and extended details; format serialization/validation remain consumer work.

Successful processing copies initialized output prefixes and returns typed generated
events borrowed until the next adapter call. Consumers perform native event
translation/delivery and own host refusal. Faults silence supported declared output
prefixes, retain tails and expose no generated events. Backend faults may retain
advanced processor history. A contained processing/reset panic poisons the adapter;
subsequent calls reject without invoking the processor, and reset cannot recover
it. Prepare a fresh adapter/processor configuration on control to recover. Normal
processing/reset avoids heap actions; exceptional panic handlers may allocate.
No native allocation or callback deadline guarantee is inferred.

## Public Gain integration

`ravet-gain-native` retains mono/stereo C handles, complete/legacy state codecs,
meters, parameters and serialized lifecycle. Its native input-event decoding uses the shared legacy parameter decoder;
product result values stay in the reference wrapper. Generic processing replaces
its private in-place scratch with separate copied-input/output planes: maximum
audio scratch is 1 MiB f32 or 2 MiB f64, plus its bounded event decoding storage.
ABI panic status remains 2; poisoned processing requires fresh preparation.
Generic schemas/state C marshalling, SDK registration and other-format
main/auxiliary/instrument integration remain separate work.

Tests cover f32/f64 timed audio, cross-bus aliases/tails, mixed input/output-only
auxiliaries, exact typed events and supplied transport, zero-frame flush, whole
input rejection, shapes/null pointers/output overlap, retained backend progression,
panic poisoning/repreparation, control bounds/refusal, instrument output, full-f64
audio/scoped event precision and prepared heap workloads. Existing Gain ABI
regressions exercise both migrated paths. See
[decision 0142](decisions/0142-native-planar-adapter.md).

## Native event conversion

`NativeEventV1` and `include/RavetNativeEvent.h` define one fixed v1 **in-process**
C record for every portable event kind. The record is 56 bytes on the validated
macOS arm64 ABI: eight u32 fields, one f64 value, four numeric u32 words. It is
native endian/aligned rather than a wire or preset format. Kind tags 1..15 and
expression/gesture selectors are explicit; optional channel/key/id scope uses
presence bits 1/2/4. Unset means wildcard; full-u32 IDs, including UINT32_MAX, are
ordinary IDs. Endpoint routing remains in the consumer shim.

Global values/modulation and note velocities use exactly widened f32 values;
decoding rejects lossy conversion, including a direct double 0.1 when an exact
widened float is required. Scoped values/modulation and expressions retain full
f64 precision. Volume accepts finite 0..=4; zero means mute, and signed-zero
bits survive. CLAP boundaries independently reject zero because its native
volume range is strictly positive; no ABI layout or kind selector changes.
See [decision 0159](decisions/0159-zero-volume-note-expression.md). MIDI stores validated short-message bytes in increasing shifts of
words[0]. UMP stores one complete packet of numeric words in specification order,
including reserved framed types. This supplies no SysEx assembly or protocol
negotiation. NoteEnd offsets retain portable delivery ordering semantics.

`from_event`/`to_event` convert individual records without block timing policy.
`encode_native_events`/`decode_native_events` accept at most 1024 events and use
caller-prepared destinations. They validate the complete stream's values,
offsets/order and capacity before writing anything. Same-frame order and tails
remain intact; zero frames accept only offset zero. Unknown tags/scope bits,
nonzero unused fields, malformed MIDI/UMP and invalid scalars reject atomically.
Used negative-zero scalar bits survive; unused value must be positive zero.
Successful converted slices borrow caller storage and allocate nothing.

`NativeParameterEvent`/`decode_parameter_events` share Gain's existing global
parameter layout and whole-stream preflight. Gain uses this decoder in both
precisions without adopting the tagged event ABI or changing its C signatures.
Consumers decode to prepared portable input storage, call the audio adapter, then
encode generated events into separate prepared native output storage before
host delivery. No generic exported factory/state/editor or additional SDK mapping
is implied. Rust tests cover every tag/scope, all UMP framing types, rejected
prefixes and 1000 complete f32/f64 native instrument cycles. C and C++ contracts
check the headers, legacy layout and numeric conventions on the actual build ABI.
See [decision 0143](decisions/0143-native-event-records.md).

## Native transport conversion

`NativeTransportV1` and `include/RavetNativeTransport.h` provide a separate fixed
96-byte block-start snapshot on the validated macOS arm64 ABI. Presence bits keep
playing/samples/beats/tempo/details/signature/increment/loops/bar independently
available. Boolean flags distinguish explicit false from unknown playing, and
absent details from known all-false details. Values retain exact i64 sample
positions/bounds, f64 beat/tempo/increment/bar values, i32 bar numbers and nonzero
u16 signatures. No fixed-point quantization, extrapolation or reset occurs.

`from_transport`/`to_transport` check finite beats/bar starts, finite positive
tempo, finite increments with known tempo, bar metadata with known beats, and
active strictly increasing loop bounds with their matching known timeline.
Time signatures need nonzero numerator/denominator without a power-of-two
restriction. An active loop can have unavailable bounds. Unknown bits, nonzero
unused/reserved fields and inconsistent dependencies reject rather than being
normalized. Used signed-zero bits survive; unused doubles must be positive zero.
Conversion allocates nothing and mutates no processor/state.

Consumers decode a supplied native record before passing its typed snapshot to
the adapter; failed conversion leaves DSP uninvoked when the caller follows that
sequence. Foreign rejection/silence and native host mapping remain caller work.
The adapter's direct typed-transport API retains exact pass-through behavior.
This in-process record is not a wire/preset representation or evidence of any
additional SDK/DAW transport support. Tests cover all 32768 availability/flag
combinations, extreme precision/ranges, invalid dependencies/canonical fields,
C11/C++17 layouts and 10000 prepared f32/f64 processing cycles per precision.
See [decision 0144](decisions/0144-native-transport-records.md).

## Exported processor entry

`NativeProcessor<P, S>::entry()` acquires a canonical static v1 table at runtime
on CONTROL for a consumer-selected `Default + Send + 'static` processor in native f32 or f64.
`export_native_processor_v1!(symbol, Processor32, Processor64)` exports the
consumer's entry symbol: 32/64 select matching tables, other values return null.
The public Gain library exports `ravet_native_processor_v1` alongside its existing
product ABI. The matching C header is `include/RavetNativeProcessor.h`.

Acquisition is deliberately non-const: exported and transitive companion paths
return the same table address across codegen units. All six companion kinds retain
exact originating-pointer checks. Move prior const expressions to CONTROL runtime
acquisition; see [canonical native tables](native-table-identity.md) for the API
change, independent actual-consumer validation and remaining lifetime obligations.

Check table version, byte size and sample bits before use. Create returns an
opaque exclusively owned handle, or null on construction panic. Keep the exact
origin table and library loaded until its handles/callbacks end; passing a handle
to another table/precision violates foreign safety. Stopped handles can transfer
to their audio owner because selected processors must be Send. Stop callbacks
before prepare/deactivate/destroy or control timing queries. No implicit locking,
worker or host scheduling is supplied.

Preparation consumes a fixed version-one configuration: main then up to eight
auxiliaries, complete capacities/rate/layouts, zero unused slots and reserved word.
It deactivates/replaces prepared storage on control and leaves rejected processors
inactive. Decoded input storage is separately bounded by 1024 portable events,
in addition to the audio adapter's sample/generated-event payload limit.
Processor-specific layouts and event support remain validated by the processor.

Process takes raw bus pointer-array addresses in the originating precision,
tagged native input records, optional native transport, separate generated-record
storage and one result record. Native output capacity must cover the full prepared
generated capacity before DSP, even when this block generates fewer events.
Whole input/transport conversion precedes DSP; successful generated conversion
preserves complete records. All inputs copy before output writes. Every valid
result pointer receives status 0 Continue/1 Silence/2 Fault and generated count,
which is zero on faults. Foreign sample ranges, metadata and output/result records
must satisfy the header's lifetime/alignment/nonoverlap obligations. Supported
raw output prefixes silence on faults and backing tails survive. Native output
conversion/delivery failure cannot roll back progressed DSP history.

Callbacks return 0 success, 1 rejection/backend failure, 2 panic/poison. Processing,
reset and control-query unwinding is contained; a poisoned owner rejects until
fresh stopped preparation. Reset/deactivation do not conceal poison. Normal
framework process/reset paths use prepared storage without heap actions; arbitrary
consumer code, aborting/destructor panics and exceptional handlers have no recovery
or deadline promise. Prepared timing queries preserve latency and None/finite/
Infinite tail declarations; consumers own restart/latency notifications and draining.

This is a generic foreign processor boundary. Parameter/state schema, product
metadata, SDK registration, editors and host event/transport conversions remain
separate interfaces and implementations. Tests use complete multibus f32/f64
entries, no-input instruments/completions, timing declarations, malformed prefixes,
capacity, unknown transport and factory/control/audio/reset/query panics. C++
contracts invoke the actual public Gain table, compare both precisions against
its legacy ABI over 2000 timed/cross-alias blocks, and check zero-frame controls,
rejected state prefixes and wide f64 samples. See
[decision 0145](decisions/0145-native-processor-entry.md).

## Native parameter and state companion

`NativeParameterProcessor<N>` is an optional **native facade** trait supplying
immutable global `ParameterSchema`, roles, product/version, complete snapshot and
atomic restore. `NativeParameters<P, S, N>::entry()` validates metadata on control
and returns a paired companion table, or None on invalid bounds/text/version or
provider panic. At most 1024 global f32 parameters are supported. Portable DSP
keeps its existing dependencies; Gain-native uses thin f32/f64 facade newtypes.
Its `ravet_native_parameters_v1` symbol selects the matching companion for the
processor entry. C layouts/callbacks are in `include/RavetNativeParameters.h`.

Check companion version/size/precision and retain its exact originating processor
table/handle/library. Metadata uses complete u32 IDs, physical f32 bounds/default,
linear/log scale and discrete interval count. UTF-8 names allow 255 bytes, units
63, without embedded NUL; arrays include trailing zero and zero unused bytes.
Oversized text rejects rather than clipping. Automatable/bypass/read-only flags
are declarations; consumer DSP and state code enforce behavior. Pure normalized/
physical conversion follows Foundation finite clamp/snap rules, including ties
rounding upward. Invalid IDs/nonfinite inputs preserve scalar outputs.

Snapshot/save/load calls require **exclusive stopped ownership**. There is no
live cross-thread DSP read, restore or publication. Framework helpers revalidate
provider snapshots against declared IDs/ranges and reorder by IDs before writing
native fields/canonical state. Snapshot destinations cover the complete parameter
count and preserve tails; save capacity covers the canonical byte count and also
preserves tails. Malformed snapshots/capacity leave destination data unchanged.
Snapshot/state-provider panics poison the shared base owner until fresh preparation.

Default state uses Foundation's complete little-endian product/version/count and
ID/f32 pairs. Full decoding/declared-schema validation precedes consumer restore;
unknown/duplicate/missing IDs, product/version/value/length errors reject whole
records. Consumers must apply validated restore atomically or leave state unchanged;
the framework cannot roll back arbitrary user code. An explicit decoder override
can migrate legacy records within an 8208-byte maximum. Gain's canonical 32-byte
version-two bytes match its existing codec, and its override retains validated
20-byte version-one migration with bypass off. No file I/O or automatic migration
is inferred for other consumers.

This global f32 schema/state companion supplies no scoped granularity or f64
parameter persistence, SDK registration, groups/text formatting/programs/editor,
automatic live publication or host notifications. The opt-in live extension below
provides a separate ownership contract. Tests cover log/stepped/role/UTF-8
metadata, limits/panics, reordered provider IDs, complete snapshots/state, malformed
prefixes and native f32/f64 recall/process/heap workloads. Actual C++ Gain contracts
compare canonical/legacy codecs, metadata/mapping and 2000 recall/audio cycles.
See [decision 0146](decisions/0146-native-parameter-state-companion.md).

## Explicit live global state

`NativeAudioParameterProcessor<N>` explicitly opts a native facade into prepared
audio-side complete base-state snapshots and atomic restores. The separate live
publication companion is available on targets with 64-bit atomics. Both hooks must avoid allocation, deallocation, blocking and I/O. They
exclude transient modulation and voices. `NativeState<P, S, N>::entry()` provides
the paired live table; Gain exports `ravet_native_state_v1`. Its layouts and full
foreign obligations are in `include/RavetNativeState.h`.

Create an independent publication object on control: null input with length zero
selects schema defaults; supplied state uses the consumer decoder with complete
cached-schema validation. The object owns cached immutable schema and fixed atomic
words, without a DSP instance. Controller publish decodes/validates complete state
before one attempt; concurrent snapshot/save observe the object, never the audio
processor. Global values remain f32 for either sample precision. Snapshot/save
preserve destination tails; save emits canonical state and explicit legacy decode
remains bounded by 8208 bytes. Creation, byte codec calls and destruction belong
on control; keep all callers stopped before destruction and retain the exact paired
types/tables/library throughout. No library unload or retirement queue is inferred.

Exclusively on the prepared audio owner, apply before processing and reflect after
processing. A newly accepted object identity/generation invokes complete restore;
the same revision skips it, preserving smoothing/history. Reflection revalidates
and reorders complete provider values by cached declared IDs. It conditionally
publishes only against the previously accepted revision, so a newer control restore
cannot be overwritten. Successful reflection updates the accepted revision, and
unchanged values do not create a new generation. Metadata providers are not called
on these audio paths. Reset preserves accepted revision; preparation/deactivation
clear it, so fresh preparation accepts current state again.

Codes are 0 success, 1 rejection, 2 panic/owner poison, 3 Busy and 4 Stale. Every
mailbox access makes one attempt without waiting or spinning. Keep the previous
accepted audio state on Busy, and arrange bounded retry outside callback waits.
Intermediate complete publications may coalesce. Failed calls preserve foreign
results. Audio-provider panics poison the shared native owner until fresh stopped
preparation; control decoder panics return 2 without accessing DSP. In-process
identity/generation are bookkeeping, never persisted protocol fields; their
relevant library/read/update lifetimes must not span wrap.

Rust tests cover complete validation, cached metadata, f32/f64 heap workloads,
apply/reflect revision semantics, stale restores, identity changes, rejection,
panic recovery and concurrent control/audio/observer coherence. C/C++ headers
check layouts, and the actual-library Gain probe exercises both precisions,
canonical/legacy state and retention of automation smoothing. These are bounded
contract tests; generic SDK registration, host notification policy and additional
DAW/platform/deadline evidence remain separate. See
[decision 0147](decisions/0147-native-live-state-publication.md).

## VST3 controller schema registration

`include/RavetVst3Parameters.hpp` is an optional C++17 consumer of an externally
supplied VST3 SDK. `ravet::vst3::registerParameters(container, nativeTable)` runs
on control and preflights the complete schema before adding SDK parameter objects.
It checks table/version/capacity, finite ranges/defaults, unique IDs, existing
container conflicts, roles and complete UTF-8 text. VST3 reserves `UINT32_MAX`;
that native ID is rejected here. Bypass requires a boolean 0..1 single-interval
range. Names must fit 127 UTF-16 code units plus terminator; supplementary Unicode
characters use two units. Oversize text rejects instead of clipping. SDK parameter
objects retain the originating native table/library; destroy them before unload.

`SchemaParameter` delegates pure physical/normalized mapping to the native table,
retaining the schema's f32 linear/log/discrete semantics on control. Mapping errors
or values unrepresentable as finite f32 return NaN. Numeric SDK formatting uses the
selected display precision and retains fractional discrete choices. Bounded numeric
parsing accepts complete locale-independent ASCII decimals/exponents with optional
sign and outer ASCII whitespace; invalid text, suffixes, overflow and nonfinite
values reject without changing output. Boolean bypass additionally roundtrips the
SDK's `On`/`Off` labels. Other discrete ranges use numeric physical text, including
two-choice ranges outside 0..1. Flags declare automation/bypass/
read-only roles; parameters use the SDK root unit. No parameter groups or custom
choice labels are supplied. Complete metadata/preflight failure preserves existing
parameters; SDK allocation/insertion exceptions do not have a rollback guarantee.

This helper does not call metadata providers on audio or translate audio automation.
Native factories, processing/event/state mappings, host notifications, editor and
program policy remain separate work. The SDK-linked contract fixture exercises
unrelated log/discrete/Unicode/role schemas, full preflight errors and actual Gain
native tables. Executed validation belongs in work status. See
[decision 0148](decisions/0148-vst3-native-schema-registration.md).

## Immutable cached parameter mapping

`NativeParameterCache<N>::new(schema, roles)` prepares immutable native metadata,
static schema/ranges and a sorted ID/index lookup on control. At most 1024
descriptors are supported; metadata retains native 255/63-byte UTF-8 limits and
declared order. All u32 IDs are preserved. Cached `info`, `normalize` and
`denormalize` use existing Foundation finite clamp/log/discrete/ties-up semantics,
without consumer provider calls, heap actions, locks, I/O or DSP access. Role
declarations do not suppress values implicitly. Cache creation/drop may allocate/
deallocate; shared readers must stop before retirement, and schema references/
library must remain alive. The safe Rust constructor does not catch provider panic.

`NativeCachedParameters<P, S, N>::entry()` supplies a separate paired companion,
keeping previous native table layouts stable. Entry acquisition and cache factory
run on control, containing provider failures; invalid metadata/provider panic
returns None/null without poisoning existing caches or DSP. Cached callbacks can
run concurrently on any owner and do not reacquire provider metadata. Gain exposes
`ravet_native_cached_parameters_v1` for both precisions; layouts and foreign
obligations are in `include/RavetCachedParameters.h`. Exact table/cache/precision/
library pairing, live aligned disjoint ranges and stopped destruction remain
caller obligations. Callback codes are 0 success and 1 rejection, preserving
failed output fields.

`NativeNormalizedParameterEventV1` stores offset/ID and f64 normalized intent.
Complete conversion validates known IDs, finite 0..1 values, ordered offsets and
destination capacity before mutation. Intent explicitly narrows to the global
normalized f32 contract; physical f32 widens exactly into tagged parameter-value
records. Equal-offset input order and destination tails survive. Conversion uses
two bounded passes and cached binary lookup, with <=1024 input/output capacity,
<=65536 frames and offset zero on zero-frame calls. Unknown IDs reject atomically;
format consumers explicitly filter/route unsupported IDs after host validation.
No scoped modulation, host queues/notifications/scheduling, arbitrary consumer
callback or deadline guarantee follows. See
[decision0151](decisions/0151-native-cached-parameter-mapping.md).

## Prepared VST3 parameter input

`include/RavetVst3ParameterInput.hpp` supplies optional external-SDK C++17 helpers.
Construct `CachedParameterInput` with a precision-matched native cache table on
control, retain the library and stop all borrowers before destruction. It caches
sorted target IDs/roles, rejects invalid entry shape/duplicate/reserved IDs, and
maps audio values through cached native ranges without provider calls. An explicit
read-only policy and Value/Unknown/Rejected outcomes leave routing/filtering to
the consumer. Nonautomatable flags do not implicitly suppress input. Failed or
unknown mappings preserve physical output. Normalized intent narrows to the
global f32 contract.

`visitValidatedGlobalParameters(changes, frames, visitor)` reads each native SDK
point once into <=16KiB stack scratch. All raw queue/total counts, getter results,
finite 0..1 values and per-queue ordered offsets validate before any visitor call,
including unknown IDs. Limits are 1024 queues/total points and 65536 frames;
zero-frame offsets are zero. Queue traversal and equal-offset order survive;
global sorting/merging belongs to the consumer. Failed raw validation invokes no
visitor. Visitor failure may leave private scratch changes, so defer output/state/
DSP publication until whole success. Native host and visitor callbacks must obey
the consumer's audio obligations; framework validation/cached lookup uses no
heap actions, locks or I/O.

IEEE finite checks inspect referenced memory as integer bits, preserving malformed
data rejection under fast-math compilation. Normal/fast-math tests inject foreign
NaN/infinity bytes instead of constructing disallowed by-value floating constants.
Gain's actual processor uses separate f32/f64 cache profiles and this walker,
retaining optional complete program expansion and stable bounded event insertion.
No continuous host-curve interpolation, gestures/scoped/modulation/performance/
transport mapping, generic SDK factory/output delivery or host scheduling follows.
See [decision0154](decisions/0154-vst3-cached-parameter-input.md).

## Direct prepared audio-owner state

`NativeAudioState<P, S, N>::entry()` is an optional companion for consumers with
an explicit controller publication/scheduling protocol. Control acquisition and
cache creation use its paired `NativeCachedParameters` entry; apply/snapshot use
the exact prepared `NativeProcessor` owner and immutable cache at matching sample
precision. Gain exports `ravet_native_audio_state_v1`; the C interface is
`include/RavetNativeAudioState.h`. Earlier table layouts stay unchanged.

Apply accepts exactly N physical f32 ID/value fields in any order. Cached binary
lookup rejects unknown/duplicate IDs, then complete canonical finite range checks
precede one atomic consumer restore. Role declarations do not implicitly filter
state. Snapshot validates complete consumer identities/ranges, reorders to cached
declared order before writes, and preserves unused output tails. Rejection leaves
outputs unchanged. Limits are 1024 fields/capacity; provider-state snapshot lookup
is bounded linear per declared ID. Native audio hooks are available without a
64-bit atomic requirement; the separate live mailbox still requires those atomics.

Both calls exclusively own prepared audio DSP and share panic poison until fresh
stopped preparation. Codes are 0 success, 1 rejection and 2 panic/poison. Normal
framework validation uses bounded stack/cached storage, without providers, codecs,
heap actions, locks or I/O. Consumers implement the explicit audio hooks under
these obligations, including atomic failed restores. Foreign live/aligned/disjoint
ranges, exact originating cache/processor/table/precision and loaded library
lifetime remain caller duties; retire after all borrowers stop.

Call restore only when state changes: repeated recall can restart smoothing. There
is no automatic publication, generation, notification, retry or scheduling. Direct
calls do not modify the live mailbox's accepted revision. Combining protocols
requires explicit consumer ordering/new publications or reprepare; new integrations
can use the independent live publication companion instead. State covers global
base values, excluding modulation/voices, with no consumer panic rollback or audio
deadline guarantee. See [decision 0155](decisions/0155-native-direct-audio-state.md).

## Prepared native telemetry

`NativeTelemetryProcessor<N>` is an optional facade contract for observations
separate from persisted parameter state. Declare immutable descriptors on control
and provide a complete fixed-size `audio_telemetry_snapshot()` under exclusive
prepared audio ownership. `NativeTelemetry<P, S, N>::entry()` supplies a separate
paired processor/cache table; entry acquisition and cache create/drop are control
operations and may allocate/deallocate. Invalid metadata/provider panic returns
None/null without altering existing owners. `NativeTelemetryCache` also exposes a
safe control constructor and shared cached `info` reads.

Descriptors preserve full u32 IDs, UTF-8 names<=255/units<=63 bytes without NUL,
and finite ordered f64 display hints. The cache rejects duplicate IDs/excess count
and invalid metadata before allocating, copies canonical metadata and builds a
sorted lookup. At most 1024 descriptors/capacity are supported. Shared cached info
uses no providers, framework heap, locks or I/O; retire after all borrowers stop.

Snapshot records preserve finite physical f64 values (including extreme/tiny and
signed-zero values), exact u64 measurement counts and a nonfinite-input-samples
flag. Raw values can exceed display hints. Complete known/unique ID, finite value
and known-flag validation precedes writes, with declared output order, failed
output preservation and successful tails. Consumer measurement/window/reset/
retention and audio hook obligations remain explicit. Normal framework snapshot
uses bounded stack/cached storage without metadata providers, codecs, heap actions,
locks or I/O. Codes are 0 success, 1 rejection and 2 snapshot panic/shared DSP poison
until fresh stopped preparation. Panic has no internal consumer rollback.

Gain exports `ravet_native_telemetry_v1` at both precisions; the C contract is
`include/RavetNativeTelemetry.h`. Its 12 fields (IDs10..21) match peak/RMS/nonfinite
indication for two input and output channels. f32 amplitudes widen exactly and
f64 levels remain native. Unused channels are zero; the Gain DSP retains its last
complete meter window on zero/rejected blocks and clears it on reset/preparation.
SDK display normalization remains explicit consumer policy.

Exact originating processor/cache/table/precision/library and foreign live,
aligned, disjoint ranges are caller duties. No 64-bit atomics, automatic UI/host
publication/scheduling, cross-thread DSP reads, state persistence/automation,
generated events, clearing or deadline guarantee is implied. See
[decision 0156](decisions/0156-native-prepared-telemetry.md).

## C++ native client

Apple-SDK consumers can adopt complete schema/control/text and isolated state
preflight through the [native AUv2 helpers](native-au.md). Their normalized-f32
parameter policy and CONTROL CF ownership are separate from processor precision,
rendering, SDK queues and host notifications.

`include/RavetNativeClient.hpp` supplies SDK-independent C++17
`ravet::native::NativeProcessorClient`. Construct it on control from the native
processor table and optional direct audio-state/telemetry tables. It validates
exact v1 shapes, needed callbacks, counts/precisions and exact originating
core table addresses, creates companion caches, then the processor. Invalid profiles
or factory failures stay invalid, with acquired partial resources retired on
stopped destruction. Tables/library/types remain alive throughout; no copy/move
or hidden state/mailbox/codec is introduced.

Companions must return the exact supplied core address. Byte-identical copied
core tables with equivalent callbacks reject before cache/processor acquisition;
use canonical runtime acquisition instead. This corrects the earlier weaker
callback-equivalence policy and does not authenticate arbitrary foreign code.
A violating foreign CONTROL acquisition/query exception is contained with balanced
earlier resource retirement; native callbacks/destruction must still not unwind.
See [exact client acquisition](native-client-acquisition.md). Exact type/library
pairing remains a caller obligation. Query `valid`, `hasAudioState`, `hasTelemetry` and capability counts
before using optional interfaces. Missing/invalid interfaces return rejection,
leaving their output fields unchanged.

Control preparation/deactivation/destruction and exclusive processing/reset/
timing/state/telemetry calls forward the underlying native contracts and codes.
Cached parameter/telemetry info supports shared reads. Foreign ranges remain live,
aligned and disjoint under those contracts; retire after all borrowers stop.
Normal forwarding uses no C++ heap actions, locks or I/O; native/consumer hooks
retain their real-time duties and result/silence/poison semantics. Complete recall
scheduling is explicit, with no inferred revisions, publications or retries.

Gain's actual VST3 processor uses separate precision-matched clients and tagged
global events. Its control setup prepares storage and marks state pending; the
first fully validated callback, including zero-frame flush, applies the current
complete published state. Post-process snapshot feeds its existing generation/CAS
reflection so newer controller restores win and unchanged state preserves ramps.
Separate telemetry retains meter IDs/display policy and host-mediated editors.
Program expansion, controller codecs/31-bit generation policy, SDK factory/buses/
editor and legacy public C exports remain consumer responsibilities. VST3
block-start transport uses the separate helper below; performance/scoped input
and generated-event mapping remain independent work. See
[decision 0157](decisions/0157-native-cpp-client.md).

## VST3 block-start transport

`include/RavetVst3Transport.hpp` provides external-SDK C++17
`nativeTransport(context, output)` and immutable `nativeTransportRequirements` for
setup-time project music/cycle/tempo/signature/transport requests. Availability
still depends on the host. Null context becomes canonical unknown transport;
non-null context validates finite positive sample rate, preserves exact i64
project samples, and reports known playing/recording/active-loop state. Flags gate
finite f64 musical position/positive tempo and positive u16 signature components.
Whole supported scalar validation precedes writes; rejected output survives.

Advertised cycle endpoints must be finite and active bounds ordered. Only active
bounds with a known musical position map into native v1. Inactive/positionless
bounds are omitted, retaining known loop state. VST3 bar start lacks an exact bar
number, so no native bar pair is invented. System/continuous time, SMPTE/frame
rate, clock/chord, tempo increment, sample loops, discontinuity and pre-roll are
unmapped. Unavailable/unmapped values and unknown future SDK flags are ignored.
No missing availability, sample rounding, extrapolation or automatic reset is
inferred. Used f64 signed zero survives and unused native fields stay canonical.

`include/RavetNativeScalars.hpp` centralizes reference-based IEEE integer-bit
finite classification, shared with cached VST3 parameter input. Normal/fast-math
fixtures inject foreign NaN/infinity bytes rather than floating by-value constants.
Mapping uses no callbacks, framework heap actions, locks or I/O; live/aligned
SDK-owned context and foreign output lifetime remain caller obligations.

Gain's SDK processor declares these requirements and maps context before complete
restore/parameter visitation/DSP/publication, forwarding canonical native transport
to the selected generic owner. Gain ignores musical timing, so delivery/composition
checks do not claim transport-driven DSP or DAW timeline behavior. Performance/
generated-event and further metadata/factory/bus integration remain separate. See
[decision 0158](decisions/0158-vst3-block-start-transport.md).

## Explicit VST3 performance input

Optional C++17 `RavetVst3PerformanceInput.hpp` consumes an external SDK event
list for one explicit bus. `nativePerformanceInput` translates one event into
one or two canonical native records; rejected/ignored results preserve the
destination and count. `visitValidatedPerformanceInput` reads each event once,
validates the complete raw list, and stages at most1024 expanded native records
before calling a visitor. Offsets are globally ordered; ties and expansion order
remain intact. Zero-frame offsets must be zero. Scratch needs56KiB plus bounded
local records; host/visitor callbacks retain their own thread obligations.

On/Off preserves f32 velocity without MIDI quantization and treats velocity-zero
On as On. SDK note ID-1 means absent; every other int32 identity, including the
SDK plug-in negative range, retains its bits in portable u32. Finite cents within
+/-12000 expand into semitone tuning after On or before Off when nonzero. Optional
note length is a hint only; no Off is inferred. SDK ppq/flags are unrepresented.
Poly pressure becomes channel/key/optional-ID scoped physical pressure. Standard
expression values are normalized0..1: volume maps to linear value*4 (including
zero mute), tuning to240*(value-.5) semitones; pan/vibrato/expression/brightness
retain f64. Expression events require a concrete ID; no voice state is inferred.

`UnknownExpressionPolicy::Reject` is default policy by explicit argument; Ignore
filters unknown kinds only after validating ID and normalized value. Other
selected types (SysEx, text, integer expressions, chord/scale, legacy MIDI output)
reject. Unselected payloads filter after common bus/timing/order checks. A visitor
refusal may retain earlier visitor effects: use private prepared scratch and
defer DSP/state/output publication until success. Gain has no performance input
bus and does not adopt this helper; a consumer must declare compatible buses and
expression metadata. No SDK instrument export, voice application, output delivery
or DAW evidence follows. See [decision0160](decisions/0160-vst3-performance-input.md).

## Explicit VST3 performance output

Optional C++17 `RavetVst3PerformanceOutput.hpp` maps canonical native records to
one explicit SDK event bus. `nativePerformanceOutput` leaves failed destinations
unchanged. Concrete On/Off and scoped Off with concrete channel/key retain exact
widened f32 velocity. ID-only standard expressions map physical volume/4 and
tuning/240+.5 to SDK normalized units; other supported scalars retain f64.
Tuning conversion may round through its floating unit arithmetic. Volume rejects
subnormal rounding/underflow using integer IEEE significand checks, including
under fast-math; tiny positive volume never silently becomes mute. Pressure
requires concrete channel/key, optional ID and exact widened f32 value.

Concrete SDK note IDs preserve int32 bits except UINT32_MAX, which would become
SDK's absent-1 sentinel and therefore rejects. Standard expressions require
ID-only scope because SDK has no channel/key fields for them. Wildcard/scalar
precision loss, note choke/end, MIDI/UMP/SysEx, parameters and text/int expressions
reject; no release is substituted for completion. SDK ppq/flags/length/tuning
metadata default to zero rather than being inferred from transport or history.

`deliverNativePerformanceOutput` preflights the entire bounded ordered native
stream before any `addEvent`. At most1024 SDK events occupy stack scratch. A host
can mutate its SDK arguments without changing native source storage. Host refusal
returns exact delivered/undelivered counts, retains rendered DSP and accepted
host prefix, and discards the suffix without replay/retry/rollback. Null host
accepts an empty stream only. Gain has no performance output and does not adopt
this helper; consumer bus declarations, metadata/voice policy and host validation
remain explicit. See [decision0161](decisions/0161-vst3-performance-output.md).

## Prepared VST3 native audio bus mapping

Optional C++17 `RavetVst3Audio.hpp::NativeAudioBusMap` validates/caches complete
native configuration on stopped control. Logical main/auxiliary buses retain
native order; zero-channel directions occupy no SDK bus, and input/output
indices compact independently. Fixed limits are1..9 nonempty buses,0..64 channels
per direction,576 total directional channels and1..65536 maximum frames. Failed
configuration retains the previous profile; consumer DSP preparation/resource
limits remain separate. Query input/output bus counts before SDK registration.

`map` validates the complete SDK audio shape at the selected native precision,
aligned sample arrays/pointers, address overflow and disjoint output prefixes
before publishing any native bus metadata. Inputs may alias outputs across any
bus. Bounded output-range sorting allocates nothing; no samples or silence flags
are changed by mapping. Caller owns live foreign metadata/arrays/samples and
keeps writable samples separate from metadata/events/private processor storage.
Flagged silence still requires valid pointers and never overrides sample data.

Zero-frame callbacks either omit both SDK directions entirely or supply exact
prepared directional counts/channel metadata; sample arrays are not read then.
Partial omitted directions reject. `silenceSupportedOutputs` clears only valid
aligned prefixes in exact prepared output shapes and supported frame
ranges at either known SDK precision (including mismatched processing precision),
skips null/misaligned/overflowing sample pointers, preserves tails and
sets bits for channels actually cleared. Unsupported metadata stays untouched;
foreign range ownership remains necessary even on rejection.

Gain now uses this profile before transport/queue staging/pending restore, and
uses bounded fault cleanup instead of directly walking rejected pointers. It
retains mono/stereo effect bus declarations; the helper's generic instrument and
auxiliary mapping is offline fixture evidence, not an exported SDK instrument or
speaker-layout/DAW claim. See [decision0162](decisions/0162-vst3-native-audio-buses.md).

## Public identified-performance reference

The separate [public native archive](../examples/performance-native/README.md)
wraps the portable 16-voice performance sine at f32/f64 precision. Its
`ravet_performance_*_v1` entries retain distinct consumer identity while reusing
these table layouts; exact originating library/table/handle pairing still applies.
Optional level schema/cache/direct audio state and native 64-bit-gated publication
are independent of core processing. State saves stable level ID 1 only, using
canonical product `RAVETPS\0`/version 1; complete recall retains voice history.

Prepare mono/stereo no-input buses with bounded input/output storage. Positive
output capacity explicitly enables original-target completion; zero suppresses
notifications without extending internal lifetime. Portable matching/synthesis/
rejection policy follows [the instrument guide](midi.md). State calls and rendering
are separate operations; consumer scheduling owns their ordering. This archive
has no SDK factory/controller advertisements, editor or device routing.

Build and validate its actual public C++ client independently:

```sh
CARGO_BUILD_JOBS=2 cargo build -p ravet-gain-native -p ravet-performance-native --release --locked
cmake -S tools/native-validation -B target/native-performance \
  -DRAVET_GAIN_LIBRARY="$PWD/target/release/libravet_gain_native.a" \
  -DRAVET_PERFORMANCE_LIBRARY="$PWD/target/release/libravet_performance_native.a"
cmake --build target/native-performance --target ravet_native_performance -j 2
ctest --test-dir target/native-performance -R '^ravet_native_performance$' --output-on-failure
```

The existing Gain archive is needed for configuration of the other fixture targets;
only the separate instrument target is built/run here. Tests cover actual-library
both-precision state/audio/identity/completion/lifecycle/tails and counted C++ heap
workloads. Offline checks do not prove SDK/DAW, device or deadline behavior. See
[decision0165](decisions/0165-public-native-performance-reference.md).
