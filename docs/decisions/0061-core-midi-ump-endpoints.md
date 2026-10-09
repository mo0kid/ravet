# 0061: Guarded CoreMIDI UMP endpoints

## Context

Prepared UMP handoff needs a native producer that preserves words and an explicit
clock domain. Existing MIDI 1.0 device APIs cannot promise native MIDI 2 precision
or CoreMIDI host-tick semantics. Native block/context lifetime and concurrent
callback entry require a narrow ownership boundary.

## Decision

Add an explicit macOS backend crate with a small Objective-C++ CoreMIDI shim.
Rust owns packet framing/queues and safe endpoint selection; foreign pointers
remain inside its deny-by-default, explicitly allowed FFI module. The shim owns
clients/ports/blocks, bounds native list traversal and atomically admits one Rust
callback. Overlap discards a whole callback. Close first gates entry, disposes
ports, waits for entered Rust callbacks, then releases the Rust context on the
control owner. Handles do not implement Send/Sync. Native output uses a fixed
stack event list for one complete framed packet on a control/worker thread.

## Consequences and validation

Exact native IDs never fall back to another endpoint. Consumer protocol/clock/
negotiation semantics stay explicit. MIDI service conversion and physical-device
behavior need their own evidence. The backend exposes no API on other targets;
Xcode/macOS builds and the macOS 27.0.1 arm64 runtime are the executed scope.
No audio deadline or physical MIDI 2 conformance claim is made.

A preliminary owned-destination SDK probe preserved exact MIDI 2 words. The
reusable Rust backend's native virtual probe then preserved 96 MIDI 1/2 packets
across all groups and six endpoint lifecycles, without hardware output. Rust
callback fixtures test exact words/timestamps, rejected protocol/null/word-limit
metadata and prepared heap behavior. This uses Apple's protocol/event-list APIs:
[CoreMIDI event lists](https://developer.apple.com/documentation/coremidi/midieventlist)
and [protocol-selected input ports](https://developer.apple.com/documentation/coremidi/midiinputportcreatewithprotocol(_:_:_:_:_:)).
