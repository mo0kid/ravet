# 0150: Prepared feedforward and feedback comb filters

Status: implemented portable DSP; validation is recorded in work status.

Expose a standalone one-channel f64 comb primitive alongside prepared delay
storage. Existing reverb combs are private, damped and return the delayed branch;
they do not supply the direct-input standalone filter contract. Keep their
tested network behavior independent. The new `CombKind` selects input-history
feedforward `y[n]=x[n]+g*x[n-D]` or output-history feedback
`y[n]=x[n]+g*y[n-D]`, both with unity direct input.

Fix D at preparation to 1..1048576 whole samples, bounding one allocation to
8 MiB excluding overhead. A positive delay avoids an instantaneous feedback
equation. Validate all arguments before allocation. Feedforward permits every
finite coefficient, including unity-magnitude notches; feedback requires finite
absolute magnitude strictly below one. Fixed-coefficient stability does not
guarantee numerical headroom or a callback deadline.

Reuse the delay cursor's oldest slot: read before writing input or output, then
advance exactly once. The storage length is D, so delay one and non-power-of-two
wraps remain exact without an interpolation guard. Coefficient replacement
validates before mutation and retains history; no smoothing or hidden gain
normalization. Zero coefficient returns input exactly while advancing history.
Reset clears history/cursor while retaining configuration and allocation.
Kind/delay changes require new preparation. No damping/fractional modulation,
nonfinite sanitization, denormal policy or automatic latency/tail declaration.

Tests compare exact signed impulse trains, independent indexed input/output
recurrences with live coefficients and analytic complex steady-state gains.
Partition/reset/empty/zero-coefficient continuity, invalid preparation/edit
preservation, exact maximum delay/allocation bounds and 10000 prepared
process/edit/reset heap measurements establish portable offline behavior.
