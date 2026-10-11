use ravet_native_dsp::{
    DenormalMode, F32x4, F64x2, LaneMask2, LaneMask4, ScopedDenormalMode, VectorBackend as Backend,
    VectorError as Error, current_float_state,
};
fn backends() -> [Backend; 2] {
    [Backend::Scalar, Backend::native().unwrap()]
}
macro_rules! qualify {
    ($regular:ident,$special:ident,$fused:ident,$p:ident,$s:ty,$bits:ty,$width:expr,$mask:ident,$delta:expr,$qnan:expr,$snan:expr) => {
        #[test]
        fn $regular() {
            let _restore = ScopedDenormalMode::enter(DenormalMode::Preserve).unwrap();
            let control = current_float_state().unwrap().control;
            for backend in backends() {
                let a: [$s; $width] = core::array::from_fn(|i| i as $s + 0.25);
                let b: [$s; $width] = core::array::from_fn(|i| 1 as $s * (1usize << i) as $s);
                let lhs = $p::new(backend, a).unwrap();
                let rhs = $p::new(backend, b).unwrap();
                for (actual, expected) in [
                    (
                        lhs.add(rhs).unwrap().to_array(),
                        core::array::from_fn(|i| a[i] + b[i]),
                    ),
                    (
                        lhs.subtract(rhs).unwrap().to_array(),
                        core::array::from_fn(|i| a[i] - b[i]),
                    ),
                    (
                        lhs.multiply(rhs).unwrap().to_array(),
                        core::array::from_fn(|i| a[i] * b[i]),
                    ),
                    (
                        lhs.divide(rhs).unwrap().to_array(),
                        core::array::from_fn(|i| a[i] / b[i]),
                    ),
                    (
                        lhs.multiply_add(rhs, lhs).unwrap().to_array(),
                        core::array::from_fn(|i| a[i] * b[i] + a[i]),
                    ),
                ] {
                    assert_eq!(actual.map(<$s>::to_bits), expected.map(<$s>::to_bits));
                }
                let squares = core::array::from_fn(|i| ((i + 2) * (i + 2)) as $s);
                assert_eq!(
                    $p::new(backend, squares).unwrap().sqrt().to_array(),
                    core::array::from_fn(|i| (i + 2) as $s)
                );
                for i in 0..$width {
                    assert_eq!(lhs.lane(i).unwrap(), a[i]);
                    assert_eq!(lhs.replace_lane(i, 9.).unwrap().lane(i).unwrap(), 9.);
                }
                assert!(matches!(lhs.lane($width), Err(Error::InvalidLane)));
                assert!(matches!(
                    lhs.replace_lane(usize::MAX, 0.),
                    Err(Error::InvalidLane)
                ));
                let reverse = core::array::from_fn(|i| $width - 1 - i);
                assert_eq!(
                    lhs.permute(reverse).unwrap().to_array(),
                    reverse.map(|i| a[i])
                );
                assert!(matches!(
                    lhs.permute([usize::MAX; $width]),
                    Err(Error::InvalidLane)
                ));
                let (lo, hi) = lhs.interleave(rhs).unwrap();
                let combined: Vec<_> = a.iter().zip(b).flat_map(|(a, b)| [*a, b]).collect();
                assert_eq!(lo.to_array().as_slice(), &combined[..$width]);
                assert_eq!(hi.to_array().as_slice(), &combined[$width..]);
                let (l, r) = lo.deinterleave(hi).unwrap();
                assert_eq!(l.to_array(), a);
                assert_eq!(r.to_array(), b);
                let integers = core::array::from_fn(|i| [i32::MIN, i32::MAX, -17, 65537][i]);
                assert_eq!(
                    $p::from_i32(backend, integers).unwrap().to_array(),
                    integers.map(|v| v as $s)
                );
                for offset in 0..=$width + 1 {
                    let mut stored = vec![99 as $s; offset + $width + 3];
                    lhs.store(&mut stored[offset..]).unwrap();
                    assert!(stored[..offset].iter().all(|v| *v == 99.));
                    assert_eq!(&stored[offset..offset + $width], &a);
                    assert!(stored[offset + $width..].iter().all(|v| *v == 99.));
                    assert_eq!($p::load(backend, &stored[offset..]).unwrap().to_array(), a);
                }
                let mut short = [99 as $s; $width - 1];
                assert_eq!(lhs.store(&mut short), Err(Error::PrefixTooShort));
                assert!(short.iter().all(|v| *v == 99.));
                assert!(matches!(
                    $p::load(backend, &short),
                    Err(Error::PrefixTooShort)
                ));
                let sum = a
                    .chunks_exact(2)
                    .map(|pair| pair[0] + pair[1])
                    .reduce(|a, b| a + b)
                    .unwrap();
                assert_eq!(lhs.sum(), sum);
                let products: Vec<_> = a.iter().zip(b).map(|(a, b)| *a * b).collect();
                let dot = if $width == 4 {
                    (products[0] + products[1]) + (products[2] + products[3])
                } else {
                    products[0] + products[1]
                };
                assert_eq!(lhs.dot(rhs).unwrap(), dot);
            }
            let scalar = $p::broadcast(Backend::Scalar, 1.).unwrap();
            let native = $p::broadcast(Backend::native().unwrap(), 2.).unwrap();
            assert!(matches!(scalar.add(native), Err(Error::BackendMismatch)));
            assert!(matches!(scalar.equal(native), Err(Error::BackendMismatch)));
            assert!(matches!(
                scalar.multiply_add(scalar, native),
                Err(Error::BackendMismatch)
            ));
            assert!(matches!(
                scalar.interleave(native),
                Err(Error::BackendMismatch)
            ));
            assert!(matches!(
                $p::select($mask::from_bits(0).unwrap(), scalar, native),
                Err(Error::BackendMismatch)
            ));
            assert_eq!(current_float_state().unwrap().control, control);
        }
        #[test]
        fn $special() {
            let _restore = ScopedDenormalMode::enter(DenormalMode::Preserve).unwrap();
            let values = [
                <$s>::from_bits(0),
                <$s>::from_bits((1 as $bits) << (<$bits>::BITS - 1)),
                <$s>::from_bits($qnan),
                <$s>::from_bits($snan),
                <$s>::INFINITY,
                <$s>::NEG_INFINITY,
                <$s>::from_bits(1),
                -<$s>::from_bits(1),
                <$s>::MIN_POSITIVE,
                -1.,
            ];
            for backend in backends() {
                for offset in 0..values.len() {
                    let a = core::array::from_fn(|i| values[(offset + i) % values.len()]);
                    let b = core::array::from_fn(|i| values[(offset + i + 3) % values.len()]);
                    let lhs = $p::new(backend, a).unwrap();
                    let rhs = $p::new(backend, b).unwrap();
                    for (actual, expected) in [
                        (
                            lhs.add(rhs).unwrap().to_array(),
                            core::array::from_fn::<$s, $width, _>(|i| a[i] + b[i]),
                        ),
                        (
                            lhs.subtract(rhs).unwrap().to_array(),
                            core::array::from_fn::<$s, $width, _>(|i| a[i] - b[i]),
                        ),
                        (
                            lhs.multiply(rhs).unwrap().to_array(),
                            core::array::from_fn::<$s, $width, _>(|i| a[i] * b[i]),
                        ),
                        (
                            lhs.divide(rhs).unwrap().to_array(),
                            core::array::from_fn::<$s, $width, _>(|i| a[i] / b[i]),
                        ),
                        (
                            lhs.multiply_add(rhs, lhs).unwrap().to_array(),
                            core::array::from_fn::<$s, $width, _>(|i| {
                                let product = a[i] * b[i];
                                product + a[i]
                            }),
                        ),
                    ] {
                        for (actual, expected) in actual.into_iter().zip(expected) {
                            if expected.is_nan() {
                                assert!(actual.is_nan());
                            } else {
                                assert_eq!(actual.to_bits(), expected.to_bits());
                            }
                        }
                    }
                    for (actual, input) in lhs.sqrt().to_array().into_iter().zip(a) {
                        let expected = input.sqrt();
                        if expected.is_nan() {
                            assert!(actual.is_nan());
                        } else {
                            assert_eq!(actual.to_bits(), expected.to_bits());
                        }
                    }
                    for i in 0..$width {
                        assert_eq!(
                            lhs.absolute().to_array()[i].to_bits(),
                            a[i].to_bits() & (<$bits>::MAX >> 1)
                        );
                        assert_eq!(
                            lhs.negate().to_array()[i].to_bits(),
                            a[i].to_bits() ^ ((1 as $bits) << (<$bits>::BITS - 1))
                        );
                        assert_eq!(
                            lhs.minimum(rhs).unwrap().to_array()[i].to_bits(),
                            (if a[i] < b[i] { a[i] } else { b[i] }).to_bits()
                        );
                        assert_eq!(
                            lhs.maximum(rhs).unwrap().to_array()[i].to_bits(),
                            (if a[i] > b[i] { a[i] } else { b[i] }).to_bits()
                        );
                        assert_eq!(lhs.equal(rhs).unwrap().lane(i).unwrap(), a[i] == b[i]);
                        assert_eq!(lhs.not_equal(rhs).unwrap().lane(i).unwrap(), a[i] != b[i]);
                        assert_eq!(lhs.less(rhs).unwrap().lane(i).unwrap(), a[i] < b[i]);
                        assert_eq!(lhs.less_equal(rhs).unwrap().lane(i).unwrap(), a[i] <= b[i]);
                        assert_eq!(lhs.greater(rhs).unwrap().lane(i).unwrap(), a[i] > b[i]);
                        assert_eq!(
                            lhs.greater_equal(rhs).unwrap().lane(i).unwrap(),
                            a[i] >= b[i]
                        );
                    }
                    for mask in 0..1 << $width {
                        let mask = $mask::from_bits(mask).unwrap();
                        let actual = $p::select(mask, lhs, rhs).unwrap().to_array();
                        for i in 0..$width {
                            assert_eq!(
                                actual[i].to_bits(),
                                (if mask.lane(i).unwrap() { a[i] } else { b[i] }).to_bits()
                            );
                        }
                    }
                }
            }
            for bits in 0..1 << $width {
                let m = $mask::from_bits(bits).unwrap();
                assert_eq!((!m).bits(), (!bits) & ((1 << $width) - 1));
                assert_eq!(m.any(), bits != 0);
                assert_eq!(m.all(), bits == (1 << $width) - 1);
                assert_eq!(m.count(), bits.count_ones());
                assert_eq!((m & !m).bits(), 0);
                assert_eq!((m | !m).bits(), (1 << $width) - 1);
                assert_eq!((m ^ m).bits(), 0);
            }
            assert_eq!($mask::from_bits(1 << $width), Err(Error::InvalidMask));
            assert_eq!(
                $mask::from_bits(0).unwrap().lane(usize::MAX),
                Err(Error::InvalidLane)
            );
        }
        #[test]
        fn $fused() {
            let _restore = ScopedDenormalMode::enter(DenormalMode::Preserve).unwrap();
            let delta: $s = $delta;
            for backend in backends() {
                let a = std::hint::black_box(1. + delta);
                let b = std::hint::black_box(1. - delta);
                let c = std::hint::black_box(-1.);
                let result = $p::broadcast(backend, a)
                    .unwrap()
                    .multiply_add(
                        $p::broadcast(backend, b).unwrap(),
                        $p::broadcast(backend, c).unwrap(),
                    )
                    .unwrap()
                    .to_array();
                assert!(result.iter().all(|x| x.to_bits() == 0));
                assert_ne!(a.mul_add(b, c), 0.);
            }
        }
    };
}
qualify!(
    packed32_complete_arithmetic_lanes_transforms_reductions,
    packed32_nan_tie_payload_masks,
    packed32_explicit_unfused,
    F32x4,
    f32,
    u32,
    4,
    LaneMask4,
    2f32.powi(-23),
    0x7fc00123,
    0x7f800123
);
qualify!(
    packed64_complete_arithmetic_lanes_transforms_reductions,
    packed64_nan_tie_payload_masks,
    packed64_explicit_unfused,
    F64x2,
    f64,
    u64,
    2,
    LaneMask2,
    2f64.powi(-52),
    0x7ff8000000000123,
    0x7ff0000000000123
);
#[test]
fn unavailable_native_backend_is_explicitly_refused() {
    let other = if cfg!(target_arch = "aarch64") {
        Backend::X86Sse2
    } else {
        Backend::Aarch64Neon
    };
    assert!(matches!(
        F32x4::new(other, [1.; 4]),
        Err(Error::UnsupportedBackend)
    ));
    assert!(matches!(
        F64x2::load(other, &[]),
        Err(Error::UnsupportedBackend)
    ));
}
