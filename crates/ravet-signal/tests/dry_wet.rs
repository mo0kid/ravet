use ravet_signal::{
    DelayInterpolation as I, DryWetError as E, DryWetGains as G, DryWetMixer as M,
    DryWetMixer32 as M32, DryWetMixerConfig as C, PanningGains as PG, PanningLaw as L,
    PanningNormalization as N,
};
const LAWS: [L; 7] = [
    L::Linear,
    L::Balanced,
    L::Sine,
    L::SineThreeHalves,
    L::SineSquared,
    L::SquareRoot,
    L::ThreeQuarterPower,
];
const NORMS: [N; 2] = [N::UnityEndpoints, N::UnityCenter];
const POLICIES: [I; 4] = [I::Integer, I::Linear, I::Lagrange3rd, I::Thiran1st];
fn close(a: f64, b: f64, e: f64) {
    assert!((a - b).abs() <= e, "{a} != {b}, {} > {e}", (a - b).abs());
}
fn config(channels: usize, capacity: usize, policy: I) -> C {
    C {
        channels,
        maximum_frames: 65536,
        fifo_capacity_frames: capacity,
        maximum_latency_samples: 17,
        interpolation: policy,
        ramp_frames: 17,
    }
}
fn curve(u: f64, l: L, n: N) -> [f64; 2] {
    let gains = match l {
        L::Linear => [1. - u, u],
        L::Balanced => [(2. * (1. - u)).min(1.), (2. * u).min(1.)],
        L::Sine => [
            (std::f64::consts::FRAC_PI_2 * u).cos(),
            (std::f64::consts::FRAC_PI_2 * u).sin(),
        ],
        L::SineThreeHalves => [
            (std::f64::consts::FRAC_PI_2 * u).cos().powf(1.5),
            (std::f64::consts::FRAC_PI_2 * u).sin().powf(1.5),
        ],
        L::SineSquared => [
            (std::f64::consts::FRAC_PI_2 * u).cos().powi(2),
            (std::f64::consts::FRAC_PI_2 * u).sin().powi(2),
        ],
        L::SquareRoot => [(1. - u).sqrt(), u.sqrt()],
        L::ThreeQuarterPower => [(1. - u).powf(0.75), u.powf(0.75)],
    };
    let center = match l {
        L::Linear | L::SineSquared => 0.5,
        L::Balanced => 1.,
        L::Sine | L::SquareRoot => 1. / 2_f64.sqrt(),
        _ => 1. / 8_f64.sqrt().sqrt(),
    };
    if n == N::UnityCenter {
        gains.map(|v| v / center)
    } else {
        gains
    }
}
#[test]
fn full_direct_wet_laws_precisions_extreme_coordinate_and_frozen_panner_bits() {
    for l in LAWS {
        for n in NORMS {
            for i in 0..=10000 {
                let wet = i as f64 / 10000.;
                let g = G::new(wet, l, n).unwrap();
                let q = g.quantize_f32();
                assert_eq!(g.wet_fraction(), wet);
                assert_eq!(q.wet_fraction(), wet);
                assert_eq!(g.law(), l);
                assert_eq!(q.law(), l);
                assert_eq!(g.normalization(), n);
                assert_eq!(q.normalization(), n);
                for ch in 0..2 {
                    close(g.gains()[ch], curve(wet, l, n)[ch], 2e-14);
                    close(q.gains()[ch] as f64, curve(wet, l, n)[ch], 1.3e-7);
                    assert_eq!(q.gains()[ch], g.gains()[ch] as f32);
                }
            }
            for wet in [
                f64::from_bits(1),
                f64::MIN_POSITIVE,
                1e-300,
                1e-50,
                1e-30,
                1e-12,
            ] {
                let g = G::new(wet, l, n).unwrap();
                if curve(wet, l, n)[1] > 0. {
                    assert!(g.gains()[1] > 0.);
                } else {
                    assert_eq!(g.gains()[1], 0.);
                }
                assert_eq!(g.wet_fraction(), wet);
                if matches!(l, L::SquareRoot | L::ThreeQuarterPower) {
                    assert!(g.quantize_f32().gains()[1] > 0. || wet < 1e-50);
                }
            }
            for bad in [-1e-300, -1., 1.0000000001, f64::NAN, f64::INFINITY] {
                assert_eq!(G::new(bad, l, n), Err(E::InvalidMix));
            }
            // Independent exact frozen W242 expressions/branch order, including tiny
            // nonzero pan whose near/far coordinate rounds to center. No helper call.
            for pan in [
                -1.,
                -0.9999,
                -0.39,
                -f64::from_bits(1),
                -1e-20,
                -0.,
                0.,
                f64::from_bits(1),
                1e-20,
                0.71,
                1.,
            ] {
                let center = l.center_gain();
                let endpoint = if n == N::UnityEndpoints {
                    1.
                } else {
                    1. / center
                };
                let p: f64 = pan.abs();
                let mut expected = if p == 0. {
                    [if n == N::UnityCenter { 1. } else { center }; 2]
                } else if p == 1. {
                    [endpoint, 0.]
                } else {
                    let far = (1. - p) * 0.5;
                    let near = 1. - far;
                    let pair = match l {
                        L::Linear => [near, far],
                        L::Balanced => [1., 2. * far],
                        L::SquareRoot => [near.sqrt(), far.sqrt()],
                        L::ThreeQuarterPower => [near.powf(0.75), far.powf(0.75)],
                        _ => {
                            let angle = std::f64::consts::FRAC_PI_2 * far;
                            let pair = [angle.cos(), angle.sin()];
                            match l {
                                L::SineThreeHalves => pair.map(|v| v.powf(1.5)),
                                L::SineSquared => pair.map(|v| v * v),
                                _ => pair,
                            }
                        }
                    };
                    pair.map(|v| v * endpoint)
                };
                if pan > 0. {
                    expected.swap(0, 1);
                }
                let actual = PG::new(pan, l, n).unwrap();
                assert_eq!(actual.gains().map(f64::to_bits), expected.map(f64::to_bits));
            }
        }
    }
    let wet = 1e-300;
    assert_eq!(2. * wet - 1., -1.);
    assert!(
        G::new(wet, L::SquareRoot, N::UnityEndpoints)
            .unwrap()
            .gains()[1]
            > 0.
    );
}
// Independent linear combination of causal taps, with cubic basis constructed
// from a product over nodes rather than expanded production coefficients.
fn delayed(history: &[f64], index: usize, d: f64, p: I, prev: &mut [f64; 2]) -> f64 {
    let n = d.floor() as usize;
    let f = d - n as f64;
    let tap = |k: usize| {
        if index >= n + k {
            history[index - n - k]
        } else {
            0.
        }
    };
    let v = tap(0);
    let y = if f == 0. || p == I::Integer {
        v
    } else {
        match p {
            I::Integer => unreachable!(),
            I::Linear => (1. - f) * v + f * tap(1),
            I::Lagrange3rd => (0..4)
                .map(|j| {
                    tap(j)
                        * (0..4)
                            .filter(|&k| k != j)
                            .map(|k| (f - k as f64) / (j as f64 - k as f64))
                            .product::<f64>()
                })
                .sum(),
            I::Thiran1st => {
                let a = (1. - f) / (1. + f);
                a * v + prev[0] - a * prev[1]
            }
        }
    };
    if p == I::Thiran1st {
        *prev = [v, y];
    }
    y
}
macro_rules! queue {
    ($name:ident,$t:ty,$m:ty,$convert:expr,$tol:expr) => {
        #[test]
        fn $name() {
            for policy in POLICIES {
                for channels in 1..=64 {
                    for d in [0., 1., 1.25, 4.75, 17.] {
                        let initial = ($convert)(G::new(0., L::Linear, N::UnityEndpoints).unwrap());
                        let target =
                            ($convert)(G::new(0.71, L::SineThreeHalves, N::UnityCenter).unwrap());
                        let mut m = <$m>::new(config(channels, 17, policy), initial).unwrap();
                        m.set_wet_latency_samples(d).unwrap();
                        m.set_target(target);
                        let dry: Vec<Vec<$t>> = (0..channels)
                            .map(|ch| {
                                (0..137)
                                    .map(|i| {
                                        ((i as f64 * (0.071 + ch as f64 * 0.001)).sin()
                                            * (0.5 + ch as f64 / 100.))
                                            as $t
                                    })
                                    .collect()
                            })
                            .collect();
                        let wet: Vec<Vec<$t>> = (0..channels)
                            .map(|ch| {
                                (0..137)
                                    .map(|i| {
                                        ((i as f64 * (0.17 + ch as f64 * 0.002)).cos() * 0.31) as $t
                                    })
                                    .collect()
                            })
                            .collect();
                        let mut expected: Vec<Vec<f64>> = vec![vec![0.; 137]; channels];
                        for ch in 0..channels {
                            let h: Vec<f64> = dry[ch].iter().map(|&v| v as f64).collect();
                            let mut prev = [0.; 2];
                            for i in 0..137 {
                                let actual_d = if policy == I::Integer {
                                    d.floor()
                                } else {
                                    d.floor() + (d - d.floor()) as $t as f64
                                };
                                let dry = delayed(&h, i, actual_d, policy, &mut prev);
                                let frac = ((i + 1).min(17)) as f64 / 17.;
                                let gains: [$t; 2] = if i + 1 >= 17 {
                                    target.gains()
                                } else {
                                    std::array::from_fn(|c| {
                                        (initial.gains()[c] as f64
                                            + (target.gains()[c] as f64
                                                - initial.gains()[c] as f64)
                                                * frac) as $t
                                    })
                                };
                                expected[ch][i] =
                                    (dry as $t * gains[0] + wet[ch][i] * gains[1]) as f64;
                            }
                        }
                        let mut pushed = 0;
                        let mut consumed = 0;
                        let mut turn = 0;
                        while consumed < 137 {
                            let push = [1, 17, 0, 3, 7, 2][turn % 6]
                                .min(m.free_frames())
                                .min(137 - pushed);
                            let before = m.remaining_ramp_frames();
                            let inputs: Vec<&[$t]> =
                                dry.iter().map(|ch| &ch[pushed..pushed + push]).collect();
                            let measured =
                                allocation_counter::measure(|| m.push_dry(&inputs, push).unwrap());
                            assert_eq!(measured.count_total, 0);
                            assert_eq!(m.remaining_ramp_frames(), before);
                            pushed += push;
                            let count = [0, 2, 7, 1, 17, 3][turn % 6]
                                .min(m.queued_frames())
                                .min(137 - consumed);
                            let mut output = vec![vec![99. as $t; count + 3]; channels];
                            let mut outputs: Vec<_> =
                                output.iter_mut().map(Vec::as_mut_slice).collect();
                            let inputs: Vec<&[$t]> = wet
                                .iter()
                                .map(|ch| &ch[consumed..consumed + count])
                                .collect();
                            let measured = allocation_counter::measure(|| {
                                m.mix_wet(&inputs, &mut outputs, count).unwrap()
                            });
                            assert_eq!(measured.count_total, 0);
                            assert_eq!(measured.count_current, 0);
                            for ch in 0..channels {
                                for i in 0..count {
                                    close(output[ch][i] as f64, expected[ch][consumed + i], $tol);
                                }
                                assert!(output[ch][count..].iter().all(|&v| v == 99.));
                            }
                            consumed += count;
                            assert_eq!(
                                m.remaining_ramp_frames(),
                                17_u32.saturating_sub(consumed as u32)
                            );
                            assert_eq!(m.accepted_dry_frames(), pushed as u64);
                            assert_eq!(m.consumed_mix_frames(), consumed as u64);
                            assert_eq!(m.queued_frames(), pushed - consumed);
                            turn += 1;
                            assert!(turn < 1000);
                        }
                        assert_eq!(m.queued_frames(), 0);
                        m.reset();
                        assert_eq!(m.current_gains(), target.gains());
                        assert_eq!(m.remaining_ramp_frames(), 0);
                        assert_eq!(m.wet_latency_samples(), d);
                        assert_eq!(m.accepted_dry_frames(), 0);
                        assert_eq!(m.consumed_mix_frames(), 0);
                        // Fresh versus reset history, full pending prefix, in-place consume.
                        let inputs: Vec<&[$t]> = dry.iter().map(|ch| &ch[..17]).collect();
                        m.push_dry(&inputs, 17).unwrap();
                        let mut fresh = <$m>::new(config(channels, 17, policy), target).unwrap();
                        fresh.set_wet_latency_samples(d).unwrap();
                        fresh.push_dry(&inputs, 17).unwrap();
                        let mut a: Vec<Vec<$t>> = wet.iter().map(|ch| ch[..20].to_vec()).collect();
                        let mut b = a.clone();
                        let mut ar: Vec<_> = a.iter_mut().map(Vec::as_mut_slice).collect();
                        let mut br: Vec<_> = b.iter_mut().map(Vec::as_mut_slice).collect();
                        m.mix_wet_in_place(&mut ar, 17).unwrap();
                        fresh.mix_wet_in_place(&mut br, 17).unwrap();
                        assert_eq!(a, b);
                        assert!(a.iter().zip(&wet).all(|(a, w)| a[17..] == w[17..20]));
                    }
                }
            }
            // Actual wrap work and full admission at both smallest/large queues.
            for capacity in [1, 3, 17, 65536] {
                let g = ($convert)(G::new(0., L::Linear, N::UnityEndpoints).unwrap());
                let mut m = <$m>::new(config(1, capacity, I::Integer), g).unwrap();
                let x: Vec<$t> = (0..capacity).map(|i| (i % 113) as $t).collect();
                let mut y = vec![99. as $t; capacity];
                for _ in 0..5 {
                    m.push_dry(&[&x], capacity).unwrap();
                    assert_eq!(m.free_frames(), 0);
                    m.mix_wet(&[&x], &mut [&mut y], capacity).unwrap();
                    assert_eq!(x, y);
                }
                assert_eq!(m.accepted_dry_frames(), (capacity * 5) as u64);
            }
        }
    };
}
queue!(
    native64_all_channels_policies_and_partitioned_fifo_work,
    f64,
    M,
    |g: G| g,
    5e-15
);
queue!(
    native32_all_channels_policies_and_partitioned_fifo_work,
    f32,
    M32,
    |g: G| g.quantize_f32(),
    1e-6
);
macro_rules! failures {
    ($name:ident,$t:ty,$m:ty,$convert:expr) => {
        #[test]
        fn $name() {
            let g = ($convert)(G::new(0., L::Linear, N::UnityEndpoints).unwrap());
            let target = ($convert)(G::new(0.71, L::Sine, N::UnityCenter).unwrap());
            for policy in POLICIES {
                let mut c = config(2, 3, policy);
                c.maximum_frames = 4;
                let mut m = <$m>::new(c, g).unwrap();
                let mut reference = <$m>::new(c, g).unwrap();
                for x in [&mut m, &mut reference] {
                    x.set_wet_latency_samples(1.25).unwrap();
                    x.set_target(target);
                    x.push_dry(&[&[0.7, -0.31, 0.9], &[0.13, 0.4, -0.7]], 3)
                        .unwrap();
                }
                let mut y = [[99. as $t; 6]; 2];
                assert_eq!(m.push_dry(&[&[0.1], &[0.2]], 1), Err(E::QueueFull));
                assert_eq!(m.push_dry(&[&[0.1], &[]], 1), Err(E::BufferTooShort));
                assert_eq!(m.push_dry(&[&[0.1]], 1), Err(E::ChannelCount));
                assert_eq!(m.push_dry(&[&[0.; 5], &[0.; 5]], 5), Err(E::TooManyFrames));
                {
                    let [a, b] = &mut y;
                    assert_eq!(
                        m.mix_wet(&[&[0.2; 4], &[0.3; 4]], &mut [a, b], 4),
                        Err(E::InsufficientDry)
                    );
                }
                {
                    let [a, b] = &mut y;
                    assert_eq!(
                        m.mix_wet(&[&[0.2; 3], &[0.3; 2]], &mut [a, b], 3),
                        Err(E::BufferTooShort)
                    );
                }
                {
                    let [a, b] = &mut y;
                    assert_eq!(
                        m.mix_wet(&[&[0.2; 3], &[0.3; 3]], &mut [a, &mut b[..2]], 3),
                        Err(E::BufferTooShort)
                    );
                }
                {
                    let [a, b] = &mut y;
                    assert_eq!(
                        m.mix_wet(&[&[0.2; 5], &[0.3; 5]], &mut [a, b], 5),
                        Err(E::TooManyFrames)
                    );
                }
                {
                    let [a, _] = &mut y;
                    assert_eq!(
                        m.mix_wet(&[&[0.2; 3], &[0.3; 3]], &mut [a], 3),
                        Err(E::ChannelCount)
                    );
                }
                {
                    let [a, b] = &mut y;
                    assert_eq!(
                        m.mix_wet_in_place(&mut [a, &mut b[..2]], 3),
                        Err(E::BufferTooShort)
                    );
                }
                {
                    let [a, b] = &mut y;
                    assert_eq!(m.mix_wet_in_place(&mut [a, b], 4), Err(E::InsufficientDry));
                }
                {
                    let [a, b] = &mut y;
                    assert_eq!(m.mix_wet_in_place(&mut [a, b], 5), Err(E::TooManyFrames));
                }
                {
                    let [a, _] = &mut y;
                    assert_eq!(m.mix_wet_in_place(&mut [a], 3), Err(E::ChannelCount));
                }
                assert_eq!(m.set_wet_latency_samples(2.), Err(E::PendingDry));
                m.set_wet_latency_samples(1.25).unwrap();
                for invalid in [-1., 18., f64::NAN, f64::INFINITY] {
                    assert_eq!(m.set_wet_latency_samples(invalid), Err(E::InvalidLatency));
                }
                assert!(y.iter().all(|c| c.iter().all(|&v| v == 99.)));
                assert_eq!(m.queued_frames(), 3);
                assert_eq!(m.accepted_dry_frames(), 3);
                assert_eq!(m.consumed_mix_frames(), 0);
                assert_eq!(m.remaining_ramp_frames(), 17);
                let mut expected = y;
                {
                    let [a, b] = &mut y;
                    let [ra, rb] = &mut expected;
                    m.mix_wet(&[&[0.2; 3], &[0.3; 3]], &mut [a, b], 3).unwrap();
                    reference
                        .mix_wet(&[&[0.2; 3], &[0.3; 3]], &mut [ra, rb], 3)
                        .unwrap();
                }
                assert_eq!(y, expected);
                for x in [&mut m, &mut reference] {
                    x.set_wet_latency_samples(0.).unwrap();
                    x.push_dry(&[&[0.17, -0.71], &[0.3, -0.2]], 2).unwrap();
                }
                let mut y = [[99. as $t; 4]; 2];
                let mut e = y;
                {
                    let [a, b] = &mut y;
                    let [ea, eb] = &mut e;
                    m.mix_wet(&[&[0.2; 2], &[0.3; 2]], &mut [a, b], 2).unwrap();
                    reference
                        .mix_wet(&[&[0.2; 2], &[0.3; 2]], &mut [ea, eb], 2)
                        .unwrap();
                }
                assert_eq!(y, e);
                let before = m.remaining_ramp_frames();
                m.push_dry(&[&[], &[]], 0).unwrap();
                m.mix_wet(&[&[], &[]], &mut [&mut [], &mut []], 0).unwrap();
                m.mix_wet_in_place(&mut [&mut [], &mut []], 0).unwrap();
                assert_eq!(m.remaining_ramp_frames(), before);
                let measured = allocation_counter::measure(|| {
                    for _ in 0..10000 {
                        m.reset();
                        m.set_target(g);
                        m.push_dry(&[&[0.1], &[0.3]], 1).unwrap();
                        assert_eq!(m.set_wet_latency_samples(2.), Err(E::PendingDry));
                        m.mix_wet_in_place(&mut [&mut [0.7], &mut [0.9]], 1)
                            .unwrap();
                        m.set_wet_latency_samples(1.25).unwrap();
                    }
                });
                assert_eq!(measured.count_total, 0);
                assert_eq!(measured.count_current, 0);
            }
            let mut m = <$m>::new(config(1, 3, I::Thiran1st), g).unwrap();
            m.set_wet_latency_samples(0.5).unwrap();
            m.push_dry(&[&[<$t>::NAN]], 1).unwrap();
            let mut out = [99.];
            m.mix_wet(&[&[0.]], &mut [&mut out], 1).unwrap();
            assert!(out[0].is_nan());
            m.push_dry(&[&[0.1]], 1).unwrap();
            m.mix_wet(&[&[0.]], &mut [&mut out], 1).unwrap();
            assert!(out[0].is_nan());
            m.reset();
            m.push_dry(&[&[0.1]], 1).unwrap();
            m.mix_wet(&[&[0.]], &mut [&mut out], 1).unwrap();
            assert!(out[0].is_finite());
            let instant = ($convert)(G::new(0., L::Linear, N::UnityCenter).unwrap());
            m.set_target(instant);
            m.reset();
            m.set_wet_latency_samples(0.).unwrap();
            m.push_dry(&[&[<$t>::MAX]], 1).unwrap();
            m.mix_wet(&[&[0.]], &mut [&mut out], 1).unwrap();
            assert!(out[0].is_infinite());
        }
    };
}
failures!(
    native64_whole_admission_pending_latency_and_recovery_heap_proof,
    f64,
    M,
    |g: G| g
);
failures!(
    native32_whole_admission_pending_latency_and_recovery_heap_proof,
    f32,
    M32,
    |g: G| g.quantize_f32()
);
#[test]
fn aggregate_pre_reserve_configuration_and_native_fractional_representability() {
    let g = G::new(0.3, L::Linear, N::UnityEndpoints).unwrap();
    let mut variants = Vec::new();
    for channels in [0, 65, usize::MAX] {
        let mut c = config(1, 3, I::Linear);
        c.channels = channels;
        variants.push(c);
    }
    for maximum in [0, 65537, usize::MAX] {
        let mut c = config(1, 3, I::Linear);
        c.maximum_frames = maximum;
        variants.push(c);
    }
    for capacity in [0, 65537, usize::MAX] {
        let mut c = config(1, 3, I::Linear);
        c.fifo_capacity_frames = capacity;
        variants.push(c);
    }
    for delay in [4_194_301, usize::MAX] {
        let mut c = config(1, 3, I::Linear);
        c.maximum_latency_samples = delay;
        variants.push(c);
    }
    let mut c = config(64, 65536, I::Linear);
    c.maximum_latency_samples = 65536;
    variants.push(c);
    for c in variants {
        let measured = allocation_counter::measure(|| {
            assert!(matches!(M::new(c, g), Err(E::Configuration)));
            assert!(matches!(
                M32::new(c, g.quantize_f32()),
                Err(E::Configuration)
            ));
        });
        assert_eq!(measured.count_total, 0);
    }
    // Exact declared byte boundary, including fixed128 native recursive values.
    let mut c = config(1, 1, I::Integer);
    c.maximum_latency_samples = (32 * 1024 * 1024 / 8) - 1 - 4 - 128;
    let m = M::new(c, g).unwrap();
    assert_eq!(m.config(), c);
    drop(m);
    c.maximum_latency_samples += 1;
    let measured =
        allocation_counter::measure(|| assert!(matches!(M::new(c, g), Err(E::Configuration))));
    assert_eq!(measured.count_total, 0);
    for p in POLICIES {
        let mut a = M::new(config(1, 3, p), g).unwrap();
        let mut b = M32::new(config(1, 3, p), g.quantize_f32()).unwrap();
        for delay in [f64::from_bits(1), 1e-50, 1e-12, 1. - 1e-12, 0.25, 17.] {
            let before = (
                b.wet_latency_samples(),
                b.native_fraction(),
                b.native_thiran_coefficient(),
            );
            let r = b.set_wet_latency_samples(delay);
            let f = (delay - delay.floor()) as f32;
            let reject = p != I::Integer
                && delay.fract() > 0.
                && (!(0. < f && f < 1.) || (p == I::Thiran1st && (1_f32 - f) / (1_f32 + f) >= 1.));
            assert_eq!(r.is_err(), reject);
            if reject {
                assert_eq!(
                    (
                        b.wet_latency_samples(),
                        b.native_fraction(),
                        b.native_thiran_coefficient()
                    ),
                    before
                );
            }
            let r = a.set_wet_latency_samples(delay);
            let f = delay - delay.floor();
            assert_eq!(
                r.is_err(),
                p == I::Thiran1st && f > 0. && (1. - f) / (1. + f) >= 1.
            );
        }
    }
}
macro_rules! spectrum {
    ($name:ident,$t:ty,$m:ty,$convert:expr,$tol:expr) => {
        #[test]
        fn $name() {
            let g = ($convert)(G::new(0., L::Linear, N::UnityEndpoints).unwrap());
            for policy in POLICIES {
                for d in [0., 2., 2.25, 2.75, 4.5] {
                    let mut m = <$m>::new(config(1, 17, policy), g).unwrap();
                    m.set_wet_latency_samples(d).unwrap();
                    let mut impulse = vec![0. as $t; 4096];
                    impulse[0] = 1.;
                    let mut actual = vec![0. as $t; 4096];
                    let mut start = 0;
                    while start < 4096 {
                        let n = 17.min(4096 - start);
                        m.push_dry(&[&impulse[start..start + n]], n).unwrap();
                        m.mix_wet(&[&[0.; 17]], &mut [&mut actual[start..start + n]], n)
                            .unwrap();
                        start += n;
                    }
                    let n = d.floor() as usize;
                    let f = m.native_fraction() as f64;
                    let a = m.native_thiran_coefficient() as f64;
                    let coefficients: Vec<f64> = if f == 0. || policy == I::Integer {
                        vec![1.]
                    } else if policy == I::Linear {
                        vec![1. - f, f]
                    } else if policy == I::Lagrange3rd {
                        (0..4)
                            .map(|j| {
                                (0..4)
                                    .filter(|&k| k != j)
                                    .map(|k| (f - k as f64) / (j as f64 - k as f64))
                                    .product()
                            })
                            .collect()
                    } else {
                        (0..4096 - n)
                            .map(|i| {
                                if i == 0 {
                                    a
                                } else {
                                    (1. - a * a) * (-a).powi(i as i32 - 1)
                                }
                            })
                            .collect()
                    };
                    for (i, &y) in actual.iter().enumerate() {
                        let expected = if i < n {
                            0.
                        } else {
                            coefficients.get(i - n).copied().unwrap_or(0.)
                        };
                        close(y as f64, expected, $tol);
                    }
                    for frequency in [0., 0.001, 0.031, 0.17, 0.25, 0.49] {
                        let omega = std::f64::consts::TAU * frequency;
                        let re: f64 = actual
                            .iter()
                            .enumerate()
                            .map(|(i, &y)| y as f64 * (omega * i as f64).cos())
                            .sum();
                        let im: f64 = actual
                            .iter()
                            .enumerate()
                            .map(|(i, &y)| -y as f64 * (omega * i as f64).sin())
                            .sum();
                        let expected = if policy == I::Thiran1st && f > 0. {
                            1.
                        } else {
                            let re: f64 = coefficients
                                .iter()
                                .enumerate()
                                .map(|(i, &c)| c * (omega * i as f64).cos())
                                .sum();
                            let im: f64 = coefficients
                                .iter()
                                .enumerate()
                                .map(|(i, &c)| -c * (omega * i as f64).sin())
                                .sum();
                            re.hypot(im)
                        };
                        let tail = if policy == I::Thiran1st && f > 0. {
                            (1. - a * a) * a.powi((4096 - n - 1) as i32) / (1. - a)
                        } else {
                            0.
                        };
                        close(re.hypot(im), expected, tail + $tol * 8.);
                    }
                    if policy == I::Thiran1st && f > 0. {
                        let actual_dc = n as f64 + (1. - a) / (1. + a);
                        close(
                            actual_dc,
                            d,
                            if std::mem::size_of::<$t>() == 4 {
                                2e-7
                            } else {
                                1e-15
                            },
                        );
                        // Finite impulse first moment differs from DC group delay by
                        // the separately bounded geometric remainder, not by hidden trim.
                        let first: f64 = actual
                            .iter()
                            .enumerate()
                            .map(|(i, &v)| i as f64 * v as f64)
                            .sum();
                        close(first, actual_dc, $tol * 8.);
                    }
                }
            }
        }
    };
}
spectrum!(
    native64_causal_impulse_polynomial_and_thiran_allpass_timing,
    f64,
    M,
    |g: G| g,
    3e-14
);
spectrum!(
    native32_causal_impulse_polynomial_and_actual_thiran_parameters,
    f32,
    M32,
    |g: G| g.quantize_f32(),
    3e-7
);
#[test]
fn consumption_only_long_ramp_and_late_retarget_use_exact_pair_policy() {
    let initial = G::new(0., L::Linear, N::UnityEndpoints).unwrap();
    let target = G::new(1., L::Linear, N::UnityEndpoints).unwrap();
    for duration in [0, 1, 2, 17, u32::MAX] {
        let mut c = config(3, 17, I::Integer);
        c.ramp_frames = duration;
        let mut m = M::new(c, initial).unwrap();
        m.set_target(target);
        let count = if duration == u32::MAX {
            1024
        } else {
            duration.max(1) + 2
        };
        for i in 1..=count {
            m.push_dry(&[&[1.], &[0.3], &[-0.7]], 1).unwrap();
            assert_eq!(m.remaining_ramp_frames(), duration.saturating_sub(i - 1));
            let mut y = [[99.; 1]; 3];
            let [a, b, c] = &mut y;
            m.mix_wet(&[&[0.], &[0.], &[0.]], &mut [a, b, c], 1)
                .unwrap();
            let gain = if duration == 0 || i >= duration {
                0.
            } else {
                1. - i as f64 / duration as f64
            };
            assert_eq!(y, [[gain], [0.3 * gain], [-0.7 * gain]]);
            assert_eq!(m.remaining_ramp_frames(), duration.saturating_sub(i));
            m.set_target(G::new(1., L::Balanced, N::UnityCenter).unwrap());
            assert_eq!(m.remaining_ramp_frames(), duration.saturating_sub(i));
        }
        let current = m.current_gains();
        let target = G::new(0.4, L::SquareRoot, N::UnityCenter).unwrap();
        m.set_target(target);
        m.push_dry(&[&[1.], &[1.], &[1.]], 1).unwrap();
        let mut y = [[0.; 1]; 3];
        let [a, b, c] = &mut y;
        m.mix_wet(&[&[0.], &[0.], &[0.]], &mut [a, b, c], 1)
            .unwrap();
        let expected = if duration <= 1 {
            target.gains()[0]
        } else {
            current[0] + (target.gains()[0] - current[0]) * (1. / duration as f64)
        };
        assert_eq!(y, [[expected]; 3]);
        m.reset();
        assert_eq!(m.current_gains(), target.gains());
    }
}
macro_rules! graph {
    ($name:ident,$t:ty,$g:ty,$m:ty,$convert:expr,$tol:expr) => {
        #[test]
        fn $name() {
            use ravet_foundation::{
                AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
                Processor, Transport,
            };
            use ravet_graph::{
                Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source,
            };
            use ravet_signal::FirConvolver;
            use std::sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            };
            struct Consumer {
                profiles: Vec<$g>,
                mixer: $m,
                wet: [FirConvolver; 3],
                layout: AudioLayout,
                config: Option<ProcessConfig>,
                position: usize,
                accepted: Arc<AtomicUsize>,
                consumed: Arc<AtomicUsize>,
                deactivations: Arc<AtomicUsize>,
            }
            impl Processor<$t> for Consumer {
                fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
                    self.config = None;
                    if c.layout() != self.layout {
                        return Err(PrepareError::UnsupportedLayout);
                    }
                    self.config = Some(c);
                    self.reset();
                    Ok(())
                }
                fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, $t>) -> ProcessStatus {
                    if let Err(e) = self.config.unwrap().validate_block(b) {
                        b.clear_outputs();
                        return ProcessStatus::Fault(e);
                    }
                    let frames = b.audio.frames();
                    let split = frames / 3;
                    let inputs = [
                        b.audio.input(0).unwrap(),
                        b.audio.input(1).unwrap(),
                        b.audio.input(2).unwrap(),
                    ];
                    self.mixer
                        .push_dry(
                            &[
                                &inputs[0][..split],
                                &inputs[1][..split],
                                &inputs[2][..split],
                            ],
                            split,
                        )
                        .unwrap();
                    self.mixer
                        .push_dry(
                            &[
                                &inputs[0][split..frames],
                                &inputs[1][split..frames],
                                &inputs[2][split..frames],
                            ],
                            frames - split,
                        )
                        .unwrap();
                    let mut scratch = [[0. as $t; 67]; 3];
                    for ch in 0..3 {
                        for i in 0..frames {
                            // Exact caller-selected three-sample wet latency plus
                            // real downstream FIR. Native32 bridge is explicit.
                            scratch[ch][i] =
                                self.wet[ch].process_sample(inputs[ch][i] as f64) as $t;
                        }
                    }
                    for i in 0..frames {
                        self.mixer
                            .set_target(self.profiles[(self.position / 29) % self.profiles.len()]);
                        let mut y = [[0. as $t; 1]; 3];
                        let [a, c, d] = &mut y;
                        self.mixer
                            .mix_wet(
                                &[
                                    &scratch[0][i..i + 1],
                                    &scratch[1][i..i + 1],
                                    &scratch[2][i..i + 1],
                                ],
                                &mut [a, c, d],
                                1,
                            )
                            .unwrap();
                        for (ch, v) in y.iter().enumerate() {
                            b.audio.output_mut(ch).unwrap()[i] = v[0];
                        }
                        self.position += 1;
                    }
                    self.accepted
                        .store(self.mixer.accepted_dry_frames() as usize, Ordering::Relaxed);
                    self.consumed
                        .store(self.mixer.consumed_mix_frames() as usize, Ordering::Relaxed);
                    assert_eq!(self.mixer.queued_frames(), 0);
                    ProcessStatus::Continue
                }
                fn reset(&mut self) {
                    self.mixer.set_target(self.profiles[0]);
                    self.mixer.reset();
                    for w in &mut self.wet {
                        w.reset();
                    }
                    self.position = 0;
                    self.accepted.store(0, Ordering::Relaxed);
                    self.consumed.store(0, Ordering::Relaxed);
                }
                fn deactivate(&mut self) {
                    if self.config.take().is_some() {
                        self.deactivations.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            let layout = AudioLayout::new(3, 3).unwrap();
            let profiles: Vec<$g> = LAWS
                .iter()
                .flat_map(|&l| {
                    NORMS.iter().map(move |&n| {
                        ($convert)(
                            G::new(if n == N::UnityEndpoints { 0.13 } else { 0.87 }, l, n).unwrap(),
                        )
                    })
                })
                .collect();
            let input: [Vec<$t>; 3] = std::array::from_fn(|ch| {
                (0..1501)
                    .map(|i| {
                        ((i as f64 * (0.17 + ch as f64 * 0.037)).sin() * (0.7 - ch as f64 * 0.17)
                            + ((i * 13 % 23) as f64 - 11.) / 97.) as $t
                    })
                    .collect()
            });
            let impulses: [&[f64]; 3] = [
                &[0., 0., 0., 0.5, -0.25, 0.125],
                &[0., 0., 0., 0.7, 0.1],
                &[0., 0., 0., -0.1, 0.9, -0.2],
            ];
            let curve_at = |i: usize| {
                let p = profiles[i];
                curve(p.wet_fraction(), p.law(), p.normalization()).map(|v| v as $t)
            };
            let mut current = curve_at(0);
            let mut target = current;
            let mut start = current;
            let mut elapsed = 17;
            let mut expected: [Vec<f64>; 3] = [vec![], vec![], vec![]];
            for i in 0..1501 {
                let next = curve_at((i / 29) % profiles.len());
                if next != target {
                    start = current;
                    target = next;
                    elapsed = 0;
                }
                if elapsed < 17 {
                    elapsed += 1;
                    current = if elapsed == 17 {
                        target
                    } else {
                        std::array::from_fn(|ch| {
                            (start[ch] as f64
                                + (target[ch] as f64 - start[ch] as f64) * (elapsed as f64 / 17.))
                                as $t
                        })
                    };
                }
                for ch in 0..3 {
                    let dry = if i >= 3 { input[ch][i - 3] } else { 0. };
                    let wet = impulses[ch]
                        .iter()
                        .enumerate()
                        .filter(|(k, _)| *k <= i)
                        .map(|(k, &h)| h * input[ch][i - k] as f64)
                        .sum::<f64>() as $t;
                    expected[ch].push((dry * current[0] + wet * current[1]) as f64);
                }
            }
            let accepted = Arc::new(AtomicUsize::new(0));
            let consumed = Arc::new(AtomicUsize::new(0));
            let deactivations = Arc::new(AtomicUsize::new(0));
            let mut c = config(3, 71, I::Integer);
            c.maximum_frames = 67;
            let mut mixer = <$m>::new(c, profiles[0]).unwrap();
            mixer.set_wet_latency_samples(3.).unwrap();
            let node = GraphNode {
                id: NodeId(37),
                layout,
                processor: Box::new(Consumer {
                    profiles,
                    mixer,
                    wet: [
                        FirConvolver::new(impulses[0]).unwrap(),
                        FirConvolver::new(impulses[1]).unwrap(),
                        FirConvolver::new(impulses[2]).unwrap(),
                    ],
                    layout,
                    config: None,
                    position: 0,
                    accepted: accepted.clone(),
                    consumed: consumed.clone(),
                    deactivations: deactivations.clone(),
                }),
            };
            let routes: Vec<_> = (0..3)
                .flat_map(|channel| {
                    [
                        Connection {
                            source: Source::Input(channel),
                            destination: Destination::Node {
                                id: NodeId(37),
                                channel,
                            },
                            gain: 1.,
                        },
                        Connection {
                            source: Source::Node {
                                id: NodeId(37),
                                channel,
                            },
                            destination: Destination::Output(channel),
                            gain: 1.,
                        },
                    ]
                })
                .collect();
            let mut graph = Graph::new(layout, vec![node], &routes).unwrap();
            for cycle in 0..2 {
                graph
                    .prepare(ProcessConfig::new(48000., 67, 0, layout).unwrap())
                    .unwrap();
                for repeat in 0..2 {
                    let mut bad = [[99. as $t; 69]; 3];
                    let [a, b, c] = &mut bad;
                    let mut out = [a.as_mut_slice(), b.as_mut_slice(), c.as_mut_slice()];
                    let inputs = [&input[0][..68], &input[1][..68], &input[2][..68]];
                    let audio = AudioBuffer::separate(&inputs, &mut out, 68).unwrap();
                    assert!(matches!(
                        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        GraphStatus::Fault { node: None, .. }
                    ));
                    assert_eq!(accepted.load(Ordering::Relaxed), 0);
                    assert_eq!(consumed.load(Ordering::Relaxed), 0);
                    assert!(
                        bad.iter()
                            .all(|ch| ch[..68].iter().all(|&v| v == 0.) && ch[68] == 99.)
                    );
                    let mut start = 0;
                    let mut turn = 0;
                    while start < 1501 {
                        let count = [1, 67, 2, 0, 31, 5][turn % 6].min(1501 - start);
                        let mut y = [[99. as $t; 69]; 3];
                        let measured = allocation_counter::measure(|| {
                            let inputs = [
                                &input[0][start..start + count],
                                &input[1][start..start + count],
                                &input[2][start..start + count],
                            ];
                            let [a, b, c] = &mut y;
                            let mut out = [a.as_mut_slice(), b.as_mut_slice(), c.as_mut_slice()];
                            let audio = AudioBuffer::separate(&inputs, &mut out, count).unwrap();
                            assert_eq!(
                                graph.process(&mut ProcessBlock::new(
                                    audio,
                                    &[],
                                    Transport::default()
                                )),
                                GraphStatus::Continue
                            );
                        });
                        assert_eq!(measured.count_total, 0);
                        assert_eq!(measured.count_current, 0);
                        for ch in 0..3 {
                            for i in 0..count {
                                close(y[ch][i] as f64, expected[ch][start + i], $tol);
                            }
                            assert!(y[ch][count..].iter().all(|&v| v == 99.));
                        }
                        start += count;
                        turn += 1;
                        assert_eq!(accepted.load(Ordering::Relaxed), start);
                        assert_eq!(consumed.load(Ordering::Relaxed), start);
                    }
                    if repeat == 0 {
                        graph.reset();
                    }
                }
                graph.deactivate();
                assert_eq!(deactivations.load(Ordering::Relaxed), cycle + 1);
            }
        }
    };
}
graph!(
    actual_native64_multichannel_graph_with_independently_batched_dry_and_latency_fir,
    f64,
    G,
    M,
    |g: G| g,
    4e-15
);
graph!(
    actual_native32_multichannel_graph_with_explicit_f64_latency_fir_bridge,
    f32,
    ravet_signal::DryWetGains32,
    M32,
    |g: G| g.quantize_f32(),
    4e-7
);
macro_rules! retap {
    ($name:ident,$t:ty,$m:ty,$convert:expr,$tol:expr) => {
        #[test]
        fn $name() {
            let g = ($convert)(G::new(0., L::Linear, N::UnityEndpoints).unwrap());
            for policy in POLICIES {
                let mut m = <$m>::new(config(2, 3, policy), g).unwrap();
                let mut source: [Vec<f64>; 2] = [vec![], vec![]];
                let mut previous = [[0.; 2]; 2];
                for i in 0..1024 {
                    let d = [0., 0.25, 1.5, 2., 0.75, 17., 3.25][(i / 13) % 7];
                    m.set_wet_latency_samples(d).unwrap();
                    let samples = std::array::from_fn::<$t, 2, _>(|ch| {
                        ((i as f64 * (0.173 + ch as f64 * 0.037)).sin() * 0.41) as $t
                    });
                    for ch in 0..2 {
                        source[ch].push(samples[ch] as f64);
                    }
                    m.push_dry(&[&samples[..1], &samples[1..]], 1).unwrap();
                    let mut y = [[99. as $t; 1]; 2];
                    let [a, b] = &mut y;
                    m.mix_wet(&[&[0.], &[0.]], &mut [a, b], 1).unwrap();
                    for ch in 0..2 {
                        let actual_d = d.floor() + m.native_fraction() as f64;
                        let expected = if policy == I::Thiran1st {
                            let n = d.floor() as usize;
                            let v = if i >= n { source[ch][i - n] } else { 0. };
                            let a = m.native_thiran_coefficient() as f64;
                            let value = if actual_d.fract() == 0. {
                                v
                            } else {
                                a * v + previous[ch][0] - a * previous[ch][1]
                            };
                            previous[ch] = [v, value];
                            value
                        } else {
                            delayed(&source[ch], i, actual_d, policy, &mut previous[ch])
                        };
                        close(y[ch][0] as f64, expected, $tol);
                    }
                }
            }
            for l in LAWS {
                for n in NORMS {
                    let mut c = config(1, 3, I::Integer);
                    c.ramp_frames = 2;
                    let initial = ($convert)(G::new(0., l, n).unwrap());
                    let target = ($convert)(G::new(1., l, n).unwrap());
                    let mut m = <$m>::new(c, initial).unwrap();
                    m.set_target(target);
                    for frame in 1..=2 {
                        m.push_dry(&[&[0.71]], 1).unwrap();
                        let mut y = [0. as $t];
                        m.mix_wet(&[&[-0.39]], &mut [&mut y], 1).unwrap();
                        let weights = if frame == 2 {
                            target.gains()
                        } else {
                            std::array::from_fn(|ch| {
                                (initial.gains()[ch] as f64
                                    + (target.gains()[ch] as f64 - initial.gains()[ch] as f64)
                                        * 0.5) as $t
                            })
                        };
                        assert_eq!(y, [0.71 as $t * weights[0] + (-0.39 as $t) * weights[1]]);
                    }
                }
            }
        }
    };
}
retap!(
    native64_changed_latency_preserves_histories_and_full_law_consumption,
    f64,
    M,
    |g: G| g,
    1e-14
);
retap!(
    native32_changed_latency_preserves_native_histories_and_full_law_consumption,
    f32,
    M32,
    |g: G| g.quantize_f32(),
    8e-7
);
