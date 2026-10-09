# 0050: Full-resolution MIDI 2.0 channel voice codecs

Separate semantic channel voice decoding from opaque UMP framing. Validated
messages cover defined MT=4 statuses and retain full velocity/control resolution,
unsigned bipolar pitch and signed relative controller changes. No implicit MIDI
1.0 conversion or normalized float loses precision. Group/channel and seven-bit
sender fields validate before construction; per-note controller indices use eight
bits. Controller/profile policy stays with consumers.

Receivers ignore reserved bits for future compatibility, while encoders zero them.
Unknown note attributes retain their data; type-zero ignored attributes and absent
bank bytes canonicalize. Exact original bits stay available in UmpPacket. A zero
velocity Note On remains Note On. Reserved status 7 rejects semantic decoding.

Independent numeric fixtures cover every defined status, signed/bipolar extrema,
all group/channel combinations, reserved bit masking, optional banks, future
attributes and sender validation. Native UMP delivery, protocol negotiation and
MIDI 2.0 synthesis/voice policy remain separate work.
