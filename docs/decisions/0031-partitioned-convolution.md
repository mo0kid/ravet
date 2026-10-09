# 0031 — Uniform partitioned convolution

Status: implemented within the documented single-channel numerical profile.

Direct FIR cost grows per sample with response length. Add a separate prepared
uniform overlap-add engine rather than change the zero-latency direct API.
Responses are partitioned into B-sample segments, transformed at 2B, and summed
against a ring of input spectra. The inverse first half adds the prior second
half; a B-sample output queue establishes exact B-sample algorithmic latency.
Caller blocks do not determine transform boundaries.

Finite responses have at most 262,144 taps and B is a power of two in 16..=4,096.
Prepared storage and reset are explicit; no live response swap, crossfade,
background preparation, SIMD, nonuniform scheduling or deadline is promised.
Audio NaNs propagate. All channels and latency compensation belong to callers.
Tail length includes B samples plus response length minus one, and flushing
requires explicit zeros. Tests compare delayed direct sums at several response
and partition sizes, verify the maximum response endpoints and measure prepared
processing/reset allocation behavior.
