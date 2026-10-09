# 0079: Captured sostenuto ownership

Status: implemented portable receiver policy; native controller validation pending.

## Context

A global pedal boolean cannot represent sostenuto: only notes held at the pedal
edge participate. Sustain and sostenuto must release independently without
retaining stolen slots or confusing later notes. The
[MIDI Association table](https://midi.org/midi-1-0-control-change-messages)
defines CC66 switching and the
[Yamaha implementation](https://usa.yamaha.com/files/download/other_assets/5/320475/CBXK1XGE.pdf)
describes holding specific notes before playing subsequent notes.

## Decision

Retain one private capture bit per voice and edge state per channel. Capture
physically held notes only on off-to-on, expose a read-only latch and report
latch changes in existing slot masks. Note-off uses either sustain or captured
latch; pedal release frees only sustained slots with neither remaining owner.
CC121 clears both owners; slot replacement/defaults clear capture. Legacy MPE
member reuse forcibly releases its old note regardless of pedals, as before.

Polyphony remains channel-local; MPE pedals remain master-only, isolated by zone,
including compatibility master notes. Existing member-expression freeze/master
expression rules apply to sostenuto-held notes without another voice phase.
This extends 0027/0059 receiver policy and the sine instrument; synthesis keeps
its existing envelope and no piano-specific acoustic model is introduced.

## Validation and limits

Tests cover duplicates, threshold edges, repeated presses, later/sustain-held
notes, both pedal release orders, channel/zone isolation, compatibility masters,
reset/kill/finish/steal/configuration/legacy reuse and 128-slot heap-free cycles.
Sample-timed sine audio persists while captured, then releases exactly with
identical split-block output and no allocation/deallocation. Native CLAP export
regression is separate evidence; no DAW/controller/half-pedal conformance claim.
