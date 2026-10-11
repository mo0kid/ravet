# Owned VST3 parameters and timed input

The default-disabled `ravet-host/vst3-catalogue` feature exposes
`vst3::instances::parameters` beside the [instance](vst3-instances.md) and
[state](vst3-state.md) owners. Explicitly link the native provider built with
the caller's external SDK; Cargo does not discover an SDK or provider library.
`ravet_vst3_parameter_provider_v1` exports an independent 72-byte companion on
the locally tested 64-bit macOS architecture. Catalogue-v1, instance-v2 and state
table layouts remain unchanged. The companion pairs with the exact instance-v2
table pointer, rather than an equivalent-looking copy.

## Controller ownership and reconciliation

Unsafe `parameters::Provider::from_raw` establishes the trusted native table,
code and thread contract. Its borrowed control facade is neither Send nor Sync.
Controller calls require an initialized, stopped, deactivated, non-faulted child
on its original platform main thread. All mutable CONTROL operations across its
parent module require every borrowed audio scope to have joined. Prepare and
start multiple children before lending their independent audio scopes; joining
one child does not permit control mutation while another remains borrowed.
Foreign control reentry is rejected. Provider code must remain loaded through
process exit when native references force pinning; a Rust borrow does not retain
an independently unloadable provider DLL.

`refresh_parameters` validates and copies a complete catalogue of at most 1024
parameters. IDs are unique and distinct from the SDK sentinel; defaults are finite
and normalized, step counts nonnegative, flags recognized and internally
consistent. Titles, short titles and units come from bounded native UTF16 and
become owned UTF8 strings. Bypass and program-selector flags are explicit; opaque
unit IDs do not imply unit enumeration or program-bank support. Failed refresh
keeps the previous native metadata available for inspection but invalidates
parameter preparation. Existing owned Rust catalogues remain valid values.

Stopped reads, plain/normalized conversions, formatting, parsing and edits call
the controller. Normalized values must be finite in `[0,1]`; plain results must
be finite. Parsing accepts at most 384 UTF8 bytes and 127 UTF16 code units without
embedded NUL. Text output has 512-byte native capacity including termination.
Read-only and hidden parameters cannot be edited or delivered as timed points.
Successful controller edits change controller state only. Deliver DSP changes
separately through `process_parameters`; the framework does not infer that a
controller edit reached the component.

A refused or thrown foreign edit may already have mutated private state. It
invalidates parameter preparation without rollback. Malformed successful edit
reports or values also invalidate the Rust companion. Local shape rejection
preserves preparation. Recovery requires explicit successful refresh and
`prepare_parameters`, or a fresh child if the native lifecycle is faulted.
Successful state restore invalidates parameter compatibility and the previous
audio profile; refresh and prepare explicitly before restarting. Restart flags
are observable requests, with no automatic clearing or reconciliation.

## Prepared sample-timed input

`prepare_parameters` reserves the native pool and selects the companion for the
existing scoped `Audio` owner. The control provider stays on main; the audio owner
carries the exact table borrow and can move into one joined worker scope. Ordinary
`prepare` selects ordinary audio processing again. No raw SDK interface escapes.

`Audio::process_parameters` admits at most 2048 copied normalized points across
64 distinct writable IDs. Source offsets are globally nondecreasing and strictly
increasing for each ID. Offsets lie inside the current block; a zero-frame block
accepts offset zero. SDK per-ID queues do not define cross-ID ordering at equal
offsets. Complete point, channel, report and protected owner/storage ranges are
checked before copying input, invoking DSP or touching caller outputs. A late
malformed point, unknown ID, alias or exhausted pool rejects the entire block
without consuming DSP history or changing output prefixes or backing tails.

Each prepared child borrows one core-owned pool: 16 fixed batches, each with 64
queue objects and 2048 points. Per-core admission is bounded to 16 pools and
64 MiB. Pool and queue storage is created on CONTROL. Processing copies into a
host-owned batch without allocation, deallocation, locks, controller calls or
registry operations. Queue data is readable only on the admitted audio thread
during the synchronous native process call, then freezes on return or exception.
Mutation methods, wrong-thread access and malformed queue reads refuse effects;
an observed admitted protocol fault is distinct from the plug-in's returned code.
Known native DSP faults silence supported output prefixes and close processing;
backing tails remain unchanged.

Reports preserve original SDK results only when `returned_status()` is present.
Exceptions have `threw()==true` and no returned status. A swallowed queue fault
can accompany an actual native zero. Owned metadata rejection can carry a
synthetic diagnostic without advertising a returned SDK code. Raw C stage and
failure constants are declared in `RavetVst3Parameters.h`; successful CONTROL
reports do not expose a result for every individual SDK getter. A successful
process report must prove the actual process return and absence of queue faults.
Raw C admission rejection preserves the report buffer. Initialize it for each
call and do not interpret a previous call's fields as new facts. Rust supplies
a fresh owned report for each operation.

## Retained queue lifetime

Every batch and individual queue retains a permanent host reference. A slot is
reusable only after all 65 reference counts return to that reference and its phase
is frozen. Retaining all 16 slots causes bounded rejection before the next native
process; automation is not silently omitted. Valid foreign leases remain usable
for reference counting and checked `QueryInterface`, while frozen data access
rejects. Foreign release cannot destroy host payload storage.

Joined main teardown detects retained references and pins the exact reserved
module/context/host core before facade retirement. Only detached host-only pools
are reclaimed on CONTROL. Closing a pinned facade consumes `ProcessPinned`;
same-core v2 reopen retains the original reservation. Later releases permit
payload reclamation but do not unpin the module. Unexpected audio-end failure
keeps the existing exceptional ownership retention policy. This bounds host
pool admission, not arbitrary foreign allocation or whole-process RSS.

## Validation

The native recipe includes C11 layout assertions, strict SDK catalogue/immutable
queue/pool fixtures, initialized controlled transactions and explicit Gain and
Performance module tests. Normal and fast-math fixtures exercise finite-value
admission, controller mutation/refusal/throw, whole-block rejection, reentry,
retained individual queues, saturation/freeze races, exact-core pin/reopen and
two prepared children sharing a core. SDK preparation refusal, synthetic bus
metadata rejection and exceptions retain distinct diagnostics.

Run the safe Rust consumer after building the native Release archive:

```sh
python3 tools/vst3-validation/check-parameter-owner.py \
  --build target/vst3-host \
  --gain /absolute/path/to/ravet_gain_vst3.vst3 \
  --performance /absolute/path/to/ravet_performance_vst3.vst3
```

The fixture proves actual timed Gain smoothing and bypass against an independent
audio oracle, controller/DSP separation, Performance's serialized DSP Level,
complete state recall with explicit refresh/reprepare and 10000 variable/zero
blocks per mono/stereo reference profile. Performance renders silence without
notes; its serialized Level is not an audible instrument test. Rust counters
observe Rust allocator calls; separate native fixtures observe C++ new/delete.
Neither proves allocation behavior of arbitrary plug-ins or a real-time deadline.
Mock providers and compile-fail examples cover malformed edit admission, exact
table/borrow ownership and exclusive joined audio scopes.

No arbitrary third-party or DAW support follows from these local references.
Active controller callbacks, automation recording/output, events, transport,
editors, f64, auxiliary buses, hosted graphs and other platforms remain separate
capabilities. See [decision 0233](decisions/0233-owned-vst3-parameters.md).
