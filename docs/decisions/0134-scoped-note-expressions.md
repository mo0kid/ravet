# 0134: Scoped standard note expressions

Status: portable expression event and selected hosted CLAP graph conversion
implemented; synthetic offline evidence only.

Represent expression scope independently of concrete note-on identity. NoteTarget
uses optional channel/key/u32 ID within a routed endpoint; None is a matching
wildcard. Seven named expression units retain f64 precision and standard finite
ranges. Receivers own voice defaults, matching, replacement and smoothing;
expressions do not mutate global parameter state or infer voice lifetime.

Hosted CLAP input/output require a selected CLAP port and explicit expression
opt-in before activation, independently of identified-note opt-in. Native IDs
must fit nonnegative i32. Wrong/wildcard ports and unknown expression kinds filter
on output; selected standard events validate raw -1 wildcard fields and ranges.
Clack 0.2.0 omits Expression in from_raw, so decode matches the complete public
wire discriminators directly. Invalid input rejects the entire prefix pre-native;
selected output faults follow existing complete-output discard/nonrollback rules.
Empty parameter flushes do not accept performance events.

Performance graph routing and complete worker queues retain all bits. Current
exported MIDI output cannot preserve scope, so it rejects the whole generated
stream before any host prefix, retaining rendered audio/DSP. No implicit conversion
to MIDI/MPE/UMP or new public sine voice behavior is introduced.

Tests cover all types/ranges, exact f64 values, wildcard targets, full portable ID
limits, malformed raw native fields, filtering and complete queue/prefix behavior.
Linked native f32/f64 fixtures exercise timed audio and echo, disabled directions,
capacity failure after native mutation, unknown/unselected output and preparation
requirements with measured heap-free processing. These checks do not establish
third-party instrument, DAW, physical device or deadline support. Native plug-in
expression profiles, note choke/end and per-note parameters remain separate work.
