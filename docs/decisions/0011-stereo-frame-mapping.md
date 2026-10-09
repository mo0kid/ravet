# 0011 — Stereo frame mapping over borrowed storage

Status: safe mapper implemented; public API provisional.

Repeated per-frame channel queries can repeat storage/layout branching inside a
DSP loop. `AudioBuffer::map_stereo_frames` selects in-place or separate storage
once, then iterates validated active prefixes. Its callback receives the offset
and two input samples and returns two outputs. Zero or one input supplies zero
for missing channels, retaining the existing instrument and mono semantics.

The mapper reads both in-place samples before writing either output, preserves
backing tails, and performs no allocation itself. Unsupported channel counts
return false before any callback or mutation; zero active frames invoke no
callback. This helper does not replace processor preparation, layout, event
validation, or the callback's real-time obligations.

Tests cover in-place swaps, separate zero/mono/stereo inputs, unequal backing
capacities, active prefixes, zero frames, and unsupported input/output counts.
Consumer-specific CPU and audio comparisons remain private evidence.
