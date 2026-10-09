# 0111: Opt-in hosted block-start transport

Status: implemented checked f32/f64 node mapping.

Preserve default native context omission. Enable explicitly on main while the
node token is stopped; running or other-thread configuration rejects. A public
allocation-free helper validates rate, optional fields and signed fixed-point
bounds before constructing the native event. All absent fields yield None; any
known field requires known playing because CLAP has no playing-valid flag. Never
turn unknown playing into an implicit stopped state.

Flag only known tempo/beat/seconds fields. Tempo is finite positive. Timeline
conversion rounds to native signed 31-fractional-bit time and rejects overflow
rather than relying on saturating casts. Samples convert through f64 seconds;
quantization/large-position precision limits are explicit. Native tempo increment,
loops, recording, time signature and other unused fields remain zero/unflagged.
No native discontinuity bit exists; do not reset histories implicitly. Node-local
steady time remains independent of playback/seek positions.

Add InvalidTransport as a portable format-representation validation fault, before
native calls and history/clock mutation, with output silence. Graph reset policy
still applies after a fault. Disabled nodes continue to ignore portable context.
Linked callback validation checks malformed rejection and unchanged native call/
clock/history counters; pure fixtures check flags, boundaries and quantization.
Loaded public fixture uses validated playing/stopped context to render x2/silence
in both precisions through wet/dry swaps and main retirement with measured heap
checks. macOS arm64 evidence does not establish other platform/device/DAW support.
