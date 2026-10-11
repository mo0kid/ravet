# 0231: Retire paired clocked callbacks before original-owner recovery

Status: Accepted implementation contract. Reviewed source is integrated;
public qualification is recorded separately in work status.

## Context and decision

The ordinary clocked duplex stream owns its raw producer in an input closure and
its processor/unique consumer inside the output renderer. Pause/drop provides no
framework stopped-owner receipt. W203 output retirement cannot prove input capture
or paired clocked ownership. W215 file/capture composition therefore cannot safely
perform native stopped file seek through the existing hidden clocked closures.

Add an explicit paired retirement route, preserving legacy APIs/defaults and the
existing corrected sinc/FIFO profile. The worker retains one preallocated shared
cell containing disjoint prepared capture and clocked renderer fields. Exactly one
private non-Clone FnMut guard borrows each field; the safe SPSC queue is their only
mutable cross-thread communication. Two error guards access separate immutable
atomic counters. All four guards publish Release flags and drop non-last cell
references; the worker waits all four Acquire flags AND stable sole strong
ownership before safe consuming cell extraction. No Weak/public/future clones,
callback-owned renderer/capture/counter Arc or endpoint transfer queue exists.
A field flag published before actual guard Arc destruction cannot retire it.
UnsafeCell/unsafe Sync are confined to this private module with documented unique-
borrow/disjoint-field and worker-retention invariants and scoped lint exception.

Spawn the worker before handing callbacks to native builders. Spawn failure returns
original processor before handoff; input/output build failures retain their first
original errors and explicitly pending proof when the backend may still hold
guards. Drop both streams CONTROL. Early receiver/handle Drop detaches the worker
without destroying cell ownership. Poll unwind is caught with permanent owner
outside the catch scope. Disconnected outcomes dispose prepared fields on worker,
never callback. Native backend never releasing callbacks has no timeout receipt.
CPAL0.18.2 CoreAudio Stream Drop signals monitor readiness while native units,
listeners and monitor/error captures own their own teardown lifetimes; framework
proof waits guard destruction instead of assuming synchronous destruction. Native
locks/allocations/destructors and actual scheduling remain backend limits.

Add failure-owned prepared constructors retaining P/raw consumer on ordinary error;
legacy new delegates and preserves its public consuming signature. Fallible scratch
reserves precede child prepare. Private Option<P> plus active state allow safe
consuming extraction and once-only deactivate; no unsafe move or stale renderable
mutable-processor API. Stopped result retains the unique prepared renderer/capture,
exposes no process/render method, and consumes into original P plus matching raw
consumer/producer. Dispose converted queue/scratch/prefetched history CONTROL.
Remaining queued raw frames retain actual position; no restoration/rollback.
Foreign prepare/deactivate/restore panic does not imply a successful owner receipt.

Include an explicit selected-ID caller-service recovery coordinator with Owned,
Live, Retiring, Disabled and exceptional Lost states. Never open, restore or seek
while old callback proof is pending. After proof, caller-selected restoration
receives original P and exact stopped telemetry before new preparation. Failed
restoration preserves P/receipt, including actual mutations; successful restoration
consumes its receipt once. Ordinary open/start failures preserve Immediate/Pending
ownership and original errors. Only successful started opens advance generation.
Known live callback faults retire before enumeration; healthy enumeration errors
retain the live pair and their original errors, rather than inferred device absence.
Disable still polls retirement safely without reopening; re-enable continues the
ownership sequence. Existing ordinary DSP fault versus backend/panic policies stay
distinct. No automatic factory, state migration, timer, fallback or subscription.

## Evidence and limits

Focused tests exercise all24 callback Drop orders, zero callback heap actions,
flags before Arc field destruction, asynchronous owners and early receiver Drop,
original spawn/build errors, processor/raw endpoint recovery, disjoint concurrent
capture/render, worker/capture/output panic and explicit proof failure. Recovery
uses actual guarded prepared fields with synthetic asynchronous native holders to
prove no premature restoration/open, exact owner/receipt retention and generation,
disable/re-enable and original errors. Actual Wave/IntegerSinc indexed worker plus
W215/canonical clocked processing proves stopped failed seek preservation,
successful seek17 and fresh preparation, exact suffix/capture PCM, full file drain
and exactly two child prepares/deactivations. The isolated focused receipts and short silent native observations are recorded in work status; public qualification follows separately.

Two short silent paired CoreAudio cycles on the same USB interface recovered the original processor after joined callback retirement. These are ownership observations only. No shared timestamp origins, acoustic
playback instant, physical synchronization, hotplug acceptance, long-session drift,
latency or callback deadline follows. No live seek mailbox or native pending-owner
extraction exists. Existing device/converter/controller bounds remain unchanged.
