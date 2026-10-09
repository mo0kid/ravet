# 0081: Sample-timed sine factory programs

Status: reference integration implemented; native MIDI/state evidence recorded
separately from DAW program menus.

## Context and decision

Consume the complete borrowed catalogue and staged MIDI requests from 0080 in
the public sine reference, rather than inventing an additional persisted control.
Bank 0 numbers 0/1/2 select Default/Quiet/Full level 0.2/0.05/1, stable IDs 1/2/3.
Only the constructor's chosen master accepts requests, even in initial polyphony,
because this reference has one global level. Unknown addresses/other channels
preserve state. Selected levels apply at event offsets/in same-frame event order
and preserve voice/phase/envelope/zone state. Reset clears pending banks while
retaining published level; identity after custom edits is not inferred.

CLAP audio reflects this level through the existing generation-protected atomic
state. Changed successful publication sets one dirty bit/request_callback;
on_main_thread drains it and calls optional HostParams rescan VALUES. Repeated
requests coalesce; racing new publication remains for a later callback. A failed
stale CAS cannot publish or notify. Audio calls only the thread-safe callback
request, not main-thread parameter rescan. Active level automation/flush uses
the same notification; inactive host edits already use their explicit caller.

## Validation and limits

Portable tests compare exact split-block output/phase-preserving level changes
and both master sides, unknown banks/programs, ordering/reset and fault atomicity.
Actual MIDI ABI renders match analytic levels and preserve unknown/member
requests; main-thread queries/save retain complete program level before another
edit. A linked parameter-aware host verifies deferred/coalesced rescan, stable
state across activations and no processing heap activity. Full bundle validator
results are recorded in work status. No General MIDI sound bank, per-channel
patches, native host program menu, CLAP preset discovery, asset loading or new
DAW program-recall support claim.
