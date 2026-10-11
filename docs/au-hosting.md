# Optional AUv3 hosting

`ravet-host`'s default-disabled `au-hosting` feature provides caller-linked macOS
AUv3 registry discovery, asynchronous ownership and prepared main-bus f32 audio.
The authored native provider uses Apple SDK APIs; SDK sources are not vendored.
Build the `ravet_au_host` target of `crates/ravet-host/native/au` with CMake on
macOS 12 or later, then explicitly link the resulting static library plus
AudioToolbox, AVFoundation and Foundation. `ravet-host` has no build script and
never links the provider itself.
The Rust application obtains `ravet_au_host_v1()` and accepts its genuine immutable
ABI with unsafe `Provider::from_raw`. Provider executable code remains live for
process lifetime, including outstanding cancelled asynchronous requests.

Discovery reads only caller-supplied type/subtype/manufacturer registry filters;
there is no filesystem search, installation or implicit component execution.
Maximum catalogue size is1..256, with zero actual matches valid. Names are owned
UTF8 up to4096bytes, descriptions/versions/flags are preserved, and ambiguous
same type/subtype/manufacturer identities refuse rather than choose arbitrarily.
Creating a selected component is explicit execution of trusted native code.

`Catalogue`, `Pending` and `Instance` are main-only, non-Send/non-Sync owners.
Async instantiation never calls a Rust callback on an Apple implementation thread.
The captured native completion state has independent lifetime; closing a catalogue
before completion is valid. Applications must service their main queue/run loop
and call nonblocking `Pending::poll`. Poll never pumps a hidden loop or waits.
There are16 process/provider pending slots and256 instance/reservation slots;
creation reserves its future instance slot before initiating the foreign request.
Dropping/cancelling a request consumes its facade, not Apple's underlying work.
Cancelled requests retain capacity until a real completion reaches main cleanup.
A foreign request that never completes cannot be reclaimed or bypass the bound.

Automatic, RequireInProcess and RequireOutOfProcess request Apple's corresponding
load options. Successful creation is checked against actual `isLoadedInProcess`
and exact delivered description. A required-mode mismatch consumes/retire once;
option bits or an extension-service PID cannot prove where rendering executes.
Original signed64 NSError code/domain and signed32 OSStatus remain separate from
local host status. The diagnostic attempted/returned fields count described API
operations, not every Objective-C metadata accessor. Async initiation increments
attempted but is not a completed NSError result; completion polling records the
actual delivered result. Numeric zero with returned0 is never a native success.

Stopped preparation supports one mono/stereo planar f32 output bus and zero/one
matching input bus, sample rate0<rate<=768000, and1..65536maximum frames. All scratch,
ARC blocks and resource setup happen on control. Output/input format negotiation
and resource allocation report BOOL+NSError; they do not invent OSStatus values.
Late foreign preparation failure fault-closes the instance and requires new
ownership, rather than claiming native rollback. Clean stop permits reprepare.

`Instance::with_audio` lends one Send/non-Sync/non-Clone scoped capability. Safe
scoped workers join before main retirement, including unwind and forgotten tokens.
Native AUDIO admission uses one lock-free CAS: nested/concurrent processing returns
Busy before reading caller records, touching diagnostics, audio or foreign DSP.
Whole byte/shape/known owner/metadata/alias admission precedes DSP. Input channels
are captured before rendering so raw-ABI input/output cross aliases are supported.
Safe Rust mutable outputs are distinct slices; backing tails remain untouched.
Public cached `AUAudioUnit.renderBlock` is invoked, not internalRenderBlock or a
direct RAVET native processor. Same-rate whole-block input pulls receive separate
scratch copied from the captured source, including repeated/in-place pulls. Subframe
and SRC pull scheduling are outside this initial profile. The exact i64 ABI sample
position is rounded into Audio Unit's inherently f64 sample timestamp.

Returned render failures and caught foreign exceptions silence fully admitted
output prefixes, preserve tails and fault-close; malformed returned buffer shape
also rejects after native history may have advanced. There is no retry, native
history rollback or sandbox guarantee. Caller/plug-in live metadata capacity and
synchronous pull lifetime remain trusted native boundary obligations.

Main geometry and ordinary SDK controls are Busy while any AUDIO child is active.
Already prepared other children can begin/end independently. Owner Drop/cancel
can transfer facade ownership into bounded deferred main retirement: protected
identities/scratch and foreign units remain held until the last joined AUDIO end.
Late completion/cancellation cannot release or reuse storage in that interval.
Foreign cleanup reentry is excluded; failed/throwing deferred cleanup retains the
bounded owner rather than guessing successful retirement or automatically retrying.
Failed native AUDIO end poisons the Rust owner and retains native ownership.

The maintained native Gain fixture and safe Rust consumer register the public
Gain implementation within their own process, asynchronously discover/load it,
observe in-process mode and render mono/stereo through public renderBlock on joined
workers. Independent Gain/tail/alias oracles, shape rejection, reentry, repeated
prepared calls, scope unwind/forget and deferred sibling Drop are qualified.
A separate explicitly injected SDK fixture checks 16 pending/256 instances, worker
completion/cancel/duplicate, synchronous completion inside the initiating call with
ABI reentry (Busy on main, WrongThread on a worker, diagnostics untouched),
success-with-NSError refusal, cancellation after delivery, wide NSError, render
OSStatus/throw/corruption and repeated input pulls. Those synthetic mode values do
not prove actual IPC. The native Gain fixture counts ordinary C++ `new` over 2000
prepared calls and the safe Rust consumer counts Rust global-allocator requests
over 2000 prepared calls; both observe zero. Neither counter covers Objective-C,
Apple framework or libc allocation, and neither certifies a deadline.

The native CTests are `ravet-au-host-failure-tests` (always built with
`BUILD_TESTING`) and `ravet-au-host-gain-tests` (only when `RAVET_GAIN_LIBRARY`
names a caller-built Gain native archive; otherwise it is absent). The complete
native and Rust composition is explicit:

```sh
python3 tools/au-host-validation/check-au-host.py \
  --build target/au-host \
  --gain-library /absolute/path/to/libravet_gain_native.a
```

It configures and builds the provider, runs CTest, compiles
`tools/au-host-validation/host-owner.rs` against the built archives in a temporary
directory and runs it. It performs no registration, installation, signing or
system scan; the Gain subclass is registered only inside the fixture process.

`tools/au-host-validation/extension.mm` (CMake target
`ravet-au-host-extension-probe`, excluded from the default build) is a manual-only
out-of-process probe. It never registers or installs anything and is not a CTest.
Run it only after a separately reviewed local extension registration; its
UNAVAILABLE result (exit 20) is not a pass.

A reviewed, reversible exact-path registration of the public Gain container and
extension was attempted on macOS 27.0.1. The ad-hoc signed bundle, kept under
`~/Library/Caches` rather than `/Applications`, with no Team ID and no
app-sandbox entitlement, never became discoverable: PlugInKit and the
AudioComponent registry reported zero matches before, after registration and
after cleanup, and the out-of-process consumer reported UNAVAILABLE. Actual
out-of-process rendering is therefore unvalidated. Option bits, successful
registration commands or a signed bundle are not evidence of IPC support. Hosted parameters/state/programs/editors/events/transport/aux/f64/graph,
recovery/scheduling and AUv3 export mappings remain separate outstanding scopes.
iOS, devices, DAWs, third-party compatibility, IPC failure isolation, production
signing/notarization/distribution and real-time deadlines are not certified.
