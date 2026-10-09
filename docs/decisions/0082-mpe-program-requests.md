# 0082: Master-routed MPE bank/program requests

Status: implemented portable receiver integration; native conformance separate.

## Context and decision

The decoder from 0080 stages wire data but knows no zones. Both negotiated voice
receivers now own one decoder and report optional requests in `NegotiatedChanges`
without changing voices or configuration flags. Polyphony accepts independent
channels; active zones accept only their masters, including zero/15 compatibility
masters. Member/unassigned bank changes are ignored before staging. The
[MIDI Association MPE overview](https://midi.org/copy-of-midi-polyphonic-expression-mpe-specification-adopted)
describes master-channel Program Change. Requests remain separate from consumer
sound availability/commit/current identity and existing-note switching policy.

Any accepted RPN6 configuration clears all bank/request state with existing
complete controller reset, including unrelated-zone masters and repeated
configurations. Explicit/System Reset also clear while retaining zones/ranges.
RPN0 range updates and CC121 preserve bank/request state. Last accepted request
inspection is bounded and returns None for invalid channels.

Sine consumes the single-zone receiver's request, removing duplicate decoder
ownership from 0081. It still accepts its chosen master only in polyphony and
changes global level without voice/phase reset. Its accepted zone configurations
now also clear pending banks; this refines 0081's reset behavior.

## Validation and limits

Two test groups cover both single-zone sides, ordinary fallback, exact independent
master banks, member/unassigned suppression, unchanged voices/configuration,
range/reset/repeated configuration behavior and 10000 heap-free cycles per side.
Sine/native export regression checks remain separate evidence. No committed
General MIDI sounds, host program menus, native expression conformance or MIDI2
program synthesis is claimed.
