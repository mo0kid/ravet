# Parameters, state, bypass and metering

Foundation now provides borrowed `ParameterSchema<N>` descriptions with permanent
IDs, names, units, defaults, automation metadata and validated physical ranges.
`ParameterRange` maps normalized values through linear or positive logarithmic
scales. Linear discrete ranges describe equally spaced steps (including a boolean
range with one interval). Finite mapping inputs clamp and snap; nonfinite inputs
return `None`. Mapping uses f64 intermediates and returns f32 values. At extreme
ranges, discrete choices remain subject to f32 precision. This is metadata and
value conversion, not a general host gesture dispatcher or parameter attachment system. The gain
CLAP product has a separate [editor control bridge](editor-control.md).

Schemas reject duplicate IDs, empty names and invalid defaults. Fixed-size
`ParameterState<N>` is tied to a schema and strictly validates every physical
value. State validation does not clamp or snap damaged records. A failed `set`
leaves values unchanged; a failed decode or migration returns no partial result.
All these operations use borrowed metadata or stack arrays and do not allocate.
Lookup, schema validation, decode and migration have costs proportional to schema
size (some are quadratic); select a bounded N before using them in callbacks.
No callback deadline is implied.

## Reusable record and migration

The exact-length little-endian record contains an eight-byte product identity,
a nonzero u32 schema version, a u32 field count, then u32 stable-ID/f32 physical
value pairs. Its length is `16 + 8*N`. Decoding accepts any field order but rejects
unknown or duplicate IDs, unexpected versions, incorrect counts, invalid values,
truncation and trailing data. Encoding validates destination length and version
before writing anything. It performs no stream I/O or allocation.

Products explicitly select a known old version and its schema before decoding.
`migrate` then matches stable IDs to a target schema: additions use defaults,
removals are dropped, and existing values must satisfy the target range. Unit
changes, ID renames and other semantic transformations require product-specific
migration code. Product identity and version selection belong to the consumer;
there is no automatic version registry, preset discovery or editor-state format.

## Portable gain product

`ravet_gain::parameter_schema()` describes gain (ID 1, 0 through 4, default 1)
and bypass (ID 2, boolean 0/1, default off). `GainProcessor::parameter_state()`
captures both requested values. `restore_parameters` revalidates against the
product schema before changing either value and snaps smoothing to restored gain.
Call it only from the thread that owns the processor, after receiving a complete
snapshot. For cross-thread state transfer, use the [snapshot mailbox](snapshots.md),
retain old data on contention, and apply accepted restores at callback boundaries.
The processor does not own a cross-thread restore mailbox.

`state::encode_parameters` writes a 32-byte version-two complete record using
the existing `RAVETGN\0` identity. `decode_parameters` reads version two or
explicitly migrates the existing 20-byte version-one gain record with bypass off.
The legacy `GainState` encoder/decoder remain version one. CLAP now advertises
both gain and stepped bypass, saves version two and loads both versions. Its
product [control bridge](editor-control.md) provides committed-edit gestures and
meter reads; a default macOS embedded gain GUI now supplies graphical controls. See [gain session
state](session-state.md) for the coherent host restore protocol.

Bypass parameter events switch before their sample. Values below 0.5 mean off;
values at or above 0.5 mean on. Nonfinite values reject the whole event stream.
Bypass copies dry input, with no crossfade, latency compensation or tail policy.
Gain smoothing continues while bypassed, so leaving bypass uses the gain reached
at that point. Zero-frame events update parameters without advancing smoothing.
Reset and reprepare preserve bypass; reset snaps gain and clears meter readings.
Abrupt bypass changes can click.

## Meter windows and transfer

`ravet_signal::PeakRmsMeter` accumulates sample peak and RMS over a caller-selected
window, using f64 sum-of-squares and a u64 sample count. Nonfinite samples count
as silence and set a flag; finite audio samples are not clipped. The meter stops
accepting samples at u64::MAX. Reset starts a new window. These are sample peak
and RMS, without true-peak detection, decay, loudness weighting or smoothing.

The gain processor exposes input/output readings per channel for the latest
successful nonempty block. Unused channels are zero. Empty or rejected blocks
retain the previous reading; preparation and reset clear it. Nonfinite audio is
flagged in telemetry without modifying the audio signal. Changing block length
changes the measurement window, so displays must choose their own ballistics.

On targets with 64-bit atomics, `GainMeterMailbox` transports all four readings
as one coherent 20-word snapshot. The audio owner explicitly calls
`try_publish(processor.meters())`; control/editor code calls `try_read()` and
retains the previous reading on `None`. Publication makes one attempt and may
fail immediately on contention. Intermediate blocks coalesce: this does not
retain every peak, schedule UI work, or guarantee delivery of every block.
Keep meter publication separate from parameter restore publication.

Tests cover mappings and range extremes, malformed schemas/records, ID-based
migration, legacy state migration, sample-timed bypass with continued smoothing,
stereo meter windows, output tails, faults and nonfinite audio. Heap probes cover
1,000 complete state encode/decode/restore, bypass, meter publication/read and
reset cycles. These are offline framework tests, not device or DAW evidence.

## Native f64 gain and metering

`GainProcessor64` provides the same mono/stereo gain/bypass IDs, schema, state
record, event timing and f32 parameter/smoother policy as `GainProcessor`. Its
sample copying/multiplication stays native f64; each f32 smoother value promotes
exactly before multiplication. This does not increase parameter resolution.
Reset/reprepare snap smoothing as before. Dry bypass retains exact f64 samples
while smoothing continues. Validation rejects before DSP state mutation; rejected
and zero-frame blocks retain the last complete meter snapshot.

`GainMeters64` exposes native f64 input/output levels. `PeakRmsMeter64` uses a
scaled sum of squares to avoid overflowing on finite f64 maxima and retain
subnormal levels. Nonfinite samples count as silence and set the diagnostic,
without sanitizing audio. At u64::MAX samples, further samples are ignored.
`MeterReading64::telemetry_f32` explicitly saturates finite levels to f32::MAX;
this telemetry conversion does not narrow audio or imply nonfinite input.
The existing f32 meter/mailbox and exported gain adapters remain separate APIs.

Tests cover sub-f32 audio differences, event gain/bypass, extreme finite RMS,
nonfinite metering, subnormal levels, rejected-state/tail preservation, zero-frame
edits, smoothing/state/reset and 10,000 prepared f64 workloads with no heap
operations. Native format mapping requires separate ABI/host evidence. See
[decision 0065](decisions/0065-native-f64-gain.md).

## Borrowed CLAP metadata registration

`ravet-clap::ClapParameterRegistration` validates a portable schema with an
ordered array of `ClapParameterOptions`, then provides complete `count`/`info`
metadata for a product's `PluginMainThreadParams` implementation. Preparation
borrows storage, without allocation; repeated metadata reads are heap-free.
Physical f32 bounds/defaults promote exactly to CLAP f64 plain values, including
continuous logarithmic schemas. Normalized editor mappings remain separate.

There are 0..1024 parameters. Names must fit 255 UTF-8 bytes and module paths
1023 bytes without NUL; the CLAP invalid ID is rejected. Discrete schemas need
unit-spaced integer bounds; fractional/coarser steps require an explicit separate
wire mapping. At most one bypass is allowed, with stepped 0/1 range. Automatable
flags come from the schema; `requires_process` is explicit consumer policy.
Explicit global/scoped modulation/automation flags are described below. No
cookies, value storage, display formatting, flush,
rescan or lifecycle scheduler is generated. The gain reference consumes the
registration and advertises that active gain/bypass changes require processing.

Acceptance checks complete metadata, continuous/logarithmic/negative integer
ranges, exact flags, wire/string/ID/bypass/capacity rejection, 10,000 heap-free
reads and the linked actual gain host extension. See
[decision 0073](decisions/0073-clap-parameter-registration.md).

## Immutable complete parameter programs

`ProgramBank<N>` borrows `ParameterProgram<N>` entries and the same validated
`ParameterSchema<N>` used for parameter state. Preparation validates all entries
strictly before registration: 1..128 programs, 1..1024 parameters, <=65536 values
total; complete canonical physical states, nonblank names <=255 UTF-8 bytes with
no NUL, unique stable `ProgramId` and unique zero-based MIDI bank/number address
(0..16383/0..127). Names may repeat; permanent IDs remain separate from indices
and MIDI addresses. Signed zero normalizes when obtaining a `ParameterState`.

`index` and `midi_index` perform bounded exact lookups; `state(index)` proposes
a complete copied state. Invalid/unavailable lookup yields None, preserving all
consumer state. Editing a returned state never changes the bank. Construction,
lookup and copying use no heap, locks or I/O. The bank has no mutable current
program or sound engine. Consumers decide atomic publication, identity after
edits, voice/history handling, host registration/notifications and persistence.
See [decision 0080](decisions/0080-complete-program-banks.md) and the separate
[MIDI request decoder](midi.md).

## Complete gain reference programmes

`ravet_gain::GAIN_PROGRAMS` and `factory_program_bank()` provide Unity, Silence
and Bypass as complete physical gain/bypass [1,0], [0,0], [1,1], with permanent
IDs 1..3 and MIDI bank 0/numbers 0..2. These are selection proposals; the portable
processors ignore raw MIDI program changes and have no current program identity.
Consumers restore a complete state or schedule both control events under their
existing lifecycle/timing/smoothing rules. The unchanged complete state codec
persists actual controls, including custom edits, rather than a catalogue index.

Native SDK consumers use `RavetGainPrograms.h`: immutable count and atomic copied
metadata include stable ID, complete controls/address and zero-padded 128-byte
UTF8 name. Results 0/1/2 mean success/rejection/contained panic. A failed call
preserves every output field; callers own live aligned writable ranges. Normal
validation/copy is heap-free with no locks/I/O or live DSP access. This reference
ABI does not generate host menu/selection/publication policy. See
[decision 0083](decisions/0083-native-gain-programs.md).

## Explicit parameter gesture events

`EventData::ParameterGesture { id, phase }` carries a raw global begin/end
boundary with stable parameter identity and sample offset. It changes no scalar
value and imposes no ownership, pairing or recording policy. Parameter graph
filters include matching boundaries; performance filters exclude them. Complete
worker event queues retain their identity, phase and order. Generated CLAP output
maps boundaries without a MIDI port; hosted graph output requires an explicit
opt-in and writable output mapping. See [the hosting guide](plugin-hosting.md#global-parameter-gesture-output)
and [decision 0121](decisions/0121-parameter-gesture-events.md).

`with_parameter_gesture_input(true)` on `ParameterEvents` or any CLAP bridge
also accepts raw native begin/end input; the default continues to ignore it.
Process offsets and same-frame order are preserved; flush offsets normalize to
zero. All native events still count against traversal capacity and timing checks.
No pairing/base edits or automatic recording are inferred. Hosted native nodes
use the same named builder option with an explicit writable input map; disabled
or unknown input rejects the complete input before any native call. See
[decision 0130](decisions/0130-parameter-gesture-input.md).

## Global modulation signals

`EventData::ParameterModulation { id, amount }` carries a finite signed physical
amount separately from saved base values. A new amount replaces the previous
signal; the consumer implements combining, clipping, smoothing, reset and
persistence. Parameter graph filters include the matching ID; performance filters
exclude it. Complete worker queues preserve order and reject non-finite amounts.

CLAP input translation is opt-in through
`with_global_parameter_modulation(true)` on `ParameterEvents`, `EffectBridge`,
`MainBusBridge` or `MultiBusBridge`. Default input continues to ignore modulation.
Only global targets map; native amounts must narrow to finite f32, and flush
normalizes offsets to zero. Invalid whole input rejects before DSP. Generated
output maps global amounts without a MIDI port and preserves host prefix refusal.
Exporters must advertise compatible native flags and explicitly implement DSP;
no reference processor support or saved-state changes are synthesized. See
[decision 0122](decisions/0122-global-parameter-modulation.md).

`ClapParameterOptions::global_modulation` (default false) advertises the native
modulatable flag for global physical amounts. It does not enable bridge input or
implement DSP; exporters explicitly align the registration option with their
processor and bridge. Hosted node input/output independently opt in to cached
writable/modulatable maps. See [the hosting guide](plugin-hosting.md#mapped-global-modulation-in-hosted-nodes)
and [decision 0123](decisions/0123-hosted-global-modulation.md).


## Scoped values and modulation in exported CLAP adapters

`ParameterValueScoped { id, target: NoteTarget, value: f64 }` and
`ParameterModulationScoped { id, target, amount: f64 }` retain finite full f64
physical values/amounts within a routed note input endpoint. Optional channel/key/
ID fields are matching wildcards. An all-wildcard target is still port-scoped;
global values/modulation keep their existing separate f32 events. Scoped events
neither edit saved base state nor combine/clip/smooth voices automatically. CLAP
consumers must implement its global/polyphonic override rules explicitly.

Prepared event queues retain all bits, and parameter-ID graph filters include
scoped events while performance filters exclude them. Exported translator/effect/
main/multibus `with_scoped_parameter_input_port(Some(index))` and main/multibus
`with_scoped_parameter_output_port(Some(index))` select independent 0..256 note
input indices. Generated scope targets originating input ports, independently of
note output/MIDI/UMP selectors. Defaults ignore scoped native input and reject
unrepresentable whole generated scoped streams before host prefixes. Invalid
selected raw fields/IDs/nonfinite values reject input before DSP; unrelated or
wildcard ports filter. Raw all--1 scope alone is global, so Clack negative-field
normalization cannot turn malformed scope into a global edit.

Scoped input preserves process timing and normalizes parameter-flush offsets to
zero. Generated scoped parameter flush is allowed at zero time; performance flush
restrictions do not apply to these parameter events. Native note IDs must fit
nonnegative i32. Missing scope routes/too-wide IDs/reserved parameter IDs reject
whole delivery while retaining successful audio/DSP; host refusal still discards
its undelivered suffix without replay. Native cookies are empty. Parameter ranges,
metadata, voice precedence, lifecycle and saved-state behavior remain consumer-owned.

`ClapParameterOptions::scoped_automation` and `scoped_modulation` independently
advertise `ClapParameterScopes { note_id, key, channel, port }`, all false by default.
`NONE`/`ALL` are const presets for static options. These flags enable neither
translation nor DSP and do not alter global automation/modulation flags. Current
public gain options use NONE. Tests cover every eight-flag combination, portable
ranges/queues/graph filters, exact native input/output/flush and malformed scopes
with heap-free prepared workloads. Hosted metadata validation is a separate
profile; no reference instrument/DAW/voice/deadline claim is introduced. See
[decision 0137](decisions/0137-exported-scoped-parameters.md) and the primary
[CLAP parameter flags](https://github.com/free-audio/clap/blob/main/include/clap/ext/params.h).


Hosted graph builders can independently enable mapped scoped input/output on a
selected CLAP note input port. Cached native scope flags, writable metadata,
range/step and native identity limits validate before input callbacks and during
output collection. This profile chooses the most specific target dimension
(ID/key/channel/port); global flags do not imply scoped support. Full-f64 scoped
process/flush values remain separate from saved base state. See
[mapped scoped parameters](plugin-hosting.md#mapped-scoped-parameter-values-and-modulation).
