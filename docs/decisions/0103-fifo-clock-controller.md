# 0103: Bounded FIFO-depth rate correction

Status: implemented fixed-state proportional controller; device integration
remains separate.

## Decision

Use caller-supplied queued+prefetched input occupancy and elapsed output-clock
time. Exponentially smooth depth, convert error to ppm, clamp the command and
limit correction slew. Positive error drains input faster through a prepared
variable resampler. No integral state, automatic clock read, allocation, device
fallback or ring ownership. A constant clock drift produces a proportional
steady occupancy offset; prepared capacity/target leave caller-chosen headroom.
Reset restores target/zero correction. Validate bounds/observations before any
state mutation; the caller must match controller/resampler maximum corrections.

Bounded commands/slew are not universal stability guarantees. Excess drift,
callback jitter, dropped samples, inadequate headroom and extreme valid settings
can still underflow/overflow. Native timing/physical clock evidence is separate.

## Evidence

Three groups cover independent 30000-callback queue simulations for both drift
signs with variable output block sizes/bounded arrival jitter, expected equilibrium
and correction bounds; independent smoothing/slew/saturation/reset landmarks;
prepared resampler coupling/heap-free loops; constructor and atomic observation
rejection. No native device support follows from these synthetic clocks.
