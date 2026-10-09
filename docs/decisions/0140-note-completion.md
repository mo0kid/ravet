# 0140: Directional note completion with carrier ordering

Status: portable event/graph/queue and opt-in hosted/exported CLAP notification
profiles implemented; synthetic offline evidence only.

CLAP NoteEnd addresses the original note input tuple and ignores time/velocity.
Treat it as a processor-to-controller notification, separate from key release,
choke and ordinary generated note output. Preserve portable matching targets and
u32 identities, with checked native nonnegative i32 limits. Do not synthesize
completion from Off/choke or infer voice tracking/ID reuse policy.

Use the existing bounded event carrier to avoid separate allocations/lifecycle
queues. Its offset orders delivery, not voice termination. Hosted native time
never participates in ordering: canonicalize to the last retained carrier offset
or zero, preserving insertion order without moving the timed-event cursor. Check
selected raw scope before normalization and ignore velocity. Other/wildcard ports
filter; selected faults/capacity discard portable output without native rollback.
Require an inactive-validated CLAP input route and explicit bounded capacities.
No note output/map/identity profile is required. Portable hosted completion input
rejects whole prefixes; parameter-only zero-frame flush rejects completion output.

Exported main/multibus choose an independent originating input endpoint for
completion delivery. Whole generated preflight rejects missing route/too-wide ID
and flush before host prefixes, while successful DSP/audio remain progressed.
Host refusal discards suffixes without replay. Incoming native completions filter,
with existing generic header/count/timing preflight unchanged. Public reference
voice behavior/metadata stay consumer-owned. No dependency change.

Tests cover portable validation, graph filter and 10000 queued roundtrips; raw
native scope/timestamp/ignored velocity/flush; linked f32/f64 1000-block no-note-
output/no-output-map profiles, filtering, invalid prefixes, capacity/nonrollback;
exported f32/f64/multibus output, independent routes, flush/width/host refusal and
heap instrumentation. No DAW, third-party voice lifecycle or deadline claim.
