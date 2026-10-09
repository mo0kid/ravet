# 0032 — Explicit MIDI connections and bounded packet handoff

Status: prototype; CoreMIDI virtual-loopback evidence on macOS arm64.

Native MIDI access is optional and stays outside the portable umbrella. Use
MIT midir 0.11.0 rather than write a second native device layer. Each connection
selects a current explicit backend ID and is owned by a control/worker thread.
No default fallback, implicit input capture, output destination, reconnect or
timestamp-to-audio conversion is chosen by the framework.

The input callback validates complete packets and copies them into unique SPSC
endpoints with atomic prepared slots. At most 1,024 packets of at most 1,024 bytes
are retained. Full input drops the newest whole packet; malformed/oversized input
is rejected and counted. Short messages use Foundation validation; complete
SysEx must have F0/F7 framing and seven-bit payload. Fragmented SysEx, embedded
real-time bytes and UMP require different protocols. All packets preserve raw
connection-local microsecond timestamps. Consumer pop returns owned stack storage.

Framework queue capture/pop do not allocate, lock or perform I/O. Backend
callbacks and native output send have separate costs and no deadline guarantee.
Teardown may wait and belongs outside callbacks. Unix virtual endpoints are
explicit and the validation fixture sends only to its own temporary endpoint.
Threaded, malformed/full, rollover and heap fixtures complement actual macOS
virtual transfer; no physical device or other runtime backend is claimed.
