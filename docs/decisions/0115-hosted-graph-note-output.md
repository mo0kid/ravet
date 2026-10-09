# 0115: Selected hosted graph performance output

Status: implemented within the bounded selected-port contract.

## Decision

Validate one inactive native output index/dialect independently of input and
parameter mappings. Share prepared output slots and fault/offset/order handling
with global parameters; permit performance-only profiles without params metadata.
Preserve MIDI short bytes and framed UMP without protocol/group translation.
Translate only concrete CLAP channel/key with wildcard note ID to portable
normalized notes. Other targets/identities/expressions filter explicitly, since
the portable model cannot retain their native meaning.

Selected malformed events poison the entire block, clearing audio and output
prefix with nonrollback BackendFailure. Native processing history may advance;
there is no retry. Recognized performance during zero-frame parameter flush
rejects rather than misusing params.flush as note delivery. Unused short/UMP
padding is ignored. Native f64 normalized velocities narrow to portable f32.

## Evidence and limits

Codec tests cover narrowing, wildcard/specific targets, invalid values/statuses,
short-message length/padding and one/two/three/four-word framed UMP. Linked f32/f64
profiles check filtering, malformed data/velocity, capacity, empty flush and
advanced native state after output faults, with no measured callback heap actions.
Loaded public echo fixture checks all four dialects in both precisions through
host/node graph event edges, dynamic swaps and orderly retirement. Evidence is
macOS arm64 synthetic processing, without broader plugin/DAW/device/platform or
real-time deadline support. Note IDs, choke/end/expression, SysEx, rescan service,
modulation/gestures/editor/device scheduling remain separate work.
