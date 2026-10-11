//! W254 r2 review follow-up (G1, G2, G10, G11, G14, G15, O1): exhaustive
//! special-value pairs at every lane/tail position under both explicit scopes,
//! judged by an integer-encoding oracle that contains no floating selection.
use ravet_native_dsp::{
    DenormalMode as Mode, F32x4, F64x2, LaneMask2, LaneMask4, PreparedVectorOps32,
    PreparedVectorOps64, ScopedDenormalMode, VectorBackend as Backend, VectorError as Error,
    current_float_state,
};
use std::hint::black_box;

fn backends() -> [Backend; 2] {
    [Backend::Scalar, Backend::native().unwrap()]
}

/// Integer-only IEEE ordering. NaN is unordered. Comparison inputs are
/// normalized to signed zero only when the actual input-flush profile is active.
fn ordered_less(mut a: u64, mut b: u64, width: u32, flush: bool) -> bool {
    let sign = 1u64 << (width - 1);
    let fraction = if width == 32 {
        (1u64 << 23) - 1
    } else {
        (1u64 << 52) - 1
    };
    let exponent = (sign - 1) ^ fraction;
    if [a, b]
        .into_iter()
        .any(|x| x & exponent == exponent && x & fraction != 0)
    {
        return false;
    }
    if flush {
        if a & exponent == 0 {
            a &= sign;
        }
        if b & exponent == 0 {
            b &= sign;
        }
    }
    if a & (sign - 1) == 0 && b & (sign - 1) == 0 {
        return false;
    }
    let mask = if width == 32 {
        u32::MAX as u64
    } else {
        u64::MAX
    };
    let key = |x: u64| if x & sign != 0 { !x & mask } else { x ^ sign };
    key(a) < key(b)
}
fn is_nan(x: u64, width: u32) -> bool {
    let sign = 1u64 << (width - 1);
    let fraction = if width == 32 {
        (1u64 << 23) - 1
    } else {
        (1u64 << 52) - 1
    };
    let exponent = (sign - 1) ^ fraction;
    x & exponent == exponent && x & fraction != 0
}
fn ordered_equal(a: u64, b: u64, width: u32, flush: bool) -> bool {
    !is_nan(a, width)
        && !is_nan(b, width)
        && !ordered_less(a, b, width, flush)
        && !ordered_less(b, a, width, flush)
}

/// The admission tightening is a no-op on the ordinary macOS targets: both
/// default target-feature sets contain the gated feature.
#[test]
fn native_admission_matches_ordinary_target_features() {
    #[cfg(target_arch = "aarch64")]
    {
        const { assert!(cfg!(target_feature = "neon")) };
        assert_eq!(Backend::native(), Ok(Backend::Aarch64Neon));
    }
    #[cfg(target_arch = "x86_64")]
    {
        const { assert!(cfg!(target_feature = "sse2")) };
        assert_eq!(Backend::native(), Ok(Backend::X86Sse2));
    }
    assert_eq!(Backend::Scalar.validate(), Ok(()));
    assert_eq!(Backend::native().unwrap().validate(), Ok(()));
}

#[test]
fn every_constructor_refuses_the_foreign_backend() {
    let other = if cfg!(target_arch = "aarch64") {
        Backend::X86Sse2
    } else {
        Backend::Aarch64Neon
    };
    assert_eq!(other.validate(), Err(Error::UnsupportedBackend));
    assert!(matches!(
        F32x4::new(other, [1.; 4]),
        Err(Error::UnsupportedBackend)
    ));
    assert!(matches!(
        F32x4::broadcast(other, 1.),
        Err(Error::UnsupportedBackend)
    ));
    assert!(matches!(
        F32x4::load(other, &[1.; 4]),
        Err(Error::UnsupportedBackend)
    ));
    assert!(matches!(
        F32x4::from_i32(other, [1; 4]),
        Err(Error::UnsupportedBackend)
    ));
    assert!(matches!(
        F64x2::new(other, [1.; 2]),
        Err(Error::UnsupportedBackend)
    ));
    assert!(matches!(
        F64x2::broadcast(other, 1.),
        Err(Error::UnsupportedBackend)
    ));
    assert!(matches!(
        F64x2::load(other, &[1.; 2]),
        Err(Error::UnsupportedBackend)
    ));
    assert!(matches!(
        F64x2::from_i32(other, [1; 2]),
        Err(Error::UnsupportedBackend)
    ));
    assert!(matches!(
        PreparedVectorOps32::new(other, 17),
        Err(Error::UnsupportedBackend)
    ));
    assert!(matches!(
        PreparedVectorOps64::new(other, 17),
        Err(Error::UnsupportedBackend)
    ));
    // Pinned precedence: the maximum is checked before backend admission.
    for maximum in [0, 1_048_577] {
        assert!(matches!(
            PreparedVectorOps32::new(other, maximum),
            Err(Error::InvalidMaximum)
        ));
        assert!(matches!(
            PreparedVectorOps64::new(other, maximum),
            Err(Error::InvalidMaximum)
        ));
    }
}

macro_rules! review {
    ($packed:ident,$slice:ident,$permute:ident,$p:ident,$ops:ident,$s:ty,$bits:ty,$width:expr,$mask:ident,$qnan:expr,$qnan2:expr,$snan:expr,$sign:expr) => {
        fn patterns() -> Vec<$s> {
            [
                0,
                $sign,
                1,
                1 | $sign,
                2,
                <$s>::MIN_POSITIVE.to_bits(),
                $qnan,
                $qnan | $sign,
                $qnan2,
                $snan,
                $snan | $sign,
                <$s>::INFINITY.to_bits(),
                <$s>::NEG_INFINITY.to_bits(),
                <$s>::MAX.to_bits(),
                (-<$s>::MAX).to_bits(),
                (1. as $s).to_bits(),
                (-1. as $s).to_bits(),
            ]
            .into_iter()
            .map(<$s>::from_bits)
            .collect()
        }
        const W: u32 = <$bits>::BITS;
        fn lt(a: $s, b: $s, flush: bool) -> bool {
            ordered_less(a.to_bits() as u64, b.to_bits() as u64, W, flush)
        }
        fn eq(a: $s, b: $s, flush: bool) -> bool {
            ordered_equal(a.to_bits() as u64, b.to_bits() as u64, W, flush)
        }
        fn pick(left: bool, a: $s, b: $s) -> $bits {
            if left { a.to_bits() } else { b.to_bits() }
        }
        fn enter(mode: Mode) -> (ScopedDenormalMode, bool) {
            let scope = ScopedDenormalMode::enter(mode).unwrap();
            let flush = current_float_state().unwrap().input_flush();
            // Host default thread state has no DAZ; FlushBoth must be observed.
            assert_eq!(
                flush,
                mode == Mode::FlushBoth,
                "{mode:?} input-flush profile"
            );
            (scope, flush)
        }

        /// G1/G10: every ordered pair of 17 encodings (ties, ±0, subnormals,
        /// qNaN/sNaN with distinct payloads, ±inf) in every lane, under both
        /// explicit scopes, against the integer oracle.
        #[test]
        fn $packed() {
            let p = black_box(patterns());
            let l = p.len();
            for mode in [Mode::Preserve, Mode::FlushBoth] {
                let (_scope, flush) = enter(mode);
                for backend in backends() {
                    for xi in 0..l {
                        for yi in 0..l {
                            let a: [$s; $width] = core::array::from_fn(|j| p[(xi + j) % l]);
                            let b: [$s; $width] = core::array::from_fn(|j| p[(yi + 2 * j) % l]);
                            let (lhs, rhs) = (
                                $p::new(backend, black_box(a)).unwrap(),
                                $p::new(backend, black_box(b)).unwrap(),
                            );
                            let (less, less_equal, greater, greater_equal, equal, not_equal) = (
                                lhs.less(rhs).unwrap(),
                                lhs.less_equal(rhs).unwrap(),
                                lhs.greater(rhs).unwrap(),
                                lhs.greater_equal(rhs).unwrap(),
                                lhs.equal(rhs).unwrap(),
                                lhs.not_equal(rhs).unwrap(),
                            );
                            let (minimum, maximum) = (
                                lhs.minimum(rhs).unwrap().to_array(),
                                lhs.maximum(rhs).unwrap().to_array(),
                            );
                            let (absolute, negated, sum) = (
                                lhs.absolute().to_array(),
                                lhs.negate().to_array(),
                                lhs.add(rhs).unwrap().to_array(),
                            );
                            for i in 0..$width {
                                let ctx = format!(
                                    "{mode:?} {backend:?} lane={i} a={:x} b={:x}",
                                    a[i].to_bits(),
                                    b[i].to_bits()
                                );
                                let (l_ab, l_ba, e) = (
                                    lt(a[i], b[i], flush),
                                    lt(b[i], a[i], flush),
                                    eq(a[i], b[i], flush),
                                );
                                assert_eq!(less.lane(i).unwrap(), l_ab, "less {ctx}");
                                assert_eq!(greater.lane(i).unwrap(), l_ba, "greater {ctx}");
                                assert_eq!(equal.lane(i).unwrap(), e, "equal {ctx}");
                                assert_eq!(not_equal.lane(i).unwrap(), !e, "not_equal {ctx}");
                                assert_eq!(
                                    less_equal.lane(i).unwrap(),
                                    l_ab || e,
                                    "less_equal {ctx}"
                                );
                                assert_eq!(
                                    greater_equal.lane(i).unwrap(),
                                    l_ba || e,
                                    "greater_equal {ctx}"
                                );
                                assert_eq!(
                                    minimum[i].to_bits(),
                                    pick(l_ab, a[i], b[i]),
                                    "minimum {ctx}"
                                );
                                assert_eq!(
                                    maximum[i].to_bits(),
                                    pick(l_ba, a[i], b[i]),
                                    "maximum {ctx}"
                                );
                                assert_eq!(
                                    absolute[i].to_bits(),
                                    a[i].to_bits() & (<$bits>::MAX >> 1),
                                    "abs {ctx}"
                                );
                                assert_eq!(
                                    negated[i].to_bits(),
                                    a[i].to_bits() ^ $sign,
                                    "negate {ctx}"
                                );
                                if a[i].is_nan() || b[i].is_nan() {
                                    assert!(sum[i].is_nan(), "NaN class {ctx}");
                                }
                            }
                            for bits in 0..1u8 << $width {
                                let mask = $mask::from_bits(bits).unwrap();
                                let selected = $p::select(mask, lhs, rhs).unwrap().to_array();
                                for i in 0..$width {
                                    assert_eq!(
                                        selected[i].to_bits(),
                                        pick(bits & (1 << i) != 0, a[i], b[i])
                                    );
                                }
                            }
                        }
                    }
                    // G10 direct arithmetic flush witnesses (black-boxed runtime operands).
                    let state = current_float_state().unwrap();
                    let half = $p::broadcast(backend, black_box(0.5)).unwrap();
                    let product = $p::broadcast(backend, black_box(<$s>::MIN_POSITIVE))
                        .unwrap()
                        .multiply(half)
                        .unwrap()
                        .to_array();
                    let zero = $p::broadcast(backend, black_box(0.)).unwrap();
                    let sub = <$s>::from_bits(1);
                    let added = $p::broadcast(backend, black_box(sub))
                        .unwrap()
                        .add(zero)
                        .unwrap()
                        .to_array();
                    if mode == Mode::Preserve {
                        assert!(!state.result_flush());
                        assert!(
                            product
                                .iter()
                                .all(|x| x.to_bits() == (<$s>::MIN_POSITIVE.to_bits() >> 1)),
                            "{backend:?} gradual underflow"
                        );
                        assert!(
                            added.iter().all(|x| x.to_bits() == 1),
                            "{backend:?} subnormal operand"
                        );
                    } else {
                        assert!(state.result_flush(), "FlushBoth result-flush profile");
                        assert!(
                            product.iter().all(|x| x.to_bits() == 0),
                            "{backend:?} result flush {product:?}"
                        );
                        assert!(
                            added.iter().all(|x| x.to_bits() == 0),
                            "{backend:?} input flush {added:?}"
                        );
                    }
                }
            }
        }

        /// G1/G2: slice min/max/extrema/bit operations with the same pair
        /// set (including sNaN) at every chunk and tail position.
        #[test]
        fn $slice() {
            let p = black_box(patterns());
            let l = p.len();
            for mode in [Mode::Preserve, Mode::FlushBoth] {
                let (_scope, flush) = enter(mode);
                for backend in backends() {
                    let ops = $ops::new(backend, 7).unwrap();
                    for offset in 0..=3 {
                        for n in 1..=7 {
                            for xi in 0..l {
                                for yi in 0..l {
                                    let mut a = vec![<$s>::from_bits(0x55); offset + n + 2];
                                    let mut b = a.clone();
                                    for j in 0..n {
                                        a[offset + j] = p[(xi + j) % l];
                                        b[offset + j] = p[(yi + 2 * j) % l];
                                    }
                                    let (a, b) = (&a[offset..], &b[offset..]);
                                    let sentinel = <$s>::from_bits(0x5a);
                                    let fresh = || vec![sentinel; offset + n + 2];
                                    let untouched = |o: &[$s]| {
                                        o[..offset]
                                            .iter()
                                            .chain(&o[offset + n..])
                                            .all(|x| x.to_bits() == sentinel.to_bits())
                                    };
                                    let ctx = format!(
                                        "{mode:?} {backend:?} offset={offset} n={n} xi={xi} yi={yi}"
                                    );
                                    let mut out = fresh();
                                    ops.minimum(a, b, &mut out[offset..], n).unwrap();
                                    for i in 0..n {
                                        assert_eq!(
                                            out[offset + i].to_bits(),
                                            pick(lt(a[i], b[i], flush), a[i], b[i]),
                                            "min i={i} {ctx}"
                                        );
                                    }
                                    assert!(untouched(&out), "{ctx}");
                                    let mut out = fresh();
                                    ops.maximum_values(a, b, &mut out[offset..], n).unwrap();
                                    for i in 0..n {
                                        assert_eq!(
                                            out[offset + i].to_bits(),
                                            pick(lt(b[i], a[i], flush), a[i], b[i]),
                                            "max i={i} {ctx}"
                                        );
                                    }
                                    assert!(untouched(&out), "{ctx}");
                                    for (op, expected) in
                                        [(0, <$bits>::MAX >> 1), (1, <$bits>::MAX)]
                                    {
                                        let mut out = fresh();
                                        if op == 0 {
                                            ops.absolute(a, &mut out[offset..], n).unwrap();
                                        } else {
                                            ops.negate(a, &mut out[offset..], n).unwrap();
                                        }
                                        for i in 0..n {
                                            let want = if op == 0 {
                                                a[i].to_bits() & expected
                                            } else {
                                                a[i].to_bits() ^ $sign
                                            };
                                            assert_eq!(
                                                out[offset + i].to_bits(),
                                                want,
                                                "abs/neg{op} i={i} {ctx}"
                                            );
                                        }
                                        assert!(untouched(&out), "{ctx}");
                                    }
                                    let mut out = fresh();
                                    ops.copy(a, &mut out[offset..], n).unwrap();
                                    for i in 0..n {
                                        assert_eq!(
                                            out[offset + i].to_bits(),
                                            a[i].to_bits(),
                                            "copy {ctx}"
                                        );
                                    }
                                    assert!(untouched(&out), "{ctx}");
                                    let mut out = fresh();
                                    ops.add(a, b, &mut out[offset..], n).unwrap();
                                    for i in 0..n {
                                        if a[i].is_nan() || b[i].is_nan() {
                                            assert!(
                                                out[offset + i].is_nan(),
                                                "add NaN class {ctx}"
                                            );
                                        }
                                    }
                                    let mut inter = vec![sentinel; offset + 2 * n + 2];
                                    ops.interleave(a, b, &mut inter[offset..], n).unwrap();
                                    for i in 0..n {
                                        assert_eq!(
                                            inter[offset + 2 * i].to_bits(),
                                            a[i].to_bits(),
                                            "interleave {ctx}"
                                        );
                                        assert_eq!(
                                            inter[offset + 2 * i + 1].to_bits(),
                                            b[i].to_bits(),
                                            "interleave {ctx}"
                                        );
                                    }
                                    assert!(
                                        inter[..offset]
                                            .iter()
                                            .chain(&inter[offset + 2 * n..])
                                            .all(|x| x.to_bits() == sentinel.to_bits())
                                    );
                                    let (mut left, mut right) = (fresh(), fresh());
                                    ops.deinterleave(
                                        &inter[offset..],
                                        &mut left[offset..],
                                        &mut right[offset..],
                                        n,
                                    )
                                    .unwrap();
                                    for i in 0..n {
                                        assert_eq!(
                                            left[offset + i].to_bits(),
                                            a[i].to_bits(),
                                            "deinterleave {ctx}"
                                        );
                                        assert_eq!(
                                            right[offset + i].to_bits(),
                                            b[i].to_bits(),
                                            "deinterleave {ctx}"
                                        );
                                    }
                                    assert!(untouched(&left) && untouched(&right), "{ctx}");
                                    let minimum = a[..n].iter().copied().reduce(|x, y| {
                                        <$s>::from_bits(pick(lt(x, y, flush), x, y))
                                    });
                                    let maximum = a[..n].iter().copied().reduce(|x, y| {
                                        <$s>::from_bits(pick(lt(y, x, flush), x, y))
                                    });
                                    assert_eq!(
                                        ops.minimum_value(a, n).unwrap().map(<$s>::to_bits),
                                        minimum.map(<$s>::to_bits),
                                        "minimum_value {ctx}"
                                    );
                                    assert_eq!(
                                        ops.maximum_value(a, n).unwrap().map(<$s>::to_bits),
                                        maximum.map(<$s>::to_bits),
                                        "maximum_value {ctx}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        /// G14/G15: permutation duplicates/identity/single invalid index and
        /// exact-length load/store.
        #[test]
        fn $permute() {
            for backend in backends() {
                let a: [$s; $width] = core::array::from_fn(|i| i as $s + 0.5);
                let v = $p::new(backend, a).unwrap();
                let identity: [usize; $width] = core::array::from_fn(|i| i);
                assert_eq!(
                    v.permute(identity).unwrap().to_array().map(<$s>::to_bits),
                    a.map(<$s>::to_bits)
                );
                for lane in 0..$width {
                    assert_eq!(
                        v.permute([lane; $width]).unwrap().to_array(),
                        [a[lane]; $width]
                    );
                    let mut bad = identity;
                    bad[lane] = $width;
                    assert_eq!(
                        v.permute(bad).map(|x| x.to_array()),
                        Err(Error::InvalidLane)
                    );
                    bad[lane] = usize::MAX;
                    assert_eq!(
                        v.permute(bad).map(|x| x.to_array()),
                        Err(Error::InvalidLane)
                    );
                }
                let rotate: [usize; $width] = core::array::from_fn(|i| (i + 1) % $width);
                assert_eq!(v.permute(rotate).unwrap().to_array(), rotate.map(|i| a[i]));
                for offset in 0..=3 {
                    let mut stored = vec![99 as $s; offset + $width];
                    v.store(&mut stored[offset..offset + $width]).unwrap();
                    assert!(stored[..offset].iter().all(|x| *x == 99.));
                    assert_eq!(&stored[offset..], &a);
                    assert_eq!(
                        $p::load(backend, &stored[offset..offset + $width])
                            .unwrap()
                            .to_array(),
                        a
                    );
                }
            }
        }
    };
}

mod f32_review {
    use super::*;
    review!(
        packed32_all_pairs_all_lanes_both_scopes_integer_oracle,
        slice32_all_pairs_all_positions_snan_both_scopes,
        packed32_permute_domain_and_exact_length_load_store,
        F32x4,
        PreparedVectorOps32,
        f32,
        u32,
        4,
        LaneMask4,
        0x7fc00123u32,
        0x7fc00456u32,
        0x7f800123u32,
        0x80000000u32
    );
}
mod f64_review {
    use super::*;
    review!(
        packed64_all_pairs_all_lanes_both_scopes_integer_oracle,
        slice64_all_pairs_all_positions_snan_both_scopes,
        packed64_permute_domain_and_exact_length_load_store,
        F64x2,
        PreparedVectorOps64,
        f64,
        u64,
        2,
        LaneMask2,
        0x7ff8000000000123u64,
        0x7ff8000000000456u64,
        0x7ff0000000000123u64,
        0x8000000000000000u64
    );
}
