# ADR 0222: Explicit prepared native AAX audio bus mapping

Status: Accepted; implemented prototype with local SDK/native/package validation.

## Decision

Use an owned fixed planar f32 mapper with explicit native bus source order,
input/output metadata capacities and static or dynamic sidechain input slices.
Prepare the entire layout in the owned native processor rather than hiding a
single-main owner behind a detached generic helper. Preserve the existing effect
and MIDI instrument context layouts through default template arguments; select a
separate effect multibus context explicitly. Pair that context with authored SDK
sidechain/aux descriptors and exact initialized public controller metadata.

Adopt a reusable portable bus-routing processor as the authoritative native
consumer. Its main gain/bypass and detector-to-aux monitoring are explicit public
reference behavior; no proprietary algorithm or audible history marker is added.
A separate existing native telemetry API supplies the transient frame-count oracle.
Actual SDK registered callbacks must process both input-only/output-only source
orders, mono/stereo main and mono/stereo monitor layouts. Detached pointer views
alone do not prove integration.

Known complete-profile faults preserve previous readiness/profile/view. Once a
native setup call runs, failure preserves its original status and fails closed;
there is no arbitrary history rollback. Whole block mapping precedes complete
state, queue dequeue, transport and DSP. Supported rejection silence protects
metadata and the storage outside the union of declared writable prefixes.

## Limits

The helper supports main plus eight aux buses, 64 total channels per direction and
1024 prepared frames. The SDK composition supports None/mono/stereo main stems and
one explicit connected sidechain slice, with no inactive-sidechain inference.
Layout replacement is stopped and does not register host routing or buffer capacity.
Default public references stay main-only. Raw SDK pointer capacities and typed
live contexts remain caller obligations. MIDI instrument aux context, AAX f64,
generated events, arbitrary custom stems, dynamic registration, production chunk
compatibility, installation/signing/Pro Tools and deadline evidence are separate.

The portable reference's complete Gain/Bypass state is separate from a transient
processed-frame telemetry count. It exports f32/f64 native tables, but this AAX
consumer adopts f32 only. Prepared C++ mapping allocation checks cover the helper,
not arbitrary SDK callbacks/native consumer hooks. See the focused guide for
ownership, failure and precision boundaries.

## Validation

Target-only validation passes five meaningful Rust routing/state/layout/f64/native
export tests and warning-denied Clippy/Rustdoc. Fourteen fresh CMake normal/fast-math
contracts pass, including actual SDK initialized registered callbacks against Rust
for eight source-order/mono-stereo profiles, independent audio/tail/state/history
oracles, backend failure/recovery, unchanged Gain/Performance owners/model, and
10,000 maximum mapper/late fault cycles with zero counted C++ heap operations.
Eight Python tests pass with the required routing archive and normal/fast multibus
contracts only in native Gain packaging. Public native Gain26, Performance4 and
stock Gain14 CTest contracts and actual registered module/export probes pass;
default Gain/Performance metadata faults preserve queued complete intent and
smoothing/voice history. Focused public Rust tests, Clippy, warning-denied Rustdoc
and strict documentation checks pass. Stock uses an explicit independent cache;
its initial mono lifecycle limitation stays separate. Local/offline evidence does
not supply Pro Tools or an audio deadline.
