//! W254 r2 review follow-up (G3, G8, G9, G13, G16, G17): bit-exact slice
//! results under the host Preserve profile, slice-level unfused witnesses,
//! distinct-lane sqrt corpus placement and special-value slice sqrt.
use ravet_native_dsp::{
    DenormalMode as Mode, F32x4, F64x2, PreparedVectorOps32, PreparedVectorOps64,
    ScopedDenormalMode, VectorBackend as Backend, current_float_state,
};
use std::hint::black_box;

fn backends() -> [Backend; 2] {
    [Backend::Scalar, Backend::native().unwrap()]
}
fn preserve() -> ScopedDenormalMode {
    let scope = ScopedDenormalMode::enter(Mode::Preserve).unwrap();
    let state = current_float_state().unwrap();
    // The qualified host default thread profile has neither flush control.
    assert!(!state.input_flush() && !state.result_flush());
    scope
}
fn corpus(precision: &str) -> Vec<(u64, u64)> {
    include_str!("data/sqrt-decimal-oracle.csv")
        .lines()
        .skip(1)
        .map(|line| line.split(',').collect::<Vec<_>>())
        .filter(|p| p[0] == precision)
        .map(|p| {
            (
                u64::from_str_radix(p[1], 16).unwrap(),
                u64::from_str_radix(p[2], 16).unwrap(),
            )
        })
        .collect()
}

macro_rules! exact {
    ($exact:ident,$unfused:ident,$sqrt:ident,$special:ident,$heap:ident,$p:ident,$ops:ident,$s:ty,$bits:ty,$width:expr,$delta:expr,$precision:expr) => {
        /// G3/G17: bit equality for every rounding operation with
        /// non-subnormal, non-NaN data; selections use an integer chooser.
        #[test]
        fn $exact() {
            let _scope = preserve();
            for backend in backends() {
                let ops = $ops::new(backend, 17).unwrap();
                for n in 0..=17 {
                    for offset in 0..=3 {
                        let len = offset + n + 3;
                        let make = |k: usize, scale: $s, bias: $s| -> Vec<$s> {
                            (0..len).map(|i| ((i * k + 13) % 101) as $s / scale + bias).collect()
                        };
                        let a = make(37, 29., -1.7);
                        let b = make(53, 31., 0.35);
                        let c = make(71, 7., -6.9);
                        let (a, b, c) = (&a[offset..], &b[offset..], &c[offset..]);
                        let mut out = vec![99 as $s; len];
                        macro_rules! check {
                            ($call:expr, |$i:ident| $expected:expr) => {
                                out.fill(99.);
                                $call.unwrap();
                                for $i in 0..n {
                                    let expected: $s = $expected;
                                    assert_eq!(
                                        out[offset + $i].to_bits(),
                                        expected.to_bits(),
                                        "{} {backend:?} n={n} offset={offset} i={}",
                                        stringify!($call),
                                        $i
                                    );
                                }
                                assert!(out[..offset].iter().chain(&out[offset + n..]).all(|x| *x == 99.));
                            };
                        }
                        let choose = |left: bool, x: $s, y: $s| if left { x.to_bits() } else { y.to_bits() };
                        check!(ops.fill(-0., &mut out[offset..], n), |i| -0.);
                        check!(ops.clear(&mut out[offset..], n), |i| 0.);
                        check!(ops.add(a, b, &mut out[offset..], n), |i| a[i] + b[i]);
                        check!(ops.subtract(a, b, &mut out[offset..], n), |i| a[i] - b[i]);
                        check!(ops.multiply(a, b, &mut out[offset..], n), |i| a[i] * b[i]);
                        check!(ops.divide(a, b, &mut out[offset..], n), |i| a[i] / b[i]);
                        check!(ops.minimum(a, b, &mut out[offset..], n), |i| <$s>::from_bits(choose(a[i] < b[i], a[i], b[i])));
                        check!(ops.maximum_values(a, b, &mut out[offset..], n), |i| <$s>::from_bits(choose(a[i] > b[i], a[i], b[i])));
                        check!(ops.gain(a, -0.37, &mut out[offset..], n), |i| a[i] * -0.37);
                        check!(ops.offset(a, 0.421, &mut out[offset..], n), |i| a[i] + 0.421);
                        check!(ops.sqrt(b, &mut out[offset..], n), |i| b[i].sqrt());
                        check!(ops.add_product(a, b, c, &mut out[offset..], n), |i| {
                            let product = a[i] * b[i];
                            product + c[i]
                        });
                        let ints: Vec<i32> = (0..n + 3).map(|i| [i32::MIN, i32::MAX, -37, 16_777_217, 65537][i % 5]).collect();
                        check!(ops.convert_i32(&ints, 0.003, &mut out[offset..], n), |i| (ints[i] as $s) * 0.003);
                        out[offset..offset + n].copy_from_slice(&a[..n]);
                        ops.gain_in_place(&mut out[offset..], 1.37, n).unwrap();
                        ops.add_in_place(c, &mut out[offset..], n).unwrap();
                        for i in 0..n {
                            assert_eq!(out[offset + i].to_bits(), ((a[i] * 1.37) + c[i]).to_bits());
                        }
                    }
                }
            }
        }

        /// G3: slice add_product is unfused in both a full chunk and a tail.
        #[test]
        fn $unfused() {
            let _scope = preserve();
            let delta: $s = $delta;
            let (x, y, z) = (black_box(1. + delta), black_box(1. - delta), black_box(-1.));
            assert_ne!(x.mul_add(y, z), 0.);
            for backend in backends() {
                for n in [$width, $width + 1] {
                    let ops = $ops::new(backend, n).unwrap();
                    let mut out = vec![99 as $s; n + 1];
                    ops.add_product(&vec![x; n], &vec![y; n], &vec![z; n], &mut out, n).unwrap();
                    assert!(out[..n].iter().all(|r| r.to_bits() == 0), "{backend:?} n={n} {out:?}");
                    assert_eq!(out[n], 99.);
                }
            }
        }

        /// G8: corpus rows at distinct lanes and tail positions (rotated).
        /// Native hardware must be exact; Scalar libm keeps the 1-ULP bound.
        #[test]
        fn $sqrt() {
            let _scope = preserve();
            let rows = corpus($precision);
            let r = rows.len();
            assert_eq!(r, 18);
            let mut observed = 0;
            for backend in backends() {
                for rotation in 0..r {
                    let row = |k: usize| rows[(rotation + k) % r];
                    let check = |got: $s, (input, expected): (u64, u64), place: &str| -> u64 {
                        let error = (got.to_bits() as u64).abs_diff(expected);
                        if backend == Backend::Scalar {
                            assert!(error <= 1, "{backend:?} {place} input={input:x} got={:x} expected={expected:x}", got.to_bits());
                        } else {
                            assert_eq!(got.to_bits() as u64, expected, "{backend:?} {place} input={input:x}");
                        }
                        error
                    };
                    let lanes: [$s; $width] = core::array::from_fn(|k| <$s>::from_bits(row(k).0 as $bits));
                    for (k, got) in $p::new(backend, black_box(lanes)).unwrap().sqrt().to_array().into_iter().enumerate() {
                        observed = observed.max(check(got, row(k), "register"));
                    }
                    for n in [r - 1, r] {
                        let input: Vec<$s> = (0..n).map(|k| <$s>::from_bits(row(k).0 as $bits)).collect();
                        let mut out = vec![99 as $s; n + 2];
                        $ops::new(backend, n).unwrap().sqrt(&input, &mut out, n).unwrap();
                        for k in 0..n {
                            observed = observed.max(check(out[k], row(k), "slice"));
                        }
                        assert_eq!(&out[n..], &[99.; 2]);
                    }
                }
            }
            println!("r2 rotated sqrt corpus {}: native exact asserted; observed maximum {observed} ULP", $precision);
        }

        /// G9/G16: slice sqrt special classes on chunks and tails, both backends.
        #[test]
        fn $special() {
            let sign: $bits = 1 << (<$bits>::BITS - 1);
            let patterns: Vec<$s> = [
                <$s>::NAN.to_bits(), <$s>::NAN.to_bits() | sign, 0, sign, 1, 1 | sign,
                <$s>::INFINITY.to_bits(), <$s>::NEG_INFINITY.to_bits(), <$s>::MAX.to_bits(),
                (-<$s>::MAX).to_bits(), (4. as $s).to_bits(), (-4. as $s).to_bits(),
                <$s>::MIN_POSITIVE.to_bits(),
            ].into_iter().map(<$s>::from_bits).collect();
            for mode in [Mode::Preserve, Mode::FlushBoth] {
                let _scope = ScopedDenormalMode::enter(mode).unwrap();
                for backend in backends() {
                    for n in 0..=17 {
                        for shift in 0..patterns.len() {
                            let input: Vec<$s> = (0..n).map(|i| patterns[(i + shift) % patterns.len()]).collect();
                            let mut out = vec![99 as $s; n + 2];
                            $ops::new(backend, 17).unwrap().sqrt(black_box(&input), &mut out, n).unwrap();
                            assert_eq!(&out[n..], &[99.; 2]);
                            for i in 0..n {
                                let (x, y) = (input[i], out[i]);
                                let ctx = format!("{mode:?} {backend:?} n={n} i={i} x={:x} y={:x}", x.to_bits(), y.to_bits());
                                // Classify by encoding: float comparisons would observe flushing.
                                let bits = x.to_bits();
                                let magnitude = bits & !sign;
                                let negative = bits & sign != 0;
                                let subnormal = magnitude != 0 && magnitude < <$s>::MIN_POSITIVE.to_bits();
                                if x.is_nan() || (negative && magnitude != 0 && !subnormal) {
                                    assert!(y.is_nan(), "{ctx}");
                                } else if magnitude == 0 || bits == <$s>::INFINITY.to_bits() {
                                    assert_eq!(y.to_bits(), x.to_bits(), "{ctx}");
                                } else if subnormal {
                                    // Policy-defined: flush scopes may observe zero.
                                    if mode == Mode::Preserve {
                                        if negative {
                                            assert!(y.is_nan(), "{ctx}");
                                        } else {
                                            assert_eq!(y.to_bits(), x.sqrt().to_bits(), "{ctx}");
                                        }
                                    }
                                } else {
                                    assert_eq!(y.to_bits(), x.sqrt().to_bits(), "{ctx}");
                                }
                            }
                        }
                    }
                }
            }
        }

        /// G13: the remaining family members inside the allocation counter.
        #[test]
        fn $heap() {
            let _scope = preserve();
            for backend in backends() {
                let ops = $ops::new(backend, 17).unwrap();
                let a: Vec<$s> = (0..17).map(|i| i as $s * 0.25).collect();
                let b: Vec<$s> = (0..17).map(|i| 1. - i as $s * 0.125).collect();
                let mut out = vec![0 as $s; 17];
                let mut inter = vec![0 as $s; 34];
                let (mut left, mut right) = (vec![0 as $s; 17], vec![0 as $s; 17]);
                let ints = [7i32; 17];
                let info = allocation_counter::measure(|| {
                    black_box(ops.sum(&a, 17).unwrap());
                    black_box(ops.dot(&a, &b, 17).unwrap());
                    black_box(ops.minimum_value(&a, 17).unwrap());
                    black_box(ops.maximum_value(&a, 17).unwrap());
                    ops.add_in_place(&a, &mut out, 17).unwrap();
                    ops.interleave(&a, &b, &mut inter, 17).unwrap();
                    ops.deinterleave(&inter, &mut left, &mut right, 17).unwrap();
                    ops.fill(1., &mut out, 17).unwrap();
                    ops.clear(&mut out, 17).unwrap();
                    ops.copy(&a, &mut out, 17).unwrap();
                    ops.subtract(&a, &b, &mut out, 17).unwrap();
                    ops.multiply(&a, &b, &mut out, 17).unwrap();
                    ops.divide(&a, &b, &mut out, 17).unwrap();
                    ops.minimum(&a, &b, &mut out, 17).unwrap();
                    ops.maximum_values(&a, &b, &mut out, 17).unwrap();
                    ops.offset(&a, 0.5, &mut out, 17).unwrap();
                    ops.absolute(&b, &mut out, 17).unwrap();
                    ops.negate(&b, &mut out, 17).unwrap();
                    ops.sqrt(&a, &mut out, 17).unwrap();
                    ops.clip(&b, -0.5, 0.5, &mut out, 17).unwrap();
                    ops.convert_i32(&ints, 0.5, &mut out, 17).unwrap();
                    ops.gain_in_place(&mut out, 0.5, 17).unwrap();
                });
                assert_eq!((info.count_total, info.count_current, info.bytes_total), (0, 0, 0));
            }
        }
    };
}

mod f32_exact {
    use super::*;
    exact!(
        slice32_bit_exact_rounding_family,
        slice32_add_product_unfused_chunk_and_tail,
        sqrt32_corpus_distinct_lanes_and_tails,
        slice32_sqrt_special_classes,
        slice32_remaining_family_heap_free,
        F32x4,
        PreparedVectorOps32,
        f32,
        u32,
        4,
        2f32.powi(-23),
        "32"
    );
}
mod f64_exact {
    use super::*;
    exact!(
        slice64_bit_exact_rounding_family,
        slice64_add_product_unfused_chunk_and_tail,
        sqrt64_corpus_distinct_lanes_and_tails,
        slice64_sqrt_special_classes,
        slice64_remaining_family_heap_free,
        F64x2,
        PreparedVectorOps64,
        f64,
        u64,
        2,
        2f64.powi(-52),
        "64"
    );
}
