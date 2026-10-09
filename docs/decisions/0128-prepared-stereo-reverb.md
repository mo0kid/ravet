# 0128: Prepared synthetic stereo reverb

Status: implemented portable DSP; offline evidence only.

Extend the existing bounded mono Schroeder network using an internal validated
fixed delay offset rather than duplicate comb/allpass mechanics. The left network
keeps its delays; right delays add sample-rounded 0..10 ms spread. Each comb
feedback is redesigned for its actual length, preserving nominal DC decay policy.
Public mono construction/output remain unchanged. Two networks receive common
mono excitation L/2+R/2. Width interpolates wet branches from their shared average
to separate output. Dry audio remains stereo. Zero spread intentionally produces
identical wet output; antisymmetric input produces no wet excitation. This is a
synthetic effect rather than a physical room or guaranteed decorrelation model.

Stereo configuration retains mono bounds, adds spread and width validation, and
caps twelve delay buffers at 133094 f64 samples. Mix replacement validates the
complete pair before changing either value and preserves histories; reset keeps
configuration. Processing advances both wet branches while dry. Whole-frame APIs
avoid partial stereo advancement. No smoothing, freeze/modulation, denormal,
headroom/sanitization or native deadline guarantee is imposed.

Tests compare zero spread against the independently tested mono engine, exact
first impulse timings and width equations, partition/reset, finite decaying tail,
dry stereo/cancellation and recovery. Invalid complete mix keeps histories, the
maximum storage bound is checked, and 10000 frames/reset/mix measure no heap
activity. These tests establish portable behavior, not DAW/device support.
