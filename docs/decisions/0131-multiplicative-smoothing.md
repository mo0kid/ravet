# 0131: Multiplicative parameter smoothing

Status: portable DSP implemented; offline evidence only.

Add strictly positive finite f32 geometric smoothing alongside existing signed
linear smoothing. Reuse explicit u32 sample-count duration/time conversion.
Changed target starts from the current effective returned f32 value; identical
target preserves progress. First sample advances one of N steps, last snaps
exactly to target, zero duration is immediate, reset snaps while keeping duration.
Zero/negative endpoints receive NonPositiveValue; nonfinite receives the existing
NonFiniteValue. Invalid updates leave complete state untouched.

Design ratio through the difference of f64 endpoint logs, avoiding f32 division
overflow across positive subnormal-to-maximum range. Retarget computes log/exp;
each sample advances f64 progress by multiplication and clips any accumulated
overshoot against target, then returns f32. Direction follows target/current,
not rounded ratio: a very long decreasing ramp can round its ratio to unity and
must remain at its current value until representable progress/final snap, not
jump immediately to target. Returned f32 rounding can repeat samples.

No allocation, implicit physical-unit interpretation, signed/zero crossing,
duration smoothing or native deadline claim is added. Tests cover geometric
landmarks and independent log interpolation, positive extremes/monotonicity,
rounded-unity direction, repeated target, retarget/reset/partition and invalid
updates preserving progress, plus prepared heap instrumentation.
