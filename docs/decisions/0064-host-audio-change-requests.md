# 0064: Deferred hosted audio change notifications

## Context

Owned port/timing snapshots need native change notifications before routing can
be refreshed. Native callback threads cannot rebuild graphs or reenter plugins.

## Decision

Provide opt-in combined ProcessingHost handlers for parameters, audio-port,
latency and tail extensions. Atomic coalesced flags bridge main-thread port/
latency callbacks and audio-thread tail callbacks to explicit consumer scheduling.
Prepared clones own callback storage. All defined port rescan flags are supported;
consumers must deactivate/reinspect on configuration changes and re-query timing
on the correct owner threads. Unknown public deferred bits are masked.

## Consequences and validation

The handoff neither guarantees an ordered configuration history nor implements
restart, graph rewiring, event-loop wakes or latency compensation. A consumer
that advertises these handlers must service/defer requests under its lifecycle
policy. Deferred callbacks never allocate, lock or invoke plugin code themselves.

Actual linked fixtures invoke port rescan on main, latency during activation and
tail from audio processing. Tests check merged flags, deferral, unknown bits and
prepared heap behavior. The offline inspection tool integrates these handlers
and reports requests/timing; this is not a native device/graph validation claim.
