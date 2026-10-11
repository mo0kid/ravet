# 0260: Typed graph stages and shared-control mono ownership

Status: accepted. Source adoption is qualified separately from the isolated
implementation receipts below.

Typed stages reuse the existing Graph topology, preflight, scratch, event broadcast,
latency/tail, explicit audio-owner hooks and fault retirement. The graph owns erased
private wrappers, while the facade owns concrete processors in one-cell
Vec<UnsafeCell<Record<P>>> allocations. Wrappers and all getters derive pointers
from the same cell.get provenance. Allocations never resize/replace while a graph
exists, no reference is stored across calls, and enclosing owner/session borrows
exclude callback/getter overlap. Internal helpers require a private-construction
permit: sealing alone would not prohibit calling a supertrait method through a
public generic bound. The negative example for this calls `T::store(stages)`
without the permit and pins E0061; deleting the permit parameter would make that
same call compile, so it fails only because of the safeguard. The wrapper Send
bound is exactly P: Send; no Sync promise.

Declare/drop the internal Graph before stage cells, including unwinding. Wrapper
Drop never accesses/destroys the processor. Consuming recovery first drops Graph
and then safely uses UnsafeCell::into_inner. Foreign panic is not normal success;
there is no forget/leak fallback, pointer clone or public raw graph escape.
The package narrowly permits unsafe code in this module under an otherwise deny
policy. Every raw-pointer access carries a SAFETY comment stating the validity,
provenance and exclusivity argument. Review the complete ownership and provenance
proof rather than treating passing counters as an aliasing proof. Installed Miri was
unavailable; ownership sequence tests remain Miri-checkable without claiming
execution.

Sealed tuple arities1..8 provide const-index concrete access. The effect-only main
profile rejects auxiliary/generated events; dry bypass skips events/history while
retaining declared latency/tail, without compensating its audible delay.
Executed child errors preserve exact first GraphStatus and map to BackendFailure
when used as a Processor, with whole-output silence and no history rollback. A start
retried on a faulted graph maps to NotPrepared and keeps the original first fault.

## Audio owner contract

The canonical Processor manual-hook contract is retained: manual `start_audio` must
be paired with `stop_audio` on its starting thread before transfer, deactivation,
extraction or destruction. Unlike the guarded session, which is non-Send and stops
on drop, this is a caller obligation. The non-Send session remains the preferred
route; a forgotten guard is misuse and leaves the same live owner as an unpaired
manual start.

With a manual owner live:

- refuse (`AudioSessionActive`): `prepare` and the stopped getter;
- panic through the canonical Graph assertion: `deactivate`, `into_stages`, drop;
- unaffected: render, reset, readonly getters, bypass flags.

`into_stages` only drops the Graph; it does not stop audio. A caught `deactivate`
panic precedes every state change, so the owner thread can still stop. A panic from
`into_stages` or drop unwinds through the facade: each stage drops exactly once
without stop/deactivate and no owners are returned; a second panic during that
unwind aborts, and nothing here guards against it. Execution is not thread-checked,
so a manually started chain is `Send` and a violation surfaces at stop on the wrong
thread, which panics and leaves the owner live.

Rejected alternatives: automatic owner-thread cleanup in Drop or extraction (a drop
may run on a thread that is not the owner, so it cannot honestly stop) and any
redesign of Graph or its Send bounds. Both would change canonical behavior beyond
this feature.

## Mono

Generic mono duplication owns one control object, shared by reference with
independent kernel states. Factory failures preserve original controls/factory and
constructed prefix. Foreign preparation is inactive on failure, earlier active
kernels deactivate in reverse once, and stopped control mutation requires prepare.
The framework does not mutate the control object while prepared, but a shared
reference is not an immutability guarantee: interior mutability in the control
object can change what later channels read, and keeping it effectively immutable is
an implementor obligation. Dropping an active duplicator deactivates prepared
kernels once in reverse. Latency and tail are maxima over kernels.
Checked4MiB payload reservations precede foreign factory/preparation; caller-owned
processor/kernel/control memory and canonical64MiB Graph preparation remain separate.
Native arithmetic/nonfinite/denormal behavior is the kernel's explicit policy.

## Qualification scope

Independent composed TPT difference equations, four-dimensional implicit nodal
Newton ladder and direct FIR sums qualify real native32/64 three-channel graphs.
Both precisions/all1..64 counts, all tuple indices, timed Gain events, fault/rollback,
getter lifetime exclusion with pinned compile-fail codes, original
ownership/drop/unwind including manual-owner panics, heap and resource refusals
are required. Allocator-failure branches after the first tuple element, the
post-reservation capacity check and a `Graph::new` error are unreachable here and
are left as defensive code without tests. This records reusable framework behavior,
not physical platform/host/deadline support or arbitrary foreign lifecycle
guarantees. Root-qualified isolated evidence is 74 runtime tests and 21 doctests,
strict all-target lint, strict Rustdoc and formatting, bound to 295 source inputs
and command logs. Initial fixture/lint failures are preserved; production behavior
is unchanged. Public source adoption remains pending and no Miri result exists.
