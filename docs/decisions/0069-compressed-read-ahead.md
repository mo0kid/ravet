# 0069: Compressed worker-to-audio read-ahead

## Context

Lazy compressed reads cannot execute on an audio callback. Prepared handoff needs
bounded worker buffering, retained backpressure suffixes and generation-safe seeks.

## Decision

Add an explicit compressed worker pump over the existing unique-endpoint audio
ring. Prepare bounded scratch once; publish whole-frame prefixes, retain suffixes
and delay another read until they are queued. Decode errors poison the pump and
publish none of the failed read, acknowledging that earlier queued/consumed audio
cannot be rolled back. EOF is decoder verification, not audible completion.

Stopped seeks reclaim both matching endpoints, complete atomic reader replay
first, then clear pending/queued old-generation data and poison. Failed replay or
foreign endpoints preserve queues. No hidden thread, device/rate fallback or
sample-clock scheduler is added.

## Consequences and validation

Workers may allocate/block during decoding/replay; consumer pop retains the
prepared ring contract. Rate matching and lifecycle remain explicit consumer
obligations. Endpoints are not cloned and must be returned after users stop/join.

All three synthetic encoded formats cover backpressure/exact seek/generation,
threaded sample order, measured heap-free consumer transfers and deferred fault/
poison recovery. These tests do not establish native device playback/deadlines.
