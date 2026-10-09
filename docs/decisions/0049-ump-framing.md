# 0049: Fixed-storage UMP framing

UMP messages have one to four words, with length determined by the first
message-type nibble. Reserved types also have defined lengths. Preserve opaque
payloads while checking exact packet boundaries rather than treating framing
as semantic or protocol conformance. Numeric words avoid implicit platform
endianness; explicitly named big-endian helpers are an application representation,
not a claim about every transport.

The incremental decoder uses four fixed words, emits complete packets, and
reports truncation without losing partial state. Reset requires a known new
boundary. Only defined grouped types expose a group. Neither framing nor byte
conversion allocates, performs I/O or changes the MIDI 1.0 processor contract.

Tests cover all sixteen lengths, reserved data, incomplete prefixes, reset,
independent MIDI 2 note bytes, destination rejection and unchanged tails.
Semantic codecs, native device delivery and protocol negotiation are separate work.
