# 0136: Explicit scoped key release and choke

Status: portable events and selected hosted/exported CLAP conversions implemented;
offline native fixture and adapter evidence only.

Reuse NoteTarget optional channel/key/ID as matching scope within a routed endpoint.
NoteOffScoped is a key release with checked finite f32 velocity 0..1; NoteChoke
carries no velocity. Receivers own matching, envelope and actual termination policy.
No voice allocation, automatic lifetime/state edit or completion is inferred.

Hosted input/output use independent scoped-note opt-ins with selected CLAP ports.
Input preserves all Off/choke scopes with native nonnegative i32 IDs; default or
malformed input rejects whole prefixes before native processing. Output preserves
wildcard-channel/key Off and choke; concrete Off continues legacy/identity profile
selection. Raw fields validate before Clack's negative normalization and unrelated
ports filter. Native choke velocity is ignored rather than erroneously rejected.
Selected output faults retain native history while discarding portable audio/events.

Exported native performance profiles also carry scoped release/choke. Default
MIDI delivery cannot preserve matching scope and rejects the whole stream before
any prefix. Existing selected zero-frame/parameter flush rules remain. NoteEnd is
separate completion matched to input ports with ignored timestamp; ordinary timed
release conversion must not invent its semantics or completion scheduling.

Portable output/queue/graph tests, selected raw conversion tests and linked f32/
f64 timed native fixture profiles cover validation, filtering, complete prefixes,
output capacity/nonrollback and prepared heap behavior. Exported adapter scopes
roundtrip with exact f32 velocity widening and ignored choke velocity. Public
instrument voice semantics remain unchanged; no third-party/DAW/deadline claim.
