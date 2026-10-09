# 0119: Caller-owned generated events for graph output workers

Status: implemented within the explicit buffered worker prototype.

## Decision

Permit 0..1024 prepared graph output slots and supply caller-owned event storage
through pump_io. Require exact capacity for a fresh graph render. Return a valid
prefix count once on successful rendering, independently of how much audio is
published immediately. Pending suffix publication returns no new events and
leaves event storage untouched; no output cache/replay is added.

Event offsets describe the rendered block, not hardware playback. The caller
owns telemetry handoff, block identity/correlation and GUI/control scheduling.
Keep existing empty-output pump conveniences; positive configured output requires
pump_io storage. Graph faults yield no valid prefix and publish none of the
faulty block, without promising rollback of earlier input/native histories.

## Evidence and limits

Portable tests cover absent/excess output capacity before rendering, once-only
output across full/partial queue backpressure, exact offset/prefix and heap-free
processing. The loaded public parameter fixture checks stepped input/read-only
output through a two-node graph and four-frame queue over 1000 native blocks.
No device/GUI/control queue/clocking or deadline claim; output events are owner-
borrowed values and the consumer must schedule further delivery explicitly.
