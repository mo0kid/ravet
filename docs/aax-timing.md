# Prepared AAX timing

`RavetAaxTiming.hpp` and opt-in native AAX owner/model timing provide initial
latency validation and an acknowledged switch between two prepared native cores.
The first supported profile is a matching mono/stereo main planar-f32 effect,
with no auxiliary buses, transport, MIDI or generated events. Existing Gain,
Performance and stock factories keep their defaults. SDKs stay external.

This prototype requires an integration scheduler to reconcile a delivered latency
notification on CONTROL before the next AUDIO callback. Actual SDK-interface
fixtures validate that scheduler; they do not establish Pro Tools, PACE signing,
arbitrary host scheduling, seamless transitions or real-time deadlines.

## Initial declaration and exact tables

Declare initial algorithm latency on the **same** component property map passed
to `AddProcessProc_Native`, using `AaxNativeTiming::declareInitial`. This uses public
`AAX_eProperty_LatencyContribution` (56), checks signed-int32 representation before
the setter and preserves its original returned result. Native latency must fit
`INT32_MAX`; tail kind must be none, samples or infinite, with unused tail samples
zero. Tail is retained as a native diagnostic; no AAX tail API is invented.

Append `EnableDynamicTiming=true` to the native owner/model template arguments and
supply `TimingProfile{initialLatency}` to the model constructor. Its initialized
`EffectInit` prepares/restores native state, validates the declared latency against
the native timing tuple, and queries the initialized public Controller for exact
initial consistency before parameter registration/readiness. This initial getter
is not a dynamic acknowledgement. A mismatch, malformed tuple or original getter
failure leaves the model unready.

Each core uses its own exact typed audio/cache/state origins. Prefer the cache
returned by that core's audio companion `cached_parameters_entry()`; matching
metadata alone does not authorize a separately exported cache callback origin.
Candidate metadata must match every complete writable global ID/order, range,
default, flags, master-bypass identity, name/text/unit, precision and codec
product/version/byte shape. Each candidate validates complete CONTROL intent with
its own isolated codec. SDK parameter objects continue borrowing the initial
immutable metadata; source libraries must outlive every handle and SDK borrower.

## CONTROL request and AUDIO adoption

One CONTROL producer and one AUDIO consumer remain required. Stop/join AUDIO for
lifecycle, candidate preparation and diagnostic copies. The enabled owner has two
fixed optional native-client slots and a stable context address. Prepare/cache
creation and object destruction occur on CONTROL; AUDIO changes slot identity and
fixed complete values. Disabled owners retain one client slot and unchanged APIs.

1. `stageTimingCandidateStopped(tables)` validates the identical schema/provenance,
   prepares the free core and applies current complete CONTROL intent. It rejects
   the current latency, invalid timing, generation exhaustion and a second pending
   candidate. Preparation failure destroys the candidate on CONTROL and preserves
   old DSP, FIFO history and intent. AUDIO may resume on the old core after staging.
2. `requestStagedTimingControl()` requests the candidate's latency through the
   public initialized Controller. It may overlap an old-core AUDIO callback.
   Successful `SetSignalLatency` and a matching getter alone do not switch DSP.
3. `NotificationReceived` takes actual no-data `SignalLatencyChanged` and canonical
   int32 `DelayCompensationState` (0/1) through a lock-free atomic mailbox. It calls
   no SDK getter, processor or codec and preserves synchronous notification reentry
   during the request. Foreign payload liveness/capacity remains the caller's duty.
4. `reconcileTimingControl()` must run after a delivered acknowledgement and before
   the next AUDIO callback. A fresh qualifying notification **and** an actually
   returned successful exact `GetSignalLatency` authorize publication. No polling,
   timeout, getter-only match or compensation-enable notice manufactures an ACK.
5. The next admitted AUDIO block preflights the entire callback/audio/event/queue
   layout before adopting. It snapshots old **rendered** complete parameters and
   applies them with the candidate's own cache, then processes queued complete
   recall and subsequent controls in source order. The candidate's original staging
   intent does not overwrite newer rendered/queued state.
6. `collectRetiredTimingControl()` deactivates and destroys the old client only
   after AUDIO releases its final old-slot access. A missing retirement token is
   Busy/rejected. Deactivation failure preserves ownership for CONTROL recovery.

No FIFO/filter/voice history is migrated. The fixed-delay consumer intentionally
starts its new FIFO at zero, making the transition discontinuous. Accepted state
and DSP cannot be rolled back after acknowledgement. Setters/restores that observe a held timed admission reject before SDK intent/queue
mutation; stopped model operations reject before SDK queries or proxy retirement.
The atomic observation is not a general CONTROL/AUDIO mutex: an operation begun
before admission may still complete through the ordinary SPSC ring. Caller stop/
join remains required for lifecycle, not inferred from an observed clear bit.
Native snapshot/apply/backend
failure after ACK makes processing fail closed with supported output silence; it
never resumes old latency under potentially changed host compensation. Whole late
buffer preflight rejection retains the published candidate for the next valid
block and preserves state/history/queue. Callback reentry fails one atomic CAS
before getters, DSP, diagnostics or output mutation, leaving the outer callback's
borrowed audio intact.

## Notification availability and failures

Compensation availability starts unknown, not enabled. Known disabled compensation
rejects a fresh request before any setter. Disable while pending retains the old
core and candidate; an enable notice cannot revive an earlier blocked ACK. A new
qualifying latency notification is required. A 24-bit ACK sequence saturates and
fails closed on exhaustion, never wraps. Request generations likewise do not wrap.
Malformed notice type/data/size/scalar preserves the existing mailbox snapshot.

A returned request refusal with no intervening ACK destroys the candidate on
CONTROL and leaves old history intact. Refusal plus ACK, an unreturned exception,
or acknowledged getter failure/mismatch is uncertain external mutation and fails
closed. Idle redundant current-latency ACKs reconcile without switching; unexpected
idle latency changes fail closed. There is no SDK cancellation token: pending
transactions cannot be discarded as though host compensation were rolled back.
Uninitialize/new instance is the general recovery after uncertain/faulted changes.
A sticky timing fault cannot be cleared by preparing or resetting that same owner.
Retirement collection still releases the old acknowledged slot, but preserves the
Faulted phase and false readiness. Stopped prepare/reset/initial-timing checks,
model reprepare/reset and same-instance Initialize reject before foreign backend
or SDK calls, retaining the earlier diagnostics. Uninitialize retires borrowers;
a new instance must use a descriptor/native profile and host latency that agree.

`timingStopped()` copies canonical current/candidate timing, phase, generation,
request/getter/native attempted/returned flags, exact returned results and the
atomic mailbox sidecar. Copy only after AUDIO stop/join and CONTROL serialization.
An unreturned exception has no original result. `lastNativeProcessStatus()` is a
separate atomic AUDIO backend status; after preflight rejection it may describe an
earlier block. Current native state operations and SDK request/getter diagnostics
remain separate. No native-success result implies host compensation success.

## Fixed-delay reference and local evidence

The separate `delay` module and `RavetDelayRouting.h` export Delay8/Delay16 native
32/64 origins. Default AAX timing adoption uses f32. The explicit
[internal precision adapter](aax-precision.md) uses matching f64 candidates
behind host-f32 buffers, retaining the same ACK/adoption/retirement contract. Both have Level (0..1, default1) and
master Bypass (0/1, default0), complete `RAVETDLY` v1/32-byte global state, matching
mono/stereo main layout, at most1024 frames/events and no output events. Bypass skips
Level multiplication while retaining the fixed FIFO delay. Prepare/reset clear
FIFO; complete global recall preserves it. Their native tails are Samples(Delay).
Neither installs a public production AAX factory or an automatable latency control.

Target strict normal/fast timing and default Gain/Performance composition passes
6/6. Actual registered SDK Initialize→Rust Delay8/16 callbacks prove independent
impulse offsets, old8 FIFO while pending without ACK, synchronous/deferred ACK,
compensation transitions, exact statuses, candidate rejection, queued state order,
late metadata rejection before adoption, original native failures and no old-core
fallback. Repeated1000 pending and1000 adopted maximum1024-frame callbacks allocate
and destroy zero C++ heap objects; CONTROL alone retires old native clients.
Counters do not instrument Rust/host allocations or deadlines. Isolated Rust15
including delay/native/state/precision checks, release archive, warning-denied
Clippy/Rustdoc and Python8 pass. A fresh full target external-SDK native Gain package composition passes32/32 and
actual loaded-module exports/version/initialized mono/stereo contracts. Final
mailbox normal/fast checks additionally prove the actual24-bit saturation boundary
and20000 concurrent notices without lost ACKs. Actual held registered worker
callbacks additionally prove setter/restore/lifecycle rejection without manager,
SDK/native ownership or FIFO mutation and post-join acceptance. Public source is integrated after review; public qualification and shared gates
remain pending. The receipts above qualify the isolated prototype.
