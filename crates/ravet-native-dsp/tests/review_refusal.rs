//! W254 r2 review follow-up (G5, G6, G7, G12, O2): per-parameter prefix,
//! maximum, 2N, channel, bank and control-value refusals leave every buffer
//! unchanged; clip endpoint domains are pinned against an integer oracle.
use ravet_native_dsp::{
    DenormalMode as Mode, PreparedVectorOps32, PreparedVectorOps64, ScopedDenormalMode,
    VectorBackend as Backend, VectorError as Error, current_float_state,
};

fn backends() -> [Backend; 2] {
    [Backend::Scalar, Backend::native().unwrap()]
}
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

macro_rules! refusal {
    ($prefix:ident,$bank:ident,$controls:ident,$clip:ident,$ops:ident,$s:ty,$bits:ty) => {
        const MAX: usize = 17;
        const SENTINEL: $s = 99.;
        fn pristine(buffer: &[$s]) -> bool {
            buffer.iter().all(|x| x.to_bits() == SENTINEL.to_bits())
        }
        /// Run `$call` against a fresh sentinel output of `$len` samples and
        /// require the error plus an untouched output.
        macro_rules! refused {
            ($len:expr, $error:expr, |$o:ident| $call:expr) => {{
                let mut buffer = vec![SENTINEL; $len];
                let $o = &mut buffer[..];
                let result = $call.map(|_| ());
                assert_eq!(result, Err($error), "{}", stringify!($call));
                assert!(pristine(&buffer), "{} edited output", stringify!($call));
            }};
        }

        /// G7/G5: every slice parameter of every method, one element short;
        /// n = maximum + 1; and 2N interleave/deinterleave buffers.
        #[test]
        fn $prefix() {
            for backend in backends() {
                let ops = $ops::new(backend, MAX).unwrap();
                for n in [1usize, 4, 5, 8, 17] {
                    let full: Vec<$s> = (0..2 * n).map(|i| i as $s * 0.25 + 0.5).collect();
                    let ints: Vec<i32> = (0..n as i32).collect();
                    let (x, s) = (&full[..n], &full[..n - 1]);
                    let e = Error::PrefixTooShort;
                    refused!(n - 1, e, |o| ops.fill(1., o, n));
                    refused!(n - 1, e, |o| ops.clear(o, n));
                    refused!(n, e, |o| ops.copy(s, o, n));
                    refused!(n - 1, e, |o| ops.copy(x, o, n));
                    for op in 0..6 {
                        let call = |a: &[$s], b: &[$s], o: &mut [$s]| match op {
                            0 => ops.add(a, b, o, n),
                            1 => ops.subtract(a, b, o, n),
                            2 => ops.multiply(a, b, o, n),
                            3 => ops.divide(a, b, o, n),
                            4 => ops.minimum(a, b, o, n),
                            _ => ops.maximum_values(a, b, o, n),
                        };
                        refused!(n, e, |o| call(s, x, o));
                        refused!(n, e, |o| call(x, s, o));
                        refused!(n - 1, e, |o| call(x, x, o));
                    }
                    for op in 0..5 {
                        let call = |a: &[$s], o: &mut [$s]| match op {
                            0 => ops.gain(a, 0.5, o, n),
                            1 => ops.offset(a, 0.5, o, n),
                            2 => ops.absolute(a, o, n),
                            3 => ops.negate(a, o, n),
                            _ => ops.sqrt(a, o, n),
                        };
                        refused!(n, e, |o| call(s, o));
                        refused!(n - 1, e, |o| call(x, o));
                    }
                    refused!(n - 1, e, |o| ops.gain_in_place(o, 0.5, n));
                    refused!(n, e, |o| ops.add_in_place(s, o, n));
                    refused!(n - 1, e, |o| ops.add_in_place(x, o, n));
                    refused!(n, e, |o| ops.add_product(s, x, x, o, n));
                    refused!(n, e, |o| ops.add_product(x, s, x, o, n));
                    refused!(n, e, |o| ops.add_product(x, x, s, o, n));
                    refused!(n - 1, e, |o| ops.add_product(x, x, x, o, n));
                    refused!(n, e, |o| ops.clip(s, -1., 1., o, n));
                    refused!(n - 1, e, |o| ops.clip(x, -1., 1., o, n));
                    refused!(n, e, |o| ops.convert_i32(&ints[..n - 1], 0.5, o, n));
                    refused!(n - 1, e, |o| ops.convert_i32(&ints, 0.5, o, n));
                    assert_eq!(ops.sum(s, n), Err(e));
                    assert_eq!(ops.dot(s, x, n), Err(e));
                    assert_eq!(ops.dot(x, s, n), Err(e));
                    assert_eq!(ops.minimum_value(s, n), Err(e));
                    assert_eq!(ops.maximum_value(s, n), Err(e));
                    // 2N: destination one short, then each source one short.
                    refused!(2 * n - 1, e, |o| ops.interleave(x, x, o, n));
                    refused!(2 * n, e, |o| ops.interleave(s, x, o, n));
                    refused!(2 * n, e, |o| ops.interleave(x, s, o, n));
                    let (mut a, mut b) = (vec![SENTINEL; n], vec![SENTINEL; n]);
                    assert_eq!(
                        ops.deinterleave(&full[..2 * n - 1], &mut a, &mut b, n),
                        Err(e)
                    );
                    assert!(pristine(&a) && pristine(&b));
                    let mut short = vec![SENTINEL; n - 1];
                    assert_eq!(ops.deinterleave(&full, &mut short, &mut b, n), Err(e));
                    assert!(pristine(&short) && pristine(&b));
                    assert_eq!(ops.deinterleave(&full, &mut a, &mut short, n), Err(e));
                    assert!(pristine(&a) && pristine(&short));
                }
                // n = 0 needs no storage, including the 2N forms.
                let mut empty: [$s; 0] = [];
                let mut empty2: [$s; 0] = [];
                ops.interleave(&[], &[], &mut empty, 0).unwrap();
                ops.deinterleave(&[], &mut empty, &mut empty2, 0).unwrap();
                assert_eq!(ops.sum(&[], 0), Ok(0.));
                assert_eq!(ops.minimum_value(&[], 0), Ok(None));
                // n = maximum + 1 with ample storage: TooManySamples, no edit.
                let n = MAX + 1;
                let big: Vec<$s> = vec![0.5; 4 * n];
                let ints = vec![1i32; n];
                let t = Error::TooManySamples;
                refused!(2 * n, t, |o| ops.fill(1., o, n));
                refused!(2 * n, t, |o| ops.clear(o, n));
                refused!(2 * n, t, |o| ops.copy(&big, o, n));
                refused!(2 * n, t, |o| ops.add(&big, &big, o, n));
                refused!(2 * n, t, |o| ops.subtract(&big, &big, o, n));
                refused!(2 * n, t, |o| ops.multiply(&big, &big, o, n));
                refused!(2 * n, t, |o| ops.divide(&big, &big, o, n));
                refused!(2 * n, t, |o| ops.minimum(&big, &big, o, n));
                refused!(2 * n, t, |o| ops.maximum_values(&big, &big, o, n));
                refused!(2 * n, t, |o| ops.gain(&big, 0.5, o, n));
                refused!(2 * n, t, |o| ops.offset(&big, 0.5, o, n));
                refused!(2 * n, t, |o| ops.absolute(&big, o, n));
                refused!(2 * n, t, |o| ops.negate(&big, o, n));
                refused!(2 * n, t, |o| ops.sqrt(&big, o, n));
                refused!(2 * n, t, |o| ops.gain_in_place(o, 0.5, n));
                refused!(2 * n, t, |o| ops.add_in_place(&big, o, n));
                refused!(2 * n, t, |o| ops.add_product(&big, &big, &big, o, n));
                refused!(2 * n, t, |o| ops.clip(&big, -1., 1., o, n));
                refused!(2 * n, t, |o| ops.convert_i32(&ints, 0.5, o, n));
                refused!(2 * n, t, |o| ops.interleave(&big, &big, o, n));
                assert_eq!(ops.sum(&big, n), Err(t));
                assert_eq!(ops.dot(&big, &big, n), Err(t));
                assert_eq!(ops.minimum_value(&big, n), Err(t));
                assert_eq!(ops.maximum_value(&big, n), Err(t));
                let (mut a, mut b) = (vec![SENTINEL; n], vec![SENTINEL; n]);
                assert_eq!(ops.deinterleave(&big, &mut a, &mut b, n), Err(t));
                assert!(pristine(&a) && pristine(&b));
            }
        }

        /// G5/G6: channel-count/length mismatches and the bad-gain /
        /// short-prefix / maximum matrix for both planar entry points.
        #[test]
        fn $bank() {
            for backend in backends() {
                let ops = $ops::new(backend, MAX).unwrap();
                let inputs: Vec<Vec<$s>> = (0..65)
                    .map(|ch| (0..20).map(|i| (ch + i) as $s * 0.25).collect())
                    .collect();
                let gains: Vec<$s> = (0..65).map(|ch| 0.5 + ch as $s * 0.015625).collect();
                let mut bank = vec![vec![SENTINEL; 20]; 65];
                let separate = |ops: $ops,
                                bank: &mut Vec<Vec<$s>>,
                                ins: usize,
                                outs: usize,
                                g: &[$s],
                                input_len: [usize; 2],
                                n: usize| {
                    let refs: Vec<&[$s]> = inputs[..ins]
                        .iter()
                        .enumerate()
                        .map(|(ch, v)| {
                            &v[..if ch == ins.saturating_sub(1) {
                                input_len[1]
                            } else {
                                input_len[0]
                            }]
                        })
                        .collect();
                    let mut views: Vec<&mut [$s]> =
                        bank[..outs].iter_mut().map(|v| v.as_mut_slice()).collect();
                    ops.gain_planar(&refs, g, &mut views, n)
                };
                let in_place = |ops: $ops,
                                bank: &mut Vec<Vec<$s>>,
                                outs: usize,
                                g: &[$s],
                                output_len: [usize; 2],
                                n: usize| {
                    let mut views: Vec<&mut [$s]> = bank[..outs]
                        .iter_mut()
                        .enumerate()
                        .map(|(ch, v)| {
                            &mut v[..if ch == outs.saturating_sub(1) {
                                output_len[1]
                            } else {
                                output_len[0]
                            }]
                        })
                        .collect();
                    ops.gain_planar_in_place(&mut views, g, n)
                };
                let pristine_bank = |bank: &Vec<Vec<$s>>| bank.iter().all(|c| pristine(c));
                let m = Error::ChannelMismatch;
                for n in [0, 17] {
                    let full = [20, 20];
                    // Channel-count and length mismatches.
                    assert_eq!(separate(ops, &mut bank, 0, 0, &[], full, n), Err(m));
                    assert_eq!(in_place(ops, &mut bank, 0, &[], full, n), Err(m));
                    assert_eq!(separate(ops, &mut bank, 65, 65, &gains, full, n), Err(m));
                    assert_eq!(in_place(ops, &mut bank, 65, &gains, full, n), Err(m));
                    let mut late = gains.clone();
                    late[64] = <$s>::NAN;
                    assert_eq!(separate(ops, &mut bank, 65, 65, &late, full, n), Err(m));
                    assert_eq!(in_place(ops, &mut bank, 65, &late, full, n), Err(m));
                    assert_eq!(
                        separate(ops, &mut bank, 64, 63, &gains[..64], full, n),
                        Err(m)
                    );
                    assert_eq!(
                        separate(ops, &mut bank, 64, 65, &gains[..64], full, n),
                        Err(m)
                    );
                    for g in [63, 65] {
                        assert_eq!(
                            separate(ops, &mut bank, 64, 64, &gains[..g], full, n),
                            Err(m)
                        );
                        assert_eq!(in_place(ops, &mut bank, 64, &gains[..g], full, n), Err(m));
                    }
                    assert!(
                        pristine_bank(&bank),
                        "{backend:?} n={n} channel mismatch edited bank"
                    );
                    // Late (64th-channel) invalid gains.
                    for bad in [<$s>::NAN, <$s>::INFINITY, <$s>::NEG_INFINITY] {
                        let mut g = gains[..64].to_vec();
                        g[63] = bad;
                        assert_eq!(
                            separate(ops, &mut bank, 64, 64, &g, full, n),
                            Err(Error::InvalidScalar)
                        );
                        assert_eq!(
                            in_place(ops, &mut bank, 64, &g, full, n),
                            Err(Error::InvalidScalar)
                        );
                        assert!(pristine_bank(&bank), "{backend:?} n={n} {bad} edited bank");
                    }
                    // Late (64th-channel) short prefix: refused at n=17. At n=0 a
                    // short prefix cannot exist, so admission succeeds without edits.
                    let short = [20, 16];
                    let expected = if n == 0 {
                        Ok(())
                    } else {
                        Err(Error::PrefixTooShort)
                    };
                    assert_eq!(
                        separate(ops, &mut bank, 64, 64, &gains[..64], short, n),
                        expected
                    );
                    assert_eq!(
                        in_place(ops, &mut bank, 64, &gains[..64], short, n),
                        expected
                    );
                    let short_output = |ops: $ops, bank: &mut Vec<Vec<$s>>| {
                        let refs: Vec<&[$s]> = inputs[..64].iter().map(|v| v.as_slice()).collect();
                        let mut views: Vec<&mut [$s]> = bank[..64]
                            .iter_mut()
                            .enumerate()
                            .map(|(ch, v)| &mut v[..if ch == 63 { 16 } else { 20 }])
                            .collect();
                        ops.gain_planar(&refs, &gains[..64], &mut views, n)
                    };
                    assert_eq!(short_output(ops, &mut bank), expected);
                    assert!(
                        pristine_bank(&bank),
                        "{backend:?} n={n} short prefix edited bank"
                    );
                }
                // Maximum + 1 frames for the whole bank.
                assert_eq!(
                    separate(ops, &mut bank, 64, 64, &gains[..64], [20, 20], MAX + 1),
                    Err(Error::TooManySamples)
                );
                assert_eq!(
                    in_place(ops, &mut bank, 64, &gains[..64], [20, 20], MAX + 1),
                    Err(Error::TooManySamples)
                );
                assert!(pristine_bank(&bank));
                // Positive control: the identical 64-channel bank is admitted.
                separate(ops, &mut bank, 64, 64, &gains[..64], [20, 20], 17).unwrap();
                for ch in 0..64 {
                    for i in 0..17 {
                        assert_eq!(bank[ch][i].to_bits(), (inputs[ch][i] * gains[ch]).to_bits());
                    }
                    assert!(pristine(&bank[ch][17..]));
                }
                assert!(pristine(&bank[64]));
            }
        }

        /// G12: nonfinite controls refuse without edits for every scalar entry.
        #[test]
        fn $controls() {
            for backend in backends() {
                let ops = $ops::new(backend, MAX).unwrap();
                let x: Vec<$s> = (0..9).map(|i| i as $s - 4.).collect();
                let ints = [3i32; 9];
                for n in [0, 9] {
                    for bad in [<$s>::NAN, <$s>::INFINITY, <$s>::NEG_INFINITY] {
                        let e = Error::InvalidScalar;
                        refused!(9, e, |o| ops.gain(&x, bad, o, n));
                        refused!(9, e, |o| ops.offset(&x, bad, o, n));
                        refused!(9, e, |o| ops.convert_i32(&ints, bad, o, n));
                        refused!(9, e, |o| ops.gain_in_place(o, bad, n));
                        let r = Error::InvalidRange;
                        refused!(9, r, |o| ops.clip(&x, bad, 1., o, n));
                        refused!(9, r, |o| ops.clip(&x, -1., bad, o, n));
                        refused!(9, r, |o| ops.clip(&x, bad, bad, o, n));
                    }
                    refused!(9, Error::InvalidRange, |o| ops.clip(&x, 1., -1., o, n));
                    // Control errors precede the prefix check.
                    refused!(1, Error::InvalidScalar, |o| ops.gain(&x, <$s>::NAN, o, 9));
                    refused!(1, Error::InvalidRange, |o| ops.clip(&x, 1., 0., o, 9));
                }
            }
        }

        /// G12/O2: clip endpoint domains (equal, signed-zero, subnormal) under
        /// both scopes, pinned to the integer oracle with the observed flush.
        #[test]
        fn $clip() {
            let width = <$bits>::BITS;
            let sign: $bits = 1 << (width - 1);
            let sub = <$s>::from_bits(1);
            let neg_sub = <$s>::from_bits(1 | sign);
            let inputs: Vec<$s> = [
                0,
                sign,
                1,
                1 | sign,
                2,
                <$s>::NAN.to_bits(),
                <$s>::NAN.to_bits() | sign,
                <$s>::INFINITY.to_bits(),
                <$s>::NEG_INFINITY.to_bits(),
                (0.25 as $s).to_bits(),
                (-0.25 as $s).to_bits(),
                <$s>::MAX.to_bits(),
            ]
            .into_iter()
            .map(<$s>::from_bits)
            .collect();
            for mode in [Mode::Preserve, Mode::FlushBoth] {
                let _scope = ScopedDenormalMode::enter(mode).unwrap();
                let flush = current_float_state().unwrap().input_flush();
                assert_eq!(flush, mode == Mode::FlushBoth);
                let lt = |a: $s, b: $s| {
                    ordered_less(a.to_bits() as u64, b.to_bits() as u64, width, flush)
                };
                for backend in backends() {
                    let ops = $ops::new(backend, MAX).unwrap();
                    for (low, high) in [
                        (0.25 as $s, 0.25 as $s),
                        (-0., 0.),
                        (0., -0.),
                        (-0., -0.),
                        (sub, sub),
                        (neg_sub, sub),
                        (sub, neg_sub),
                        (neg_sub, 0.),
                        (0., sub),
                    ] {
                        let admitted = !lt(high, low);
                        // O2 pin: (+sub, -sub) is refused only without input flush.
                        if low.to_bits() == sub.to_bits() && high.to_bits() == neg_sub.to_bits() {
                            assert_eq!(admitted, flush);
                        }
                        for n in 0..=inputs.len() {
                            for shift in 0..inputs.len() {
                                let x: Vec<$s> =
                                    (0..n).map(|i| inputs[(i + shift) % inputs.len()]).collect();
                                let mut out = vec![SENTINEL; n + 1];
                                let result = ops.clip(&x, low, high, &mut out, n);
                                let ctx = format!(
                                    "{mode:?} {backend:?} low={:x} high={:x} n={n}",
                                    low.to_bits(),
                                    high.to_bits()
                                );
                                if !admitted {
                                    assert_eq!(result, Err(Error::InvalidRange), "{ctx}");
                                    assert!(pristine(&out), "{ctx}");
                                    continue;
                                }
                                result.unwrap();
                                for i in 0..n {
                                    let below = if lt(x[i], low) { low } else { x[i] };
                                    let expected = if lt(high, below) { high } else { below };
                                    assert_eq!(
                                        out[i].to_bits(),
                                        expected.to_bits(),
                                        "{ctx} i={i} x={:x}",
                                        x[i].to_bits()
                                    );
                                }
                                assert!(pristine(&out[n..]), "{ctx}");
                            }
                        }
                    }
                }
            }
        }
    };
}

mod f32_refusal {
    use super::*;
    refusal!(
        slice32_every_parameter_prefix_maximum_and_2n_refusal,
        bank32_channel_mismatch_and_late_refusal_matrix,
        slice32_nonfinite_controls_refused_without_edit,
        slice32_clip_endpoint_domains_both_scopes,
        PreparedVectorOps32,
        f32,
        u32
    );
}
mod f64_refusal {
    use super::*;
    refusal!(
        slice64_every_parameter_prefix_maximum_and_2n_refusal,
        bank64_channel_mismatch_and_late_refusal_matrix,
        slice64_nonfinite_controls_refused_without_edit,
        slice64_clip_endpoint_domains_both_scopes,
        PreparedVectorOps64,
        f64,
        u64
    );
}
