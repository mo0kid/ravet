# Owned stopped VST3 state

The default-disabled `ravet-host/vst3-catalogue` feature exposes
`vst3::instances::state` alongside the [scoped instance owner](vst3-instances.md).
The caller-linked native library exports `ravet_vst3_state_provider_v1`, a separate
32-byte companion on the locally tested 64-bit macOS architecture. Catalogue-v1
and instance-v2 tables retain their 40-byte and 112-byte layouts. Cargo neither
supplies an SDK nor discovers or links a native provider automatically.

## Admission and ownership

Unsafe `state::Provider::from_raw` establishes the same executable, thread and
native-code trust as the instance provider. Its companion points to the exact
instance table; an equivalent-looking different table is rejected before native
state calls. The copied table borrow is neither Send nor Sync. All state methods
require exclusive access to an initialized, stopped, deactivated, non-faulted child
on its original platform main thread, before first preparation or after joined
audio teardown and stop. Parent control guards reject foreign lifecycle/state
reentry. All mutable CONTROL operations require every audio scope across the
parent joined; shared keeper geometry stays fixed during AUDIO. No SDK interface
or stream escapes the safe Rust API.

`CaptureCapacity::new(component, controller)` accepts explicit capacities of
1..8 MiB each, including a controller buffer when the instance has no controller.
There is no sizing retry. `capture_state` allocates owned output and captures
mandatory component state, then optional controller-own state. `ControllerState`
distinguishes no controller, a controller returning SDK `NotImplemented`, and
successfully captured bytes. Successful empty bytes are supported state, not
an unsupported getter. Any other controller failure rejects the whole capture.
Only a fully successful transaction publishes a snapshot; raw output prefixes,
record and backing tails remain unchanged on failed capture. A foreign getter may
still change its own private state; this API promises no getter rollback.

`Snapshot` owns copied bytes, controller metadata and the component's opaque
16-byte class UID. It can move between threads without retaining native objects.
`Snapshot::new` admits bounded caller-owned bytes; neither constructor nor restore
authenticates a plug-in's version, schema, state encoding or native code. Restore
checks class UID, controller presence, lengths and every native range again.

## Restore, failure and recovery

Restore reserves and copies all source streams before the first mutating call.
It calls component `setState`, controller `setComponentState` with an independent
zero-position copy of component bytes, then controller `setState` when own bytes
were supplied. Optional unsupported controller-own state still receives
`setComponentState`. Raw read-only sources may overlap each other, but neither may
alias metadata, diagnostics or protected native owner/stream/scratch storage.
Caller-declared pointers must actually denote live typed memory; the provider does
not isolate unrestricted foreign code in another address space.

Successful restore invalidates the old prepared profile. Call `prepare` explicitly
before `start`. If using the [parameter companion](vst3-parameters.md), explicitly
refresh and `prepare_parameters` instead. No automatic bus negotiation, parameter cache refresh, restart-flag
clearing or processing synchronization occurs. For an exclusively owned child:

```rust
use ravet_host::vst3::instances::state::CaptureCapacity;
// `state` is the exact trusted companion; `child` is stopped on main.
let saved = child.capture_state(state, CaptureCapacity::new(4096, 4096)?)?.snapshot;
child.restore_state(state, &saved)?;
child.prepare(profile)?;
child.start()?;
```

Compound restore can partially commit before a later refusal. Any failed foreign
restore or sticky stream fault poisons further preparation, processing and state
operations, even if the plug-in returned SDK success after a rejected stream
operation. There is no rollback or retry. Joined stop and retirement remain safe;
retire the child and create a fresh one for recovery. This state fault is distinct
from unproved audio-end ownership, which requires exceptional native retention.
Malformed post-restore reports also fail closed because they cannot prove no
mutation occurred. A genuine preflight refusal preserves the previous profile.

Reports identify the failed component/controller stage. `returned_status()` is
the original returned SDK code only when a call actually returned; a caught
exception has `threw()==true` and no returned status. Sticky stream faults remain
separate from that code, preserving an actual SDK zero alongside a swallowed
stream failure. `controller_capture_status()` is meaningful on successful capture
alongside the snapshot's controller mode. Getter failure does not automatically
poison a stopped child; setter failure does.

## Bounded streams and retained foreign references

Actual SDK `IBStream` objects own fixed storage. Cursor/data methods admit only
the original main thread during their synchronous phase. They validate signed
seek arithmetic, sizes, optional output pointers and known aliases before effects;
capture gaps are zeroed and short EOF reads report their actual byte count.
Streams freeze on every foreign return or exception. Frozen streams reject
cursor/data access. `QueryInterface` and reference counting remain available
across threads and after freeze; selector/output alignment and known aliases are
validated before output writes without changing a completed stream diagnostic.

The native core keeps a host reference before passing streams to foreign code.
Per-core budgets are 16 streams and 64 MiB; each stream is at most 8 MiB. All leases
for a transaction are reserved before foreign calls. A foreign worker's last
release leaves that host reference, so payload destruction occurs later on control.
Host-only frozen streams are reclaimed on the next control state operation.

A retained foreign stream forces process pinning through the core's existing
reserved registry slot, including retention after rejected or throwing getters.
Last parent-facade retirement consumes `ProcessPinned`; module, factory, context,
host and keeper ownership survive until OS process exit. Reopening a known v2
path/alias uses that same core; v1 cannot adopt it. Releasing all foreign streams
later permits control payload reclamation but does not unpin the module. Provider
executable and callback code must also remain loaded through process exit. There
is no static-exit destruction or promised whole-plugin heap/RSS bound.

## Validation

The native CMake recipe includes C wire-size, actual SDK bounded-stream,
controlled component/controller transaction and module-retention tests. Authored
SDK-interface fixtures prove combined/separate/absent controllers, optional and
zero-byte state, exact failure stages, swallowed stream faults, reentry rejection,
partial commits, bounded exhaustion, frozen worker references and same-core reopen.
The loader substitute is separate from actual Gain/Performance module evidence.

After building the native Release archive, run the complete safe Rust owner:

```sh
python3 tools/vst3-validation/check-state-owner.py \
  --build target/vst3-host \
  --gain /absolute/path/to/ravet_gain_vst3.vst3 \
  --performance /absolute/path/to/ravet_performance_vst3.vst3
```

This explicit local fixture tests complete state capture/restore, distinct connected
controller-own values, explicit reprepare, mono/stereo rendering, rejected late
controller state, safe retirement and fresh-child recovery. Its 10000-block loops
measure zero Rust allocator calls/deallocations; SDK/C++ allocation and real-time
deadlines are not measured by that counter. Pointer-valid Rust provider fixtures
cover exact table pairing, bounded admission and contradictory reports, while
compile-fail examples preserve main ownership and exclusive audio borrowing.

No DAW, arbitrary third-party plug-in, editor/event/graph integration,
other-platform runtime or real-time deadline support follows. See
[decision 0227](decisions/0227-owned-vst3-state-transactions.md).
