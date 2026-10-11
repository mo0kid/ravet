# 0267: Explicit packed numerical values and prepared slices

Status: accepted; implemented in `ravet-native-dsp` and qualified within the
declared native profiles (native AArch64, Rosetta-translated x86_64), including
the revision that followed an independent static review.

Add the full float packed-register/value and bounded prepared-slice family to
`ravet-native-dsp`, keeping portable DSP defaults and unsafe-forbid unchanged.
Explicit Scalar, AArch64 NEON and x86_64 SSE2 choices have fixed widths f32x4/f64x2.
Native selection is a compile-time admission of `aarch64`+`neon` or `x86_64`+`sse2`
on an ordinary OS user-mode target; any other target is refused with
`UnsupportedBackend` and never silently falls back. The private intrinsic module holds
all complete local-array pointer/alignment/extent/feature invariants. Public API
values use fixed safe storage, not promised register residency or a SIMD ABI.

All matching-backend, lane/mask/index, prefix/maximum/control/range/2N admission
occurs before output mutation. Native operations never overread a partial chunk;
scalar tails are explicit, including hardware scalar sqrt for Native and libm
sqrt for Scalar. Scalar reference optimization is allowed and is not an instruction
disabling mode. The direct exact `libm=0.2.16` dependency reuses the existing MIT
math backend for no_std scalar sqrt; no new resolved third-party package is needed.

Special-value semantics are public: ordered comparison except unordered !=;
minimum/maximum choose left only for ordered less/greater and otherwise preserve
right bits. Scalar fallback, tails, clipping and sequential extrema select through
one non-inlined integer-only boundary: translating a float selection into x86
MIN/MAX can destroy the original subnormal representation under DAZ. Qualification
preserves the initially failing oracle and actual tail cases; the corrected
independent integer-bit oracle models input flushing only during ordering, then
selects untouched encodings. Corrected emitted code has integer conditional moves
rather than floating MIN/MAX for selection. Abs/negate/mask selection retain
payload bits. Arithmetic follows
executed native/scalar math, with no implicit CPU-policy guard or universal NaN
payload/rounding parity. Products then sums are unfused; reduction order is fixed
and width-dependent. Sequential extrema are distinct from pairwise sum trees.
Clip preserves NaNs and controls must be finite; its endpoint-order check is a
floating comparison and therefore observes active input flushing. No overflow
saturation is hidden.

The complete slice family and 1..64-channel gain admission need no heap, and
late-channel rejection is atomic. Packed fixed interleave/deinterleave use the
actual native instructions; arbitrary validated permutation uses safe lane copies
without claiming a native dynamic-gather instruction for every pattern.

Primary intrinsic/feature facts were reviewed from
[Rust core::arch](https://doc.rust-lang.org/core/arch/index.html),
[the official SSE2 interface](https://doc.rust-lang.org/core/arch/x86_64/index.html)
and [Arm ZIP1 interface](https://doc.rust-lang.org/core/arch/aarch64/fn.vzip1q_f32.html),
with instruction semantics from the Intel/Arm primary manuals cited by the scoped
floating-control decision. No vendor implementation is copied.

Both widths/backends, all operation/mask/special-value policies (all ordered
special-encoding pairs in every lane and tail position, under both explicit scopes),
bounded prefixes, scalar tails, per-parameter prefix, maximum, 2N, channel-count and
control refusals, and actual maximum/bank capacities are qualified, plus an
independent Decimal120 root corpus (native exact, Scalar within 1 ULP) and real
Foundation/Graph gain/mix→FIR composition with an exactly checked mixer stage.
Slice arithmetic other than the packed flush witnesses is bit-qualified only under
the host's no-flush Preserve profile. Actual packed native output and separately translated x86 execution
support this explicit API; timing or compiler auto-vectorization alone does not.
No universal speedup/deadline, physical x86 support, hardware audio/host support or
whole-kernel FFT/FIR vectorization is inferred.
