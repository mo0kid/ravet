# 0038 — Prepared memory playback transport

Date: 9 October 2026. Status: implemented within stated limits.

A playback transport should not perform file seeks or decoding on audio. Supply
an explicit bounded owned memory clip first, separate from WAV read-ahead and
recording workers. Interleaved f32 copying, exclusive commands and a frame cursor
provide deterministic play/pause/seek/loop behavior without a device dependency.
Preparation caps copied samples at 16,777,216 and renders at 65,536 frames.

Pause freezes position. Stop returns to the loop start or zero. Loops are
half-open and normalize outside positions to their start. EOF stops and clears
the output suffix. Invalid render buffers silence before rejecting and never
advance; invalid commands preserve state. Consumers match sample rates and
serialize commands on the audio owner. No resampling, smoothing, streaming
seek, cross-thread command queue or physical-clock evidence is implied.

Tests exercise sample-exact boundaries, zero-frame calls, rejected mutation and
10,000 heap-instrumented prepared workloads. A runnable synthetic WAV example
connects decoding to the prepared transport without device output or disk files.
