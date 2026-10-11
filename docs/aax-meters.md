# Prepared native AAX meters

The external-SDK `AaxPreparedMeters<Count>` and `registerAaxMeters` helpers compose
native telemetry with explicit AAX host taps. `NativeAaxProcessor` and
`NativeAaxEffectParameters` append `MaximumMeters=0` to their existing template
arguments. Zero keeps the existing Context types, factories and processing
profiles. An opted-in count1..64 appends one `float **meterTaps` field to the
selected flat Context. The live field points to one readable pointer slot whose
value addresses Count writable contiguous f32 taps. Caller capacity, lifetime,
registration and stop/join obligations remain explicit; the adapter cannot prove
foreign allocation extents or memory isolation.

CONTROL supplies the exact selected32/64 native core/companions and immutable
`AaxMeterBinding` records. Each record names a host meter ID, native telemetry ID,
UTF8 name, SDK type/orientation/ballistics and projection. Host IDs are unique;
several taps may deliberately select the same native source. Preparation reads
all native telemetry metadata (1..1024 fields), validates full IDs/names/units/
finite ordered display hints, copies names and selected descriptors, and retires
one temporary telemetry cache. This unavoidable metadata-cache acquisition
precedes schema, codec, DSP and SDK mutation. Earlier malformed bindings/origins
refuse before that creator; late native metadata refusal/throw still requires
its balanced retirement attempt. A violating destructor throw cannot establish
resource reclamation. Native/SDK ABIs are otherwise nonthrowing.

`registerAaxMeters` uses the actual typed `AAX_FIELD_INDEX(Context,meterTaps)`,
ordered IDs and public `AddMeters`/`AddMeterDescription` APIs. It retains the
original first SDK refusal and completed description count; earlier SDK calls
have no rollback. The prepared metadata owner is deliberately noncopyable/nonmovable. The caller
keeps copied names/IDs alive through descriptor
borrowers. The Gain reference retains this immutable metadata for process
lifetime. No proprietary SDK layout, global registration or implicit host meter
UI is provided.

AUDIO admits the same lock-free owner token used by raw input/generated output/
timing profiles. Busy reentry changes no diagnostics, getters, samples or taps.
Before any node getter, queue consumption or DSP, whole tap-slot/tap spans must
be aligned, nonoverflowing and disjoint from known owner, Context/list/frame,
node, pointer-array and active audio regions. Late overlap silences only
independently admitted safe audio prefixes and preserves protected metadata,
all prior taps and tails. These known-range guards do not validate arbitrary
foreign pointers. Stop/join excludes lifecycle changes; the CAS is admission,
not a cross-thread lifecycle lock. Ordinary meter-only and raw profiles preserve
supported CONTROL→AUDIO SPSC scalar/complete-state publication. Existing stricter
generated/timing control policies stay explicit. Count>0 generated-output/editor
composition has callback syntax evidence only; no metered generated-output/editor
reference is distributed or runtime-qualified by these fixtures.

After successful authoritative DSP and any required native64 sample narrowing /
generated delivery, the adapter snapshots the complete native telemetry tuple,
validates all IDs/flags/finite values and prepares all projected words before
any host tap copy. `UnitClamp` explicitly clamps linear values to0..1;
`FiniteFloat` uses deterministic IEEE ties-to-even narrowing and rejects finite
f32 overflow. Signed zero and gradual subnormals follow the existing precision
helper. Display hints never infer dB or decay. Known native nonfinite-sample flags
are validated but not encoded into the scalar taps; audio's existing nonfinite
policy is unchanged, with no invented SDK clipped flag.

A post-DSP snapshot/projection refusal preserves every old tap and successful
host audio. Consumed commands/native history are not rolled back. Snapshot
poison2 or an unreturned violating snapshot throw makes future readiness
unavailable. Original negative native AUDIO refusal instead retains that code
and silences supported audio; an unreturned native throw is explicit and retains
the previous returned code. `meterDiagnosticsStopped` separates native/state
attempted/returned/status/exception from snapshot attempted/returned/status and
publication. Numeric1 before a first return is a sentinel, not an observed code;
status belongs to this block only with its matching Returned flag. Atomic
`lastNativeProcessStatus` retains the last returned code after structural
rejection/throw. All ordinary diagnostic copies require AUDIO stopped/joined.
No independent meter-diagnostics reset or automated decay is supplied.

Dynamic timing requires an explicitly supplied candidate telemetry companion.
CONTROL validates the complete field order and selected metadata against the
prepared meter bindings before candidate DSP/state ownership. Original late
candidate metadata refusal and temporary-cache retirement remain available in
`candidateMeterDiagnosticsStopped`; the old current DSP/history/taps are retained.
A matching host latency ACK permits AUDIO adoption and subsequent telemetry from
the new selected owner; old DSP/cache reclamation stays on CONTROL after join.
This is configured host compensation acknowledgement, not an invented host tail
API or seamless/history-preserving transition.

## Opt-in Gain reference

```sh
CARGO_BUILD_JOBS=1 python3 scripts/bundle-aax.py --plugin gain-meters \
  --internal-precision 64 --vst3-sdk /path/to/vst3sdk --aax-sdk /path/to/AAX
```

The default remains Gain without taps. The opt-in reference uses distinct
`org.ravet.gainmeters.mono` / `.stereo` identities, product0x52764d47 and native
IDs0x524d4731/0x524d4732, with the unchanged canonical Gain32/64 schema, complete
state, transport, smoothing and bypass DSP. Host audio and state scalars stay
f32. It has no custom editor or stock profile. Bundles are under
`target/aax/gain-meters[64]/release`; development signing/notices never establish
PACE authorization or Pro Tools acceptance. The authored external SDK bootstrap
exposes zero VST3 classes; owned AAX factory/algorithm registration is independent.

| Tap index | Host ID | Native source | Explicit projection |
|---|---|---|---|
|0|101|10 Input L peak|UnitClamp|
|1|102|11 Input L RMS|UnitClamp|
|2|103|16 Output L peak|UnitClamp|
|3|104|17 Output L RMS|UnitClamp|
|4|105|19 Output R peak|UnitClamp|
|5|106|20 Output R RMS|UnitClamp|

Gain meters are native block windows. Controls published after a callback samples
its queue tail are consumed by the following block; publication alone is not
current DSP adoption. Mono absent right-channel readings, bypass/reset/zero-frame
behaviour follow the actual native consumer. Host ballistics are explicit
NoDecay properties; the adapter invents no extra release/decay DSP.

Target-only qualification includes canonical Gain32/64 actual SDK startup,
AddMeters and owned factory→Initialize→callback analytic peak/RMS/bypass/reset,
distinct concurrent restore/trailing scalar and late alias/history tests.
Separate initialized SDK composition fixtures exercise32/64 note/raw instruments,
both detector/monitor bus orders and Delay8→16 candidate metadata/refusal/ACK/
adoption/retirement using frozen R4/W241 authoritative Rust archives. Their
public dependencies were adopted and the pipelines rebuilt from public sources. Normal/fast helper
checks include complete late faults and10000 maximum64-tap cycles with no ordinary
C++ new/delete. Compiler/host/Rust allocation and deadline guarantees are not
inferred. Package/module/Python evidence remains local macOS arm64; no Pro Tools,
production distribution, other architecture/platform or audio deadline claim.
