# 0106: Bounded CLAP native audio-owner handoff

Status: implemented lifecycle bridge; graph/device routing is separate.

Portable graph preparation/retirement cannot infer CLAP's split thread lifecycle.
Keep instance activation/deactivation on main, transfer stopped audio tokens to
an audio owner, and keep started tokens private in a non-Send/non-Sync running
engine. Native process/reset and stop stay on that owner. Finish returns a Send
ready state; main drains unstarted proposals/current/retired tokens and explicitly
deactivates matching retained instances. Consumer tags/configuration compatibility
and request scheduling are not inferred from token identity.

Use the existing pinned rtrb 0.4.0 for 1..4-slot SPSC ownership queues. Native
callbacks get no framework locks, allocation or destruction in normal operations.
Reserve retirement capacity before consuming a candidate. Start the candidate
before stopping the old processor; a rejected start returns its stopped token to
control and keeps the old processor. Retirement pressure/abandoned control leaves
current processing intact. No automatic tail drain, state migration or crossfade.
Drop/finish and all endpoint destruction are outside callbacks, with matching main
instances retained until stopped tokens return; no implicit native unload occurs.

Synthetic native callbacks check actual main/audio thread ownership, failure,
backpressure and complete shutdown. Linked/binary effects check retained audio and
measured allocation behavior. This does not provide graph port/config mapping,
native device scheduling, other format hosting, crash isolation, arbitrary plugin
real-time behavior or physical deadline evidence. A dedicated audio worker can
retain this owner across blocks; integration with native callbacks requiring Send
closures needs explicit worker/thread ownership rather than moving a running
engine between callbacks or pretending portable graph retirement stops CLAP.
