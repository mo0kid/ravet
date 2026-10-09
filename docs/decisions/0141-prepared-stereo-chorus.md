# 0141: Prepared stereo fractional-delay chorus

Status: portable f64 stereo effect implemented and offline tested.

Prepare bounded storage for every allowed live delay edit rather than resizing
in processing. Validate rate/controls before allocating two buffers of
ceil(rate*0.2)+1 samples, at most 76802 f64 values total. Use existing linear
fractional delay reads, shared sine phase and explicit right offset. Each channel
reads wet before writing input plus its own feedback; minimum one sample prevents
an instantaneous feedback loop. No cross-channel feedback or hidden mixing.

Complete controls validate atomically, retain histories/phase and change
immediately. Reset clears both histories and left phase. Dry mode still advances
wet feedback/LFO, so later mix edits expose retained history. Exact mix endpoints
avoid multiplying inactive paths. Zero LFO rate freezes current phase. Exact
channel lengths preflight before all writes/history; empty channels do nothing.

Depth scales the center delay; this is a linear interpolation modulation profile
without bandlimited resampling, automatic latency/tail reporting, smoothing,
nonfinite/denormal/headroom policy or callback deadline support. No dependency
added. Consumers own effect routing and lifecycle.

Tests compare independent indexed full-history recurrence, static fractional
impulses/positive-negative feedback, stereo-phase landmarks, dry history,
partition/reset/frozen phase, invalid construction/edit/shape preservation and
maximum bounded storage with 10000 heap-instrumented prepared workloads.
