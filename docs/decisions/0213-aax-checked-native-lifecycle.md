# 0213: Checked native AAX lifecycle and bounded global intent

Status: prototype; actual registered SDK/native mono/stereo initial realtime
fixtures and strict normal/fast-math owner/model contracts pass. Normal public
native18/stock14 CMake contracts, both actual package probes and six Python
profile tests pass; no Pro Tools/signing/production-state acceptance.

The prior stock AAX wrapper prepares its processor before applying the mono bus
arrangement. Native-schema adoption alone does not fix ordinary initial mono
rendering. A void offline toggle reparses shape but ignores native setup returns.

Use an authored AAX_CEffectParameters model and descriptor-declared native
algorithm Context instead. Compose public SDK APIs with exact native processor,
audio-state, cached-schema and complete-codec companions; copy no SDK source,
access no private wrapper layout and infer no precision/profile from exports.
Default local Gain selects mono/stereo main planar f32 at1024frames, preserving
public effect/manufacturer/product/native IDs. Explicit stock regression keeps its
original consumer/registration and records the unresolved stock mono gap.

Actual SDK Initialize must query exact effect identity, finite positive rate and
matching stems and check native prepare then current complete state success before
ResetFieldData publishes a ready owner. Original effect/rate/stem query statuses remain unchanged;
native and AAX error domains are distinct. Retain exact stopped lifecycle native
codes separately from a lock-free atomic latest AUDIO code; the SDK void algorithm
cannot silently erase backend errors. ResetFieldData is context publication, not
DSP reset. Explicit stopped reset/reprepare restore accepted intent and coalesce
queued commands; failed provider operations disable readiness until stopped
recovery. Invalid known profile preflight preserves an earlier owner before backend
mutation. Retirement invalidates readiness before native/SDK borrower cleanup.

CONTROL serializes callbacks, schema/SDK ownership, codec and lifecycle; one AUDIO
consumer exclusively owns prepared DSP. Stop/join before lifecycle. Mutation guards
reject foreign notification reentry into retirement/other mutations. Native table,
identity/SDK-library and all foreign ranges outlive borrowers. Outer Context/frame/
Owner-pointer metadata requires live aligned declared storage, disjoint from samples
and owner. Fixed main-bus preflight validates frame/range/overflow/alignment, whole
Owner/pointer-array metadata overlap and output channel overlap before state/queue/DSP.
Known rejection preserves queued intent/history, clears supported output prefixes,
leaves tails. Invoked backend failure fail-closes; no arbitrary history rollback or
pointer-validity/isolation/deadline guarantee is invented.

A bounded SPSC ring uses power-of-two slots2..1024, default32/capacity31. Its u32
release/acquire counters and modulo indexes remain coherent across rollover. Stage
whole known scalar/capacity state before SDK acknowledgement, commit canonical
CONTROL intent only after success. SDK host-request setters are distinct from
acknowledged Update callbacks. Scalar staging uses cached native mapping; codec
work stays CONTROL construction/save/restore. Zero-frame callbacks with pending
intent reject; positive-frame blocks consume offset0 events. Overflow does not
overwrite. Accepted complete intent survives failed/rejected processing and
successful stopped reset/reprepare coalesces it once.

Complete state validates through an isolated native codec before any SDK/queue
mutation. Newest complete restore supersedes older global commands; trailing
parameters retain source order. Coherent save includes acknowledged pending values,
excludes voices/history. The local prototype changes the default stock combined
chunk to complete native RvS1 including bypass, with strict outer IDs/version/size;
no production session compatibility or legacy migration receipt follows. Whole
known malformed input is atomic; arbitrary foreign void notifications or partial
exceptional SDK updates are not rolled back. Public SDK registration/automation
void wrappers may erase delegate statuses; they are not new checked successes. Cancel reservation, invalidate readiness,
then require stopped cleanup/fresh initialization to restore accepted intent.

The first model requires writable global parameters. Read-only/generated reflection,
programs, MIDI/transport, auxiliary buses, latency/tail/output, f64, editors and
AudioSuite remain distinct format mapping work. Offline notifications are explicit
mode metadata only because native setup has no processing-mode field.

Evidence separates actual ordinary default Gain registration/SDK Initialize and
first mono/stereo1024 blocks from a separately initialized synthetic exact-origin
owner through the same registered void algorithm for original backend poison/status.
Strict normal/fast-math owner/model fixtures inject host/native lifecycle faults,
check independent successful DSP phase oracle against rejected buffers, queue/state
ordering/capacity and10000 C++ allocation-counted AUDIO/control cycles. Existing Rust
native allocation evidence is separate. Normal public native/stock packaging and
six Python checks pass; actual DAW/signing/other-platform/production/deadline
support stays unproven.
