# 0059: Separate two-zone MPE voice receiver

## Context

The initial configured/single-zone voice policy deliberately releases earlier
notes on a reused member channel and refreshes sustained member expression. The
MPE receiver rules instead permit shared-channel polyphony and preserve member
expression after note-off. Retrofitting that policy into the sine reference would
change its tested performance contract.

## Decision

Add `MpeReceiver<N>` using one fixed shared voice pool and two negotiated zones.
Keep legacy allocation/single-zone APIs unchanged. Latest RPN6 configuration
shrinks/disables overlap, with the inactive master's channel reusable as a
member. Accepted configuration clears all notes/controllers (including unrelated
zone tails), with explicit changed-slot masks. Zone ranges are independent and
RPN0 updates each zone's common member range or master range.

A private allocator policy enables shared-channel notes, master compatibility
notes, ignored member poly-pressure/zone-only controls, frozen post-note-off
member expression and continued master expression on sustained/released tails.
Pressure/timbre components stay separate for consumers' DSP combination. Ordinary
polyphony is the explicit both-zones-disabled fallback; unassigned channels are
ignored while any zone is active.

## Scope and validation

The public MIDI Manufacturers Association MPE v1.0 specification informed the
zone/polyphony/expression rules; the Association states v1.1 made editorial,
rather than technical design, changes. References:
[original specification](https://d30pueezughrda.cloudfront.net/campaigns/mpe/mpespec.pdf)
and [v1.1 announcement](https://midi.org/the-midi-association-announces-the-v1-1-update-to-the-midi-polyphonic-expression-mpe-specification).

This is a voice/expression component, not full MIDI device/profile certification.
Program/bank selection, extra pedals, mode switching, MIDI-CI and native
synthesis/host integration remain separate. Tests cover all zone-size pairs in
both configuration orders, channel reuse, polyphony/master notes, independent
ranges, sustain/tail expression, isolation/fallback and prepared heap behavior.
