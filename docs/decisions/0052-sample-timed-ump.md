# 0052: Sample-timed UMP and CLAP transport

Add opaque framed UMP to the portable event enum, retaining group and resolution
without forcing conversion to normalized notes or MIDI 1.0. Existing event
capacity and ordered offsets apply. Processors own semantic/protocol policy.

CLAP's fixed four-word event is interpreted using the UMP first-nibble size;
unused words are ignored on input and zeroed on output. The explicit selected
port routes MIDI 1 and UMP packets independently of group values. Keep the old
MIDI 1-only translator and add a combined translator used by MainBusBridge.
Parameter flush still ignores performance packets. Exported dialect declarations
must match processor support; reference processors do not gain MIDI 2 synthesis.

Generated packets use existing whole-stream preflight and host prefix rejection.
Tests echo all sixteen message types, preserve defined words/timestamps, check
port selection/zero padding and run 10,000 prepared callbacks with zero heap
operations. Native MIDI 2 DAW/device performance remains separate validation.
