# 0029 — Static acyclic processor routing with explicit fault progress

Status: accepted for the routing prototype, 9 October 2026.

Own trusted portable processor nodes, compile channel routes and topology outside
callbacks, and prepare scratch plus latency compensation. Keep native plug-in
loading/GUI lifecycle separate. Broadcast input events explicitly; do not invent
per-node parameter identity mappings.

Expose GraphStatus instead of implementing the stricter Processor contract.
Outer malformed blocks reject before mutation, but a runtime failure in a later
node cannot undo earlier DSP histories/parameters. Silence host outputs, disable
the graph and require reset. This limitation is visible rather than claiming
transactional rollback of arbitrary processors.

Prepared route delays align input/output arrivals from node latency declarations.
Graph memory/capacity and tail policies are explicit. Independent impulse/alias/
partition/fault fixtures plus heap instrumentation establish the scoped behavior,
not arbitrary native hosting or deadlines. See [the guide](../audio-graphs.md).

The original f32-only scope is superseded by
[decision 0055](0055-native-graph-precision.md), which adds explicit native f64
precision with the same lifecycle/fault policy and byte-counted storage bound.
