# 0102: Prepared bounded live sinc rate adjustment

Status: implemented one-channel f64 variable-rate profile; clock integration
remains separate.

## Decision

Reuse the causal sinc stream/history/table but prepare a symmetric finite ppm
range up to one percent. Bound all permitted input/output steps within 1/16..16
and build the fixed kernel at the largest permitted step. This retains a
conservative cutoff during live adjustments without callback allocation/table
replacement. The original fixed constructor/cutoff arithmetic remains unchanged.

A cheap validated setter changes subsequent output spacing without phase/history
reset. Preserve the next already scheduled output position; new spacing follows
that output. Positive ppm consumes input faster. Invalid values leave complete
state unchanged. Reset clears history but retains the adjustment; zero restores
nominal explicitly. Caller owns scheduling/smoothing/clock/FIFO estimation.
No automatic device conversion or physical clock/spectral/deadline claim.

## Evidence

Three groups verify piecewise-rate partition identity/DC/continuity, independent
100000-input sample counts at positive/negative extremes and small drift,
reset/fresh-history identity, prepared bounds/atomic rejection/pending position
and heap-free changes/process/reset. Existing fixed-rate resampler regressions
pass. These finite synthetic clocks are not native independent-clock evidence.
