# 0159: Explicit zero-volume note expression

Status: implemented; local code and format validation passed.

## Context

Portable standard expression ranges originally followed CLAP's strictly positive
linear volume. Other native expression protocols can represent mute as zero.
Clamping mute to an epsilon changes its meaning and cannot preserve exact values.

## Decision

Widen portable `NoteExpression::Volume` to finite 0..=4, with zero meaning mute.
Signed zero is numerically mute and retains its f64 bits in used native fields
and complete portable worker queues; positive subnormals remain valid. Negative
nonzero values, nonfinite values and values above four reject without clamping.
Receiver voice matching, defaults, smoothing and application remain explicit.

Native v1 records retain their existing size, kind/selector and scope conventions.
This changes the accepted scalar domain of a provisional API, not the layout;
older consumers may still reject zero and must be updated explicitly. No implicit
cross-library version negotiation or fallback is introduced.

Hosted and exported CLAP conversion performs separate format-specific validation
for positive volume at both input and output boundaries. Selected zero input
rejects before DSP; a generated zero volume makes the complete stream
unrepresentable before any host delivery, retaining rendered audio/DSP. Unknown
expression kinds and unrelated ports retain existing filtering. No epsilon,
choke, release or completion event is substituted for mute.

## Validation and limits

Tests cover both zeros and minimum positive/negative subnormal values, exact
native round trips across scopes, complete worker handoff without heap actions,
CLAP selected input rejection/filtering, positive exact conversion and complete
generated prefix rejection. This scalar contract supplies no instrument voice
application, VST3 mapping, DAW/device/platform or callback deadline evidence.
