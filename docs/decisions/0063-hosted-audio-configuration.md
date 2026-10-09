# 0063: Owned directional hosted audio metadata

## Context

Hosted graphs need explicit native port layouts, precision capabilities and
reported timing. Assuming one stereo bus or synthesizing timing zero when an
extension is missing hides important configuration information.

## Decision

Inspect bounded inactive audio-port metadata into owned directional snapshots.
Preserve stable IDs, flags, names/type bytes and opposite-direction in-place IDs;
validate uniqueness, main placement, channel/count limits and resolvable equal-
channel pairs. Refresh replaces a snapshot only after complete validation.
Do not infer portable auxiliary pairings or change native configuration.

Query optional latency on the main thread and optional tail through a live audio
processor handle. Missing declarations remain None. No native extension pointer
is stored in a snapshot or carried beyond its instance's lifetime.

## Consequences and validation

Nine ports per direction/64 channels per port are explicit inspection limits.
Other layouts need separate mapping work. Main-thread preparation and audio-owner
tail queries stay separate; consumers own native notifications, graph compensation
and scheduling. Offline tests establish metadata behavior, not host deadlines.

Linked sidechain/instrument references and malformed synthetic declarations cover
owned bytes, directional IDs, precision flags, pairs, atomic refresh and active
rejection. Timing fixtures preserve exact latency and finite/infinite tail and
measure prepared tail-query heap behavior. Actual built sidechain metadata is
inspected separately through the explicit trusted native loader.
