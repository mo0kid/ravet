# CoreMIDI UMP endpoints

`ravet-core-midi` is an explicit macOS-only backend outside the portable umbrella.
It builds a narrow Objective-C++ CoreMIDI ownership shim with local Xcode tools;
Rust owns names, packet framing, queue preparation, delivery and statistics. Other
workspace targets compile the package with no backend API. Cross-compilation is
not configured. The UMP APIs require an Apple SDK providing the protocol-selected
CoreMIDI facilities; runtime evidence here is macOS 27.0.1 arm64 only.

`sources()` and `destinations()` return bounded control-thread snapshots of at
most 4096 exact signed unique IDs and complete UTF-8 display names (1023 bytes
maximum). Names need not be unique. Concurrent removal/metadata failure rejects
the snapshot; there is no implicit reconnection or fallback to another device.

`CoreUmpInput::connect` selects a source ID, explicit MIDI 1/2 receive protocol
and 1..4096 packet queue. `virtual_destination` instead owns an explicit endpoint
and returns its unique ID. `CoreUmpOutput::connect` targets an exact destination ID
with explicit send protocol. `send` submits one complete framed numeric packet;
native success is submission, rather than an application acknowledgement. The
native service can translate to a peer's protocol; this crate does not perform
MIDI-CI negotiation or convert semantic messages itself.

`CoreUmpSource::new` owns a named protocol-selected virtual source. Its `id`
permits exact source subscriptions; `send` distributes to every connected input
port. Other applications can discover and subscribe to this visible endpoint.
The backend opens no hardware connections automatically. IDs are assigned per
lifetime; persistent IDs and reconnection remain consumer policy. Closing a
source removes it from enumeration without destroying subscriber input owners.
This follows Apple's [virtual source API](https://developer.apple.com/documentation/coremidi/midisourcecreatewithprotocol(_:_:_:_:))
and [event distribution API](https://developer.apple.com/documentation/coremidi/midireceivedeventlist(_:_:)).

Timestamps are CoreMIDI host-clock ticks. Zero output means now. Queue records
retain callback timestamps; no conversion to connection-local microseconds or
audio sample offsets occurs. Framing accepts complete packets independently of
semantic/protocol validity. Consumer policy must choose supported messages.

Native callbacks bound lists to 1024 event packets/4096 total words before Rust
entry. Each native packet's word batch receives framing preflight and normal
accepted-prefix/full-suffix handoff; earlier native packets can already have been
published if a later packet is malformed. Metadata errors and concurrent native
callback entry have separate diagnostics. A nonblocking atomic gate discards an
overlapping whole callback; unique mutable Rust producer ownership is preserved.

Construction/close belong outside callbacks. Native handles have thread-local
Rust ownership and cannot be moved/shared between threads; construct an output
inside its selected worker. Receive endpoints transfer a separate Send consumer
to audio/worker code. Closing gates new Rust callbacks, disposes native ports,
waits for any entered callback, then frees the Rust context on the owner thread.
No physical-device, sample-clock, hotplug or real-time deadline claim follows.

Run an explicit synthetic native test without sending to hardware:

```sh
CARGO_BUILD_JOBS=2 cargo run -p ravet-core-midi --example virtual-probe --locked
CARGO_BUILD_JOBS=2 cargo run -p ravet-core-midi --example source-probe --locked
```

The local macOS run preserves MIDI 1/2 numeric words across all 16 groups, including
MIDI 2 16-bit velocity and attribute data, for 96 packets/six owned endpoint cycles.
It checks exact-ID enumeration, diagnostics, queue producer teardown and endpoint
removal. Ten additional cycles close owned destinations while worker outputs
submit packets, then verify producer teardown, bounded retained packets and
endpoint removal. Framework callback fixtures exercise 10,000 raw-entry handoffs and
30,000 rejected metadata cases with zero measured heap operations. The native
service and SDK wrapper internals have no heap/deadline guarantee. The source
probe also passes 198 exact deliveries to two subscribers across all groups and
six MIDI 1/2 lifecycles, direction mismatch/name rejection, independent subscriber
teardown and source removal/new-connection rejection. Physical MIDI 2 devices,
protocol negotiation and other OS backends
remain separately unvalidated. See
[decision 0061](decisions/0061-core-midi-ump-endpoints.md) and
[decision 0072](decisions/0072-owned-ump-sources.md).
