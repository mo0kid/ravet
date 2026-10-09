# 0060: Prepared UMP device handoff

## Context

MIDI 1.0 device handoff uses bounded byte messages and connection-local
microsecond timestamps. UMP native transports carry numeric words and can use
host-clock ticks; putting them in that byte-message API would imply conversion
and an incorrect clock domain.

## Decision

Add separate unique prepared UMP producer/consumer endpoints with fixed framed
packets, opaque timestamps and explicit MIDI 1/2 protocol labels. Use pinned rtrb
0.4.0 SPSC storage (already in the workspace's graph mechanism). Bound queue
capacity to 1..4096 packets and word batches to 4096. Validate whole batch framing
before publication; capacity refusal keeps an accepted prefix and discards the
suffix, with explicit delivery/statistics. No implicit retry or clock conversion.

## Consequences and scope

Construction and endpoint destruction happen outside callbacks; push/pop use no
framework allocation/locks/I/O. Protocol labels do not prove semantic conformance.
Native producers own clock definition, protocol negotiation and serialized
callback delivery. This mechanism does not connect a UMP device. Tests preserve
all framed message types/words/timestamps, reject malformed whole batches, cover
prefix overflow/abandonment and exercise prepared/cross-thread heap-free handoff.
