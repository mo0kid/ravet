# 0139: Prepared fourth-order Linkwitz–Riley crossover

Status: portable one-channel f64 electrical profile implemented and offline tested.

Reuse four validated existing biquads: each branch cascades two identical
second-order Butterworth sections at Q=1/sqrt(2). A fourth-order Butterworth
filter uses different pole-pair Q values and cannot substitute for LR4. Prepare
both branches atomically off the processing path. Keep coefficient fields private
and reject unrepresentable strict stability margins through existing constructors.

Use fixed storage, per-sample two-branch output and exact-length caller buffers.
Shape validation precedes all writes/history mutation. Empty blocks do nothing.
Replacement retains all states; reset clears them. Consumer smoothing, channels,
headroom, denormals and callback scheduling remain explicit. No dependency added.

At cutoff each branch is -6 dB; summed branches have unity magnitude and the
phase response of a second-order all-pass, not time-domain input identity. Tests
compare independent bilinear amplitudes, all-pass impulse/energy, sinusoidal
power, partition/reset/replacement histories, malformed designs/shapes and heap
instrumentation. These are electrical/offline evidence, not acoustic alignment
or native host/device/deadline support. Other orders and automatic routing are
separate work.
