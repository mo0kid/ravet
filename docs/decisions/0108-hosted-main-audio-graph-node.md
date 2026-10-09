# 0108: Prepared hosted CLAP f32 main audio graph node

Status: implemented fixed main-audio profile; other mappings are separate.

Main consumes a trusted Clack instance, validates fixed port/configuration bounds,
allocates f32 native scratch/port arrays/one token return slot and activates through
a caller-supplied host audio-handler factory. Retain the non-Send main control
until graph audio sessions stop and control retirement returns its stopped token.
The Send node prepares by validating the already activated fixed config, starts/
processes/resets/stops on audio, then returns a stopped token without native main
callbacks. Main reclaim deactivates exactly once; inactive instance extraction
permits editing/fresh activation. No graph reprepare implicitly reactivates.

Map zero/one main port per direction, up to 64 channels, 65536 frames and at most
32 MiB node-owned float scratch. Graph-owned memory cap excludes native/node
storage. Input copies into mutable native buffers; cleared output scratch avoids
stale unwritten output. Empty blocks skip native calls. Events, transport, f64,
auxiliaries, editor and active parameter mapping remain separate implementation.
Graph latency/tail are explicitly declared by the consumer, not guessed or queried
on the wrong thread. Success status telemetry does not automatically sleep/drain.

Add BackendFailure to the portable error contract for runtime backend failures:
output/events still clear, but native histories may have advanced and cannot be
rolled back. Graph disables until explicit reset; native reset while stopped is
deferred to next audio start. Node-local steady time does not follow playback
loops; overflow becomes unavailable until reset. Normal processing introduces no
framework allocation, locks or I/O; exceptional/plugin behavior is separate.

Phase/owner guards reject main audio start and require orderly shutdown. Main
control destruction before token retirement follows Clack's existing protective
leak behavior. The native token is never destroyed/deactivated on an incorrect
owner as a substitute for complete shutdown. Explicit core callback service uses
its installed mailbox, without polling or implicit restart/process scheduling.

Linked native callbacks and own/upstream actual gain graph fixtures establish
thread order, cold prepare, rejection/failure/reset/steady/sleep handling, wet/dry
routing, dynamic stopped retirement and heap behavior. These do not establish
arbitrary third-party compatibility, other format/device scheduling, platform
support or physical deadlines. See WORK_STATUS for exact native scope.
