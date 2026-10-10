# 0143: Complete native event records

Status: v1 C-layout event conversion and shared legacy parameter decoding are
implemented and validated with Rust/C/C++ and rebuilt native-bundle contracts.
SDK-specific registration/routing remain
separate work.

Keep native marshalling in ravet-native without changing portable event memory
layout or Gain's existing C ABI. Use a named v1 fixed record with numeric kind,
explicit wildcard-presence bits, complete u32 note/parameter identities, full f64
scoped values/expressions and exact widened f32 global values/velocities. Never
narrow silently. Reject unknown tags/bits, nonzero unused fields, invalid values
and incomplete MIDI/UMP framing. Preserve UMP numeric words including reserved
framed messages, without inferring semantic/protocol support.

Bulk conversion uses caller-prepared storage and validates the whole bounded
stream before writing any destination prefix. Preserve offsets/equal-time order,
zero-frame offset-zero events and destination tails. Conversion owns neither
endpoint selection, event delivery, voice allocation nor state changes; consumers
retain those policies and foreign-memory safety. The record is a native in-process
layout, not byte serialization or a portable disk/wire protocol.

Share the legacy global parameter record/decoder with Gain and replace its private
input conversion loop. Keep separate tagged performance records rather than
silently reinterpreting existing parameter-only hosts. C/C++ consumers compile
matching headers; Rust tests validate canonical records and complete prepared
native audio/event cycles in both precisions.
