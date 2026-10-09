# Bounded OSC packets and explicit UDP

The explicit `ravet-osc` crate provides worker/control-thread packet codecs and
UDP transport. It is outside the processing umbrella. Decode/encode allocate
metadata; sockets may block. No socket, destination, receive thread, dispatch,
audio event bridge or background network activity is created implicitly.

The codec follows the [OSC 1.0 wire specification](https://opensoundcontrol.stanford.edu/spec-1_0.html)
for big-endian integers/IEEE floats, padded ASCII strings/blobs and nested bundles.
It supports core tags i/f/s/b plus h/d/t/T/F/N/I. Other tags, including arrays,
MIDI atoms, symbols, colors and characters, reject explicitly. Address patterns
must begin with slash and are preserved; matching/dispatch is not implemented.
Bundle timetags preserve raw NTP bits (1 means immediate). Temporal nesting rules,
clock synchronization, scheduling and atomic dispatch are consumer responsibilities;
this is a codec/transport, not a complete OSC server.

Limits are 65504 packet bytes, eight nesting levels, 256 total packets and 256
arguments per message, with address length up to 1024 bytes. Decode validates
four-byte packet alignment, exact scalar lengths, blob bounds, ASCII/padding,
tags and trailing data. Argument strings/blobs borrow the input; metadata owns
bounded vectors. Nonfinite IEEE values retain their bits, so consumers validate
values before translating them into processor parameter events.

`OscSocket::bind` takes an explicit numeric interface/port, a packet byte limit
(divisible by four, 4..65504) and positive timeout at most 300 seconds. Send takes
an explicit numeric destination; no DNS/peer discovery or credentials are inferred.
Receive returns the sender and borrowed decoded packet; Rust prevents another
mutable receive while borrowed data lives. Oversized datagrams reject, including
truncated reads detected by extra scratch capacity. Timeout/I/O errors retain
platform error kind. There is no authentication/encryption or reconnect policy.

Four fixtures check an independent gain packet, every truncated prefix of it,
optional scalar/nested bundle roundtrips, strict padding/tags/bounds and an
explicit localhost UDP exchange with oversized-datagram rejection and recovery.
Only synthetic loopback on macOS arm64 has been executed; no remote/device,
real-time or cross-platform support claim follows. See
[decision 0030](decisions/0030-osc-codecs-transport.md).
