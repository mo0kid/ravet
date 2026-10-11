# Native DSP boundary

`ravet-native-dsp` is an optional workspace crate for explicitly selected native
numerical behaviour. It provides two independent capabilities: scoped
current-thread floating-point policy, and explicit packed values with prepared
vector slice operations. The core is `no_std`, needs no allocator and has empty
default features. Only its private native modules may use `unsafe`; the crate
otherwise denies it, and the portable crates keep the workspace unsafe-forbid.
The umbrella `ravet` crate does not re-export it. Decisions:
[0266](decisions/0266-scoped-native-floating-control.md) and
[0267](decisions/0267-explicit-packed-register-and-slice-family.md).

## Explicit native floating-point policy

`ravet-native-dsp` is a separate optional numerical boundary. Portable DSP defaults
remain unchanged. Its core is `no_std` and needs no allocator; the optional
`processor` feature depends on the portable Foundation contracts. Native register
access is confined to a private audited module. There is no implicit per-filter
policy, global thread configuration, hidden state snapping or device selection.

Choose `DenormalMode::Preserve` to retain the actual current controls, or
`FlushBoth` to request f32/f64 operand and result flushing after actual capability
admission. `ScopedDenormalMode::enter` returns a non-Copy, non-Clone, non-Send,
non-Sync guard bound to its creating thread. Drop restores the exact prior
register control **and sticky status**, including normal return, early return,
unwind and nested scopes. Thus even Preserve isolates scope-created sticky flags;
it does not merge them into the caller's cumulative observations. Forget, abort
or foreign longjmp can prevent Drop. Other code executed on this same thread
inside the scope observes that environment.

The supported register families are ordinary OS user-mode AArch64 FPCR/FPSR and
x86_64 SSE2 MXCSR. Architecture selection alone does not prove privileged,
bare-metal or sandbox register access. The x86 scope neither controls nor
restores x87 state. It inspects the actual MXCSR_MASK using a pre-zeroed aligned
512-byte FXSAVE area; a zero hardware mask uses the architectural 0xffbf fallback,
which lacks DAZ. Missing DAZ/FTZ support or any unmasked SIMD exception rejects
FlushBoth before changing controls. Entry performs one capability save, or two
when a changed control is read back; this is bounded work, not a deadline claim.
On AArch64, FZ is admitted only with alternate handling AH=0 and all declared
floating exception trap-enable bits clear. It changes neither rounding, default
NaN, FZ16 nor other controls. A rejected control read-back restores the original
control/status before returning its original error. Unsupported architectures
return an error rather than a successful no-op called flushing.

`current_float_state` exposes read-only actual backend/control/status, the x86
mask and standard-profile flush observations. It is not a register setter.
AArch64 AH=1 is not interpreted as this standard profile. x86 result flushing
also requires masked underflow. The crate makes no universal claim that an
optimized Rust expression or external math function executes precisely the
source arithmetic: constants, identity rewrites, library/compiler floating
assumptions and fused instructions remain separate issues. Runtime instructions
and emitted code are qualified explicitly. NaN/Inf classification is tested;
no cross-platform NaN payload or universal bitwise scalar parity is promised.

With `processor`, `DenormalProcessor<P>` owns an existing portable Processor and
an immutable selected mode. Constructor refusal returns the exact uncalled P and
original native error. Its constructor probes only the constructing thread.
Borrowed `prepare` reports the child's actual PrepareError and retains adapter
ownership on failure; a CONTROL-thread probe cannot qualify a future AUDIO
thread. Each process call validates the entire configuration, block, buses and
events before entering the actual thread scope or calling P. Native refusal
silences/discards all active outputs/events without consuming child history,
retains the first original native error, and disables retries until explicit
reset/reprepare. Child faults or panics may have committed histories; no rollback
is inferred. Child faults silence outputs, and an unwound process stays disabled
until explicit reset/reprepare. Scope Drop still restores the registers.

Manual `start_audio`/`stop_audio` hooks must be paired on the AUDIO owner before
CONTROL deactivation or transfer. Graph's explicit session guard supplies that
pairing; consumers using manual hooks must do it themselves. No adapter Drop
implicitly stops/deactivates P. `into_inner` returns the original P only when
stopped/deactivated; refusal retains the complete adapter without allocating.
A child lifecycle panic remains uncertain until explicit successful cleanup;
stop alone cannot prove a panicked prepare left P inactive. Reset is the child's
existing contract, not a guarantee that any external native backend can recover.

The headless `scoped-policy` example takes exactly `preserve` or `flush-both`,
observes runtime operand flushing and prints prior/during/restored state. Tests
qualify signed f32/f64 subnormal operands/results, normal and special classes,
actual sticky/unrelated control restoration, thread-local/nested/unwind ownership,
synthetic mask/AH/trap/read-back refusals, late-event preflight, failure-owned
constructor/prepare/extraction and foreign child faults. Actual Foundation →
Graph → scoped native policy → native32/native64 TPT IIR/FIR compositions render
separate and in-place buffers, variable/zero blocks and two explicit session
cycles, with independent transfer-equation/convolution checks and no callback
heap activity. This does not imply physical audio, a host or deadline support.

The qualification host executes AArch64 natively and separately executes thin
x86_64 Mach-O tests through Rosetta. The latter proves this translated register
profile, not physical Intel/AMD hardware. Both release assembly profiles contain
the declared register controls and runtime floating multiply instructions. The
scope provides no automatic vectorization; the explicit packed values and prepared
vector operations below are a separate API that never changes this policy.

## Explicit packed values and prepared vector operations

`F32x4` and `F64x2` provide fixed packed lanes with immutable explicit
`VectorBackend::{Scalar,Aarch64Neon,X86Sse2}` selection. `VectorBackend::native()`
is a compile-time admission: `aarch64` with `target_feature = "neon"` or `x86_64`
with `target_feature = "sse2"`, the ordinary OS user-mode baseline. Any other
target, including one built with those features disabled, returns
`UnsupportedBackend` rather than silently substituting Scalar. It is not a runtime
CPU probe and does not establish privileged, bare-metal or sandbox register access.
The Scalar reference expresses scalar Rust arithmetic and uses the exact existing
`libm=0.2.16` dependency for no_std square root. Compiler auto-vectorization may
optimize that reference; Scalar is not a promise to disable SIMD instructions.
The native paths deliberately use audited packed intrinsics. Fixed private lane
arrays support safe storage and transient register arithmetic, with no public raw
pointers, alignment requirement, SIMD ABI or permanent register residency claim.

Construct from arrays, broadcast, or load/store checked complete prefixes at any
slice offset. Lane reads/replacement and arbitrary permutations validate all
indices. `LaneMask4`/`LaneMask2` reject unavailable high bits, supply bit operations
and checked lane tests, and support exact-bit selection. Binary values must have
the same selected backend; a mismatch refuses before an output edit. Arithmetic
covers add/subtract/multiply/divide, explicit unfused multiply-then-add,
square root and fixed sum/dot. Absolute/negate are bit operations preserving NaN
payloads without quieting them. Interleave/deinterleave provide an exact
full-width pair. Arbitrary checked permutation is defined by lane copies; it does
not imply a dedicated native gather instruction for every runtime index pattern.

Comparisons are ordered except `not_equal`, where an unordered NaN lane is true.
Minimum/maximum select the left bits only for ordered less/greater; otherwise
right bits are preserved, including ties, signed zero and NaN. Mask selection
copies the chosen bits. An integer-only, non-inlined selection boundary prevents
optimized scalar minimum/maximum instructions from rewriting selected subnormal
representations under DAZ. Comparisons still observe the actual CPU input-flush
policy; preserving selected bits does not disable that policy. An independent
integer-encoding test oracle separates those two obligations. Arithmetic/sqrt
have actual backend floating semantics,
including current rounding/denormal/NaN controls and observable overflow; there
is no payload parity guarantee for arithmetic NaNs, fused repair or hidden
control guard. The crate does not turn every optimized Rust expression into a
specified instruction sequence. Call an explicit denormal scope separately if
that policy is desired. Scalar libm and native hardware are separately named
implementations; numerical tests qualify the declared selected profiles.

Packed f32 sums use `(lane0+lane1)+(lane2+lane3)` in f32; f64 uses `lane0+lane1`.
Dots multiply first, then use the same sum. These operations do not widen,
compensate or fuse products into sums. The reduction tree is part of the API.

`PreparedVectorOps32`/`PreparedVectorOps64` fix a selected backend and maximum
1..1,048,576 samples without allocating. Their slice family provides
fill/clear/copy, elementwise add/subtract/multiply/divide/minimum/maximum, finite
scalar gain/offset, absolute/negate/sqrt, ordered clip with finite endpoints,
unfused add-product, sum/dot/sequential extrema, scaled i32 conversion and checked
stereo interleave/deinterleave. Fill and source samples may contain nonfinite
values; control gains/scales/offsets must be finite. Clip preserves input NaN bits.
Its `low > high` range check is a floating comparison, so it observes the live
input-flush policy: inside a `FlushBoth` scope subnormal endpoints compare as signed
zeros, and for example `low = +min-subnormal, high = -min-subnormal` is admitted
although its bit order is inverted. Selection then follows the same flushed
comparisons while copying the original endpoint/input bits.
All requested prefixes/control values and checked 2N lengths are admitted before
any edit. Backing suffixes remain untouched; safe distinct mutable borrows define
the alias contract. There is no implicit aligned allocation or callback heap.

Native chunks use the full fixed width. Remaining arithmetic elements execute
scalar operations; native sqrt tails use actual scalar hardware sqrt, while
Scalar tails use libm. No incomplete chunk overreads or unused-lane division/sqrt
exceptions are introduced. Slice sums/dots left-fold packed pairwise chunk totals,
then left-fold scalar tail values, starting at +0. Thus they have an explicit
width-dependent tree and can differ from another partition/tree. Slice extrema
fold in source order using the same right-preference rule and return None on an
empty prefix. Raw i32 conversion is exact in f64 and rounded in f32; scaling
follows afterward in sample precision without saturating or concealing overflow.

Separate/in-place gain and in-place addition are available. `gain_planar` and
`gain_planar_in_place` admit 1..64 matching channels/gains and every channel prefix
before editing any channel. A channel-count or gain/output-count mismatch
(including 0 or 65 channels), a late NaN/±infinite gain, a 64th-channel short
prefix or a frame count above the maximum leaves the whole bank unchanged. Invalid
counts and gains are refused at zero frames too; a short prefix cannot exist at
zero frames, so such a call is admitted without edits. Requested prefixes
alone are bounded; an unused longer backing slice is not rejected. Stereo N
means per-channel frames and permits an independently checked 2N destination.

Qualification covers each lane/operation/backend, mask domains, checked
permutations, exact-length and offset load/store, all prefix lengths 0..17, tails,
suffix preservation, heap-free callback work for every slice method and an actual
1,048,576-sample prefix. Every ordered pair of 17 special encodings (signed zeros,
subnormals, qNaN with distinct payloads, sNaN, infinities and finite extrema) is
placed in every packed lane and every slice chunk/tail position; comparisons,
minimum/maximum, selection, extrema and bit operations are checked against an
integer-encoding oracle inside both explicit Preserve and FlushBoth scopes. Rounding
slice operations, conversion and sqrt are bit-exact against independently written
unfused scalar sequences under the host's Preserve profile, which the tests observe
to have no flush control set; Preserve itself retains, rather than clears, the
ambient control. Packed arithmetic has direct result-flush and input-flush witnesses
on both backends; other slice arithmetic and conversion inside FlushBoth are not
separately qualified. Refusal tests truncate every slice parameter of every method,
exceed the maximum, shorten each 2N buffer, and cover channel-count mismatches and
nonfinite controls with all buffers unchanged. A separate Decimal120 non-negative
dyadic sqrt corpus of 36 inputs (including 0 and extreme normal/subnormal values)
is placed in distinct rotated register lanes and tail positions. Native hardware
results must match it exactly; Scalar libm is held to a 1-ULP bound, and the
recorded ARM and translated x86 runs observed 0 ULP. This finite evidence is not a
universal correctly-rounded claim. Subnormal sqrt inside FlushBoth stays
policy-defined and unasserted: with libm's default `arch` feature disabled, as the
workspace configures it, Scalar sqrt of the smallest positive subnormal was observed
to return the input bits on both runs, while native hardware flushes. The real Foundation → Graph consumer processes three
independent packed gain/mix channels followed by existing FIR in both precisions,
external separate/in-place buffers, variable/zero blocks and two lifecycle cycles.
The mixer stage alone, including a variant using `gain_planar_in_place` and
`add_in_place`, is compared exactly (numerical equality) with the independent
rounded scalar schedule; only the FIR stage keeps a tolerance. No callback heap is used.

Native AArch64 and separately Rosetta-translated x86_64 release tests and packed
instruction output are recorded separately. Translation is not physical Intel/AMD
support. The headless `vector-ops` example explicitly selects each backend and
reports numerical checks plus repeated min/median/max timings for the selected
65,536-sample gain/add-product profile. Those timings establish neither universal
speedup nor a deadline; compiler optimization, machine load and instruction
selection remain relevant. These reusable primitives do not claim whole FFT/FIR
kernels have been vectorized, or imply host/device support, AVX/AVX512, half/SVE,
integer-register families, or automatic runtime vector-width dispatch.
