# 0230: Prepared AAX initial and acknowledged dynamic timing

Status: Accepted implementation contract. Reviewed source is integrated;
public qualification is recorded separately in work status.

Validate public initial LatencyContribution and the initialized Controller's exact
initial latency against canonical native timing before owned-model readiness.
Initial getter consistency does not authorize dynamic switching. Native tail stays
an exact diagnostic; the SDK supplies no invented generic tail advertisement here.

Use an opt-in two-slot native owner with immutable identical schema, complete global
state family and exact per-core cache/audio/codec provenance. Prepare candidates
while stopped on CONTROL, then continue old DSP while a request awaits host ACK.
An atomic notification mailbox admits reentrant notices without getters/lifecycle.
Successful SetSignalLatency plus a fresh eligible SignalLatencyChanged and exact
returned GetSignalLatency are all required. Compensation unknown is distinct from
enabled; disable clears eligibility, enable cannot revive an old blocked ACK.

Require an explicit CONTROL reconciliation point between delivered ACK and next
AUDIO. Publish the prepared slot with release/acquire generation, whole-preflight
before AUDIO adoption, and copy newest rendered complete parameters with the new
core's own cache. Queued recalls and trailing controls retain ordinary source order.
Only complete global parameters migrate; new Delay16 FIFO starts zero. This is a
discontinuous handoff, not seamless DSP history migration. AUDIO releases old-slot
retirement only after its final access; CONTROL alone deactivates/destroys it.

Returned refusal without ACK preserves old DSP/history and retires the candidate.
Refusal plus ACK, unreturned exceptions, getter mismatch/failure or post-ACK native
state/backend failures are uncertain compensation and fail closed. No old-latency
fallback or SDK cancellation is claimed. Initial/candidate/request/pending/published/
adopted/faulted phases and original attempted/returned native versus SDK diagnostics
remain distinct; stop/join is required to copy non-atomic diagnostic fields.

Target evidence: actual registered mono/stereo SDK Initialize and real Rust Delay8
and Delay16, independent block-spanning impulse/FIFO/state-order oracles, no-ACK
retention, compensation transitions, reentrant callback guard, candidate/schema/
provenance/native failures and supported tail-preserving silence. Strict normal/
fast timing plus unchanged default Gain/Performance6/6 pass. Rust15/doctests, release,
warning-denied Clippy/Rustdoc and Python8 pass. Repeated1000 pending+1000 adopted
maximum-frame callbacks instrument zero C++ allocation/destruction; Rust/host heap
and deadlines are not inferred. Fresh full target native Gain composition32/32
and actual module contracts pass; final normal/fast mailbox boundary/concurrency
checks pass. Actual held worker callback/control/lifecycle guard and post-join
acceptance prove timed atomic availability without SDK retirement or FIFO changes.
Public source is integrated; public qualification and shared gates remain pending.
SDKs remain external; no Pro Tools/signing, arbitrary scheduler, AAXf64/aux timing,
AudioSuite/DSP hardware, seamless handoff or host-tail support is declared.

Sticky timing failure is an irreversible owner state. Old-slot CONTROL collection
may complete cleanup but retains Faulted rather than exposing Initial readiness.
Stopped preparation/reset/initial validation and model reprepare/reset/EffectInit/
same-owner Initialize reject before backend/SDK calls. This prevents successful
setup reporting while the subsequent AUDIO callback still rejects the sticky fault.
Actual fault→collection regression observes native prepare/reset/state/timing and
SDK query/reference counters, false readiness and retained original audio code.
Uninitialize permits retirement; a new correctly reconciled instance is required.
