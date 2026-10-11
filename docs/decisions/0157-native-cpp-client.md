# 0157: Native C++ client and SDK reference adoption

Status: implemented; linked native and macOS arm64 SDK format validation passed.

The originating-table admission policy is superseded by
[ADR 0250](0250-native-client-acquisition.md). The earlier acquisition and
validation observations below remain historical.

## Context

Native lifecycle, explicit audio state and readout companions need a reusable
owner in C++ consumers. Adding tables without a real format consumer does not
validate their composition. Existing Gain controllers already own a complete
publication/state protocol and host-mediated editor/meter delivery.

## Decision

Add SDK-independent C++17 `ravet::native::NativeProcessorClient`. Control
construction validates exact v1 shapes/callbacks/precisions and optional complete
state/readout profiles, acquires originating caches, then creates the processor.
Failed partial construction retires acquired caches during stopped destruction.
The owner is noncopyable/nonmovable, retains no mailbox/codecs, and forwards
lifecycle/processing/timing, optional explicit state and separate telemetry.
Cached metadata reads are shared; every DSP callback requires exclusive ownership.
Exact library/types/foreign ranges and stopped retirement remain caller duties.

Matching callback profiles establish the supported binding; table addresses can
differ after Rust constant promotion. These checks do not authenticate arbitrary
foreign code and require the selected library's callback identities to agree.
Profile shape or factory failures stay invalid; no fallback is supplied. Native
callback codes/result/silence/poison semantics remain unchanged. Normal forwarding
uses no C++ heap actions, locks or I/O; native/consumer real-time duties still apply.

Gain's VST3 processor owns separate f32/f64 clients and retains cached parameter
input profiles. SDK preparation allocates/prepares the selected precision, clears
old preparation and marks complete state pending. The first fully validated audio
callback (including zero-frame flush) applies the currently published complete
state. Tagged global events go directly to the generic processor. Complete base
snapshot afterward feeds the existing atomic generation/CAS reflection protocol,
so newer controller restores win and unchanged state does not restart smoothing.
Telemetry preserves the existing12 meter IDs/display normalization/host delivery.
Programs, controller codecs, editor policy and exported legacy C functions remain
independent consumer behavior. Shared panic poison disables further SDK processing
until fresh stopped setup; rejected base setup deactivates the new preparation.

## Limits

No generic SDK factory/bus/editor/state protocol, performance/scoped/modulation or
host transport mapping, generated-event delivery, automatic publication/retry or
host/UI scheduling is added by this owner. The reference still exposes mono/stereo global gain
and bypass and its existing optional program/editor profile.
[Decision 0158](0158-vst3-block-start-transport.md) adds the separate SDK
block-start transport mapping after this adoption. The consumer's 31-bit packed generation protocol and
its existing wrap/coalescing limits remain unchanged. Rust/lib/foreign callback
behavior is not certified by C++ new/delete counters or local native checks.
Actual validation is recorded after gates; no new DAW/device/deadline/platform or
production AAX signing claim follows.
