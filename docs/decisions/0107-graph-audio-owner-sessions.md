# 0107: Explicit graph audio-owner sessions

Status: implemented lifecycle mechanics; native node mapping is separate.

Add no-op-default start_audio/stop_audio hooks to Processor without changing
ordinary portable rendering. Explicit borrowed static/dynamic graph sessions
start nodes topologically on an audio owner, expose process/reset/timing only,
and stop nodes in reverse order before control ownership resumes. The guard is
non-Send/non-Sync; do not forget it. Active preparation returns AudioSessionActive
and deactivation requires ended sessions. No existing adapter implicitly opts in.

Failed start must leave its own node stopped. A rollback guard stops earlier
successful nodes on returned errors and unwind, leaving the graph faulted until
reset. While a dynamic session is active, a candidate starts before the old graph
stops. Failed candidates retire and old rendering continues. Successful old nodes
stop before bounded control retirement; deactivation/destruction remains control
work. Existing validation, queue pressure, config/latency rules and no migration/
crossfade/tail-drain contracts remain in force.

Fixtures establish actual hook thread ownership, reverse ordering, error/panic
rollback, retained processing, invalid-input/backpressure, control destruction and
prepared heap behavior, with default f64 compatibility. These hooks alone do not
complete native plug-in graph mapping or device scheduling. Native nodes still
need retained main instances, activated token return, validated ports/events,
prepared native buffers and correct error policy. A dedicated worker can retain
an owner guard; Send native callback closures need explicit worker ownership.
