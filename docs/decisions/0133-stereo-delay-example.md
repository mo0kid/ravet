# 0133: Stereo delay teaching example

## Decision

Replace the gain processor in the interactive effect tab and its explicit WAV
fixture with a portable delay composed from `ravet-signal::DelayLine` and
`LinearSmoother`. Keep it in the teaching library; existing gain plug-in exports
remain separate. Reuse the Amber theme, rotary controls, metering, input routing
and default standalone Audio/MIDI settings rather than adding device ownership
inside DSP. No delay plug-in wrapper is added by this change.

Prepare two f64 delay lines for up to two seconds at 8..192 kHz. Mono/stereo f32
processing permits up to 1024 frames and 128 ordered events, preflighting known
parameter ranges before any mutation. Time uses a 20 ms linear glide, accepting
the resulting pitch motion; feedback/mix/ping-pong routing use 5 ms ramps.
Feedback is bounded to 0..0.95. Cross-feedback interpolates between same-channel
and opposite-channel repeats. Dry bypass preserves internal history; reset
clears it without allocating and retains requested controls. Tail reporting is
conservatively infinite. Nonfinite input samples become zero. No limiter or
tempo synchronization is implied.

Use staggered, tapered demo bursts to expose the repeats without an input
device. The offline fixture instead uses isolated stereo impulses and exact
feedback/bypass events. The existing fixed command queue carries complete
validated delay controls; UI and native device operations remain outside audio.

Standalone editor containers follow content-view resizing. Fixture size checks
must verify the actual browser viewport before checking document overflow;
otherwise a fixed-size embedded page can silently invalidate minimum-size claims.

The examples opt into an initial 900:780 content aspect-ratio lock. Scale the
complete fixed design canvas uniformly on browser resize, rather than growing
panel widths while preserving vertical sizes. Text, controls and spacing share
one uncapped scale factor. Keep this an opt-in standalone-window capability so
other consumers can use responsive layouts. Native fixtures test both smaller
and larger sizes and constrain mismatched width/height requests, measuring all
visible component dimensions and positions against their baseline.

## Status and limits

Portable delay, live tab and offline fixture are implemented. Validation checks
stereo/ping-pong impulse timing and decay, block-partition independence,
transactional rejection, bypass tail retention, mono/max delay, heap behavior
and native controls/layout. Native UI tests open no actual devices and do not
establish device/DAW compatibility or a real-time deadline.
See [examples](../examples.md) and [work status](../WORK_STATUS.md).
