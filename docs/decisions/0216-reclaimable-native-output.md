# 0216. Retire native output captures on an explicit control worker

- Status: Accepted implementation; focused and local silent validation passed.
- Work package: W203.

Native output handoff previously made the processor and prepared consumer
inaccessible to control. Native stream Drop alone does not prove synchronous
callback destruction, so it cannot authorize a matching file-worker stopped seek.
Add a separate recoverable output owner and explicit pending retirement proof.
Preserve existing output APIs and their consuming failure signatures.

Keep `PreparedOutput`'s processor in a private Option. Its additive `new_owned`
preflights configuration and fallibly allocates scratch before foreign prepare;
ordinary failures return the exact original processor/error. Failed prepare obeys
the existing inactive trait contract. Consuming `into_processor` takes the Option
before deactivation to avoid duplicate deactivation during a foreign panic. The
returned owner must be explicitly prepared/reopened; no mutable live renderer
exposure or hidden source seek/reset is added.

A private shared cell owns the whole prepared renderer and separate immutable
atomic counters. Exactly one non-Clone data guard temporarily borrows the renderer
mutably through UnsafeCell. The error guard accesses separate counters and never
aliases that borrow. These are the complete framework callback captures; do not
reuse the legacy callback factory, which independently captures a counters Arc.
No Weak/public cell access or later cloning is allowed.

Guard Drop Release-publishes its own flag before its Arc field is destroyed.
Flags alone are insufficient: a worker could otherwise reclaim before that field
Drop and make the callback the final allocator owner. The permanent worker owner
waits for BOTH Acquire flags AND stable sole strong ownership. It then consumes
Arc/UnsafeCell safely, deactivates/extracts the processor and publishes a mutually
exclusive recovery outcome. Final framework renderer/scratch/cell/counters
allocation disposal belongs to control/worker. No callback queue push/full branch,
forgotten processor, silent leak or arbitrary source retry is needed.

Retain the worker owner outside caught polling scopes. A polling panic before
retirement records a worker fault and continues waiting for the same proof. An
early user receiver/stream-owner Drop detaches the worker; it still waits for both
actual callback owners, then disposes a disconnected result on control. Foreign
processor deactivation panic after safe retirement is exceptional and may lose
owner recovery; the result channel cannot manufacture success. A nonblocking poll
must not join an unfinished worker merely because its sender disappeared during
unwinding. Explicit `wait` joins only on control without a deadline/cancellation
claim.

Create both guards before worker spawning and retain a constructor owner until
backend handoff completes. Spawn failure occurs before backend ownership and
returns the exact processor and original spawn error. Backend opening failure can
return the original backend error with pending ownership because its closures can
retire asynchronously. Fixed-zero buffer/configuration/scratch/preparation failures
reject before backend handoff. Legacy OpenError remains unchanged.

Primary CPAL 0.18.2 CoreAudio implementation owns AudioUnit and monitor references;
Stream Drop signals monitor readiness and does not provide this framework's
synchronous callback retirement proof. Native CPAL/OS closure storage, locks,
allocation and destruction remain separate backend behavior. Processing panic
handlers/runtime also retain exceptional cost. Narrow package deny/module allow
for the audited UnsafeCell/Sync boundary is required; no dependency is added.

Focused evidence must cover both callback Drop orders, retirement flag observed
before Arc field Drop, full callback capture allocation/deallocation counts, early
receiver drop, asynchronous backend owners, caught poll panic, processing panic,
preparation/scratch/spawn/backend errors and actual converted consumer recovery,
failed/successful stopped seek and explicit reprepare/reopen. A concrete silent
native open/start/stop/recover/reopen receipt remains environment-specific and does
not establish physical support, clocks/drift or real-time/teardown deadlines.

Accepted isolated validation passed 30 tests/docs, including 13 new ownership/fault
groups, warning-denied all-target/all-feature Clippy and Rustdoc. The local default
f32 CoreAudio profile (20 channels, 96kHz) completed two silent native stop/recover/
reopen cycles with two prepares/deactivations and no backend errors/panics. This
is an environment-specific ownership receipt; broad physical support, audible
quality, clock mapping/drift and callback/teardown deadlines remain unclaimed.

Public integration also passed the same 30 tests/docs and warning-denied Clippy/
Rustdoc. The reusable feature-gated zero-PCM converted probe completed the local
20-channel default f32 CoreAudio profile: 257 frames at44.1kHz retained590 frames
at96kHz, consumed590 then573 after stopped seek17 and explicit reopening. Both
EOF drains and original worker errors were checked. Underflow observations were
reported without advancing source position; they are not deadline evidence.
