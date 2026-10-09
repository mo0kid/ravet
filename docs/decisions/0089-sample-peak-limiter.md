# 0089: Prepared sample-peak limiter

Status: implemented; portable numerical/heap evidence.

## Decision and scope

Use existing rectified peak detection with instantaneous attack and configurable
exponential release. Apply unity-or-ceiling/level gain and a final symmetric hard
clamp to protect the amplitude ceiling from rounding. Configuration prepares a
-120..24 dBFS ceiling and validated rate/release; one f64 channel owns history.
Zero release is memoryless. Expose current gain and detector scalars; reset
clears history and restores unity gain, retaining the ceiling.

No lookahead/true-peak/oversampling, channel linking or separate gain smoothing
is implied. Clipping may distort. Nonfinite input is not validated: NaN propagates,
infinity passes the clamp, and either can poison detector history until reset.
No device/host/deadline claim follows from offline samples.

## Evidence

Three groups exercise independent memoryless/reference gains, extremes/both
polarities/pass-through, release recurrence and instantaneous retrigger,
partitioning/reset, invalid configuration/nonfinite recovery and 100000 samples
with no counted allocations. True peaks and spectral distortion are not covered.
