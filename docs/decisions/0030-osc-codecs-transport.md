# 0030 — Bounded OSC codecs with explicit worker UDP

Status: accepted, 9 October 2026.

Keep allocating protocol metadata and blocking sockets outside portable callbacks.
Offer bounded wire codecs and explicit numeric bind/send destinations. Preserve
borrowed strings/blobs and timetags; reject malformed/unsupported packets rather
than guessing missing type tags. No default transmission or receive task exists.

Do not claim an OSC server from a codec: scheduling, nested timetag semantics,
pattern dispatch and application-to-audio event delivery remain consumer policies.
Bound UDP scratch with extra capacity to detect truncation/oversize. Test public
wire fixtures and localhost fault/recovery without accessing any remote service.
See [the guide](../osc.md) for exact capabilities and limits.
