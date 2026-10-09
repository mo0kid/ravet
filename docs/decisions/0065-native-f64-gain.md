# 0065: Native f64 gain with shared control policy

## Context

Portable f64 buffers/graphs cannot establish native gain precision when the
reference processor and its exported ABI use f32 samples. Native f64 metering
also needs to avoid overflow when squaring large finite values.

## Decision

Add a separate GainProcessor64 that reuses existing gain parameter/state/smoother
control policy while copying/multiplying native f64 audio. Preserve existing f32
public behavior. Per-frame f32 gain promotes exactly; parameter resolution stays
explicitly f32. Native f64 meters use scaled sum-of-squares and separate reading
snapshots; optional f32 telemetry saturates finite out-of-range readings.

## Consequences and validation

This is portable mono/stereo processing, not native format support. Exporters
need their own f64 ABI and host mapping evidence. Meter/mailbox UI remains the
existing f32 path. Audio nonfinite values propagate; meter diagnostics do not
sanitize the samples. State/IDs remain shared and version-compatible.

Acceptance covers retained sub-f32 samples, gain/bypass event timing, complete
state/reset, preflight rejection preservation, zero-frame behavior, f64 extrema/
subnormals/nonfinite readings and prepared heap workloads. Native deadlines are
not inferred from offline tests.
