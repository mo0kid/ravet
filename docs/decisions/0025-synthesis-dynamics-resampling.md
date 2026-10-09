# 0025 — Explicit numerical synthesis, dynamics and rate conversion

Status: accepted, 9 October 2026.

Keep phase oscillators, peak dynamics and rate conversion in the portable signal
crate. Expose quality, time constants, sidechain and streaming backpressure rather
than hiding allocation, resampling or linking behind processor lifecycle.

Provide polyBLEP saw/square, trig sine and explicitly naive triangle. Provide a
soft-knee peak compressor with no lookahead/brickwall promise. Provide prepared
causal sinc conversion with a fixed normalized phase profile and caller-owned
input/output prefixes. Preparation alone allocates; callbacks/reset do not.

Independent phase, exponential/curve, partition, DC and band fixtures establish
behavior within documented limits. They do not establish deadline performance,
alias-free oscillation, production limiter behavior or broad resampler rejection.
See [the signal guide](../signal-processing.md) for exact contracts.
