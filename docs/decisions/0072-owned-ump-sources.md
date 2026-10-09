# 0072: Owned virtual UMP sources

## Context

Exact-ID source connection needs native end-to-end evidence independently of
hardware. Virtual-source distribution is also a reusable application facility.

## Decision

Expose CoreUmpSource with explicit names/protocol and thread-local owned endpoint.
Reuse framed numeric-word validation and stack event-list submission. Native code
only owns the client/source and calls MIDISourceCreateWithProtocol and
MIDIReceivedEventList; Rust retains protocol, ID, validation and API policy.
Construction assigns an ephemeral native unique ID; no persistent-ID mutation,
hardware connection, automatic subscriber or protocol negotiation is added.

## Consequences and validation

Visible sources distribute to all current subscribers, potentially other apps.
Consumers choose names and publication intent. Removing a source does not drop
connected input owners or their bounded producers; owner destruction does that.
Source/destination type mismatch fails without fallback. Native send success is
submission, without an acknowledgement or callback deadline guarantee.

On macOS 27.0.1 arm64, an explicit synthetic probe passes 198 exact deliveries
across six MIDI 1/2 lifecycles, all groups and two subscribers; it also checks
name/direction failures, one subscriber closing independently, source removal,
rejected new connections and final producer teardown. Physical devices, protocol
conversion and persistent hotplug recovery remain unvalidated.
