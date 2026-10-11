# Explicit trusted VST3 instances

The default-disabled `ravet-host/vst3-catalogue` feature also exposes
`vst3::instances`: an independent ABI-v2 table and scoped Rust instance owner.
The feature retains its original name for compatibility. Cargo supplies no SDK,
native loader or implicit library discovery. Explicitly link the C++ provider
built with the caller's external SDK as described in [VST3 hosting](vst3-hosting.md).
The static library exports both `ravet_vst3_provider_v1` and
`ravet_vst3_provider_v2`; the original 40-byte v1 table stays unchanged. The v2
table is 112 bytes on the locally tested 64-bit macOS architecture.

## Ownership and trust

Unsafe `instances::Provider::from_raw` establishes exact table/code/thread trust;
unsafe `load` explicitly trusts the selected module and dependencies. The SDK
provider requires the original platform main thread. Rust parent libraries and
child instances are neither Send nor Sync. Children borrow their parent, so safe
code cannot retire a parent while its children remain alive. Preparation,
activation, stopping and retirement belong on main, outside audio callbacks.
The native provider rejects foreign control reentry while changing an owner.
All mutable CONTROL operations across a parent require every borrowed audio scope
to have joined. Prepare/start multiple children before lending their independent
scopes; joining one does not permit mutation while another remains borrowed.

`Instance::with_audio` lends one non-cloneable Send, non-Sync capability to an
exclusive scope. Move it into one joined `std::thread::scope` worker; the first
native process binds it to that worker. The token cannot escape into a static
thread or outer thread scope. Native end runs on main after joined workers on
normal return or unwind. Forgetting the audio value still ends the native scope.
The capability has no interface ownership or destructor. An unexpected failed
native end poisons and retains the native child instead of attempting destructive
cleanup against an unproved audio owner. This exceptional retention can persist
until process exit; it is not an automatic recovery mechanism.

Consuming parent retirement reports complete, failed native context clearing, or
`ProcessPinned`; consuming child retirement reports complete or the first native
cleanup failure. Other rejection returns the exact owner for an explicit retry.
Best-effort Drop performs one legal control retirement and discards its result;
it never repeats a consumed native cleanup.

Factories declaring non-discardable components are pinned before the first native
creation attempt, including failed or throwing attempts. Closing their last
facade returns the consuming `ProcessPinned` outcome; the exact module, factory,
host context and reservation survive until OS process exit. V2 reopens a known
canonical path or symlink alias using that same core without repeating module
entry or context installation. V1 cannot adopt pinned cores. Known active paths
reject before loading, and factory identity rejects duplicate context ownership.
Different paths that return the same factory are still checked after loading;
the provider does not prove every possible loader alias harmless.

The provider executable and host callback code must also remain loaded until
process exit when pinning or exceptional retained ownership is possible. Native
module retention does not retain an independently unloaded provider DLL. Trusted
foreign code has unrestricted process access; crashes, arbitrary allocation,
memory validity and native side effects are not isolated or rolled back.

## Prepared audio profile

This initial profile accepts finite positive sample rates up to 768000 Hz,
1..65536 prepared frames, main mono/stereo f32 outputs, and either no audio input
or a matching mono/stereo main input. Bus metadata is bounded to 16 buses per
direction; event buses remain inactive and auxiliary audio buses are unsupported.
Component and advertised controller interfaces initialize independently; a
combined controller initializes and terminates once without self-connection.
Mandatory handler setup/clearing and reverse successful connection cleanup are
owned. Control edit callbacks reject unsupported edits. `restart_requests()`
reads coalesced native flags without clearing or automatic reconfiguration.

Caller blocks use exact prepared channel counts and borrowed planar f32 slices.
Slice lengths and the prepared maximum are checked before any native process.
Native range checks reject alignment, overflow, reserved fields, late capacities,
output overlap and protected metadata/owner/scratch aliasing before writes.
All input prefixes copy to prepared scratch before caller output is touched,
allowing validated input/output aliasing through the raw C ABI. Safe Rust mutable
slices remain disjoint. Zero frames do not write samples, and backing tails stay
unchanged. Known native DSP faults silence supported output prefixes and close
the processing profile; malformed blocks preserve caller bytes. Declared foreign
ranges must actually be live readable/writable memory under the unsafe contract.

Stopped state uses a separate exact-table-paired [state companion](vst3-state.md).
Timed parameter input uses a separate exact-table-paired
[parameter companion](vst3-parameters.md). Ordinary preparation preserves the
original audio path. The audio profile has no event, transport, editor, graph, f64, auxiliary-bus,
latency/tail reconciliation or third-party compatibility claims in this profile.
Those remain subsequent integrations. Successful stopped state restore requires
explicit reprepare; failed foreign restore permits safe joined retirement but
rejects further processing. Explicit stop/start is supported after
legal joined audio teardown; native preparation faults require a fresh child.

## Validation

The public native CMake recipe builds catalogue/context, C ABI, core registry,
instance lifecycle, block-fault and process-pin contract tests. Setting
`RAVET_VST3_TEST_MODULES` additionally registers actual catalogue and v2 processing
tests; with no module list those actual-module tests are absent. Use separate
explicit local Gain and Performance reference paths in that order for the
current two-reference processing oracle. Contract fixtures use authored valid
SDK objects and a substituted loader; actual module tests use the SDK loader.

The local native composition passes actual mono/stereo Gain unity and Performance silent
processing for 10000 blocks per profile, invalid shapes, aliases, tails, joined
panic and stopped restart. Measured processing loops record zero regular C++
new/delete and, independently, zero Rust allocator calls/deallocations. These do
not count all malloc/aligned/OS/plugin allocations or certify real-time deadlines.
The complete Rust/native composition fixture is explicit:

```sh
python3 tools/vst3-validation/check-instance-owner.py \
  --build target/vst3-host \
  --gain /absolute/path/to/ravet_gain_vst3.vst3 \
  --performance /absolute/path/to/ravet_performance_vst3.vst3
```

Build the `ravet-vst3-host` Release target first. The fixture takes only those
explicit built archives and reference bundles; it performs no system scan or
installation. Its temporary executable remains local and is removed afterward.

Eight negative Rust compilation cases prove ownership restrictions. Pointer-valid
Rust provider faults prove consuming failures, owner-returning rejections,
catalogue staging cleanup and poisoned audio-end retention. Focused public integration and the next combined workspace checks follow the
source adoption; staged receipts remain explicitly scoped to that composition.

No DAW, device, accessibility, arbitrary third-party plug-in or other-platform
runtime support follows. See [decision 0215](decisions/0215-scoped-vst3-instances.md).
