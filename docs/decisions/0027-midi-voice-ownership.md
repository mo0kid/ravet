# 0027 — Explicit bounded MIDI voice ownership

Status: accepted, 9 October 2026.

Use an explicit portable instrument crate for voice policies, separate from
foundation wire validation and consumer DSP. Stable channel/key ownership,
fixed slots, deterministic stealing and changed-slot masks allow allocation-free
callback integration. Consumer envelopes decide when released slots become idle.

Offer configured lower/upper MPE-style member expression without pretending to
implement negotiation. Master bend/sustain and member expression are explicit;
channel reuse freezes earlier released expression. Document receiver policies and
missing RPN/MIDI-CI/master-controller behavior. Devices and format exports remain
separate integrations.

Tests cover boundary policies, expression, channel reuse and age rollover with
prepared heap instrumentation. See [the MIDI guide](../midi.md) for limits.


9 October 2026 extension: expression snapshots now retain separate master
pressure and CC74 values. Master changes refresh all held/sustained members;
released tails freeze every expression field. The DSP consumer chooses how to
combine master and member pressure/timbre. No arbitrary combination rule or
complete conformance claim is added. Lower/upper, sustain, reset and frozen-tail
fixtures cover this extension. Opt-in negotiation is described by decision 0037.
