use ravet_signal::{
    PanningError as E, PanningGains as G, PanningLaw as L, PanningNormalization as N,
    StereoPanner as P, StereoPanner32 as P32,
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
fn close(a: f64, b: f64, e: f64) {
    assert!((a - b).abs() <= e, "{a} != {b}, {} > {e}", (a - b).abs());
}
// Independent signed u coordinate and direct left/right expressions (production
// uses abs(p), a reflected near/far pair and an explicit structural center).
fn oracle(p: f64, l: L, n: N) -> [f64; 2] {
    let u = (1. + p) * 0.5;
    let v = [1. - u, u];
    let mut g = match l {
        L::Linear => v,
        L::Balanced => v.map(|x| (2. * x).min(1.)),
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
        L::SquareRoot => v.map(f64::sqrt),
        L::ThreeQuarterPower => v.map(|x| x.powf(0.75)),
    };
    if n == N::UnityCenter {
        let c = match l {
            L::Linear | L::SineSquared => 0.5,
            L::Balanced => 1.,
            L::Sine | L::SquareRoot => 1. / 2_f64.sqrt(),
            _ => 1. / 8_f64.sqrt().sqrt(),
        };
        g = g.map(|x| x / c);
    }
    g
}
#[test]
fn full_law_normalization_and_precision_sweeps_match_independent_curves() {
    for l in LAWS {
        for n in NORMS {
            let center = G::new(0., l, n).unwrap();
            let q = center.quantize_f32();
            let c = if n == N::UnityCenter {
                1.
            } else {
                l.center_gain()
            };
            assert_eq!(center.gains(), [c; 2]);
            assert_eq!(q.gains(), [c as f32; 2]);
            let end = center.endpoint_gain();
            assert_eq!(G::new(-1., l, n).unwrap().gains(), [end, 0.]);
            assert_eq!(G::new(1., l, n).unwrap().gains(), [0., end]);
            for i in 0..=10000 {
                let pan = i as f64 / 5000. - 1.;
                let g = G::new(pan, l, n).unwrap();
                let q = g.quantize_f32();
                let mirror = G::new(-pan, l, n).unwrap();
                let qm = mirror.quantize_f32();
                assert_eq!(g.pan(), pan);
                assert_eq!(q.pan(), pan);
                assert_eq!(g.law(), l);
                assert_eq!(q.law(), l);
                assert_eq!(g.normalization(), n);
                assert_eq!(q.normalization(), n);
                assert_eq!(g.endpoint_gain(), q.endpoint_gain());
                assert_eq!(g.left(), mirror.right());
                assert_eq!(g.right(), mirror.left());
                assert_eq!(q.left(), qm.right());
                assert_eq!(q.right(), qm.left());
                for ch in 0..2 {
                    close(g.gains()[ch], oracle(pan, l, n)[ch], 2e-14);
                    close(f64::from(q.gains()[ch]), oracle(pan, l, n)[ch], 1.3e-7);
                    assert_eq!(q.gains()[ch], g.gains()[ch] as f32);
                    assert!((0.0..=end).contains(&g.gains()[ch]));
                }
                let power = g.left().powi(2) + g.right().powi(2);
                if matches!(l, L::Sine | L::SquareRoot) {
                    close(
                        power,
                        if n == N::UnityEndpoints { 1. } else { 2. },
                        8. * f64::EPSILON,
                    );
                }
            }
        }
    }
    // Distinguish theoretical conventional rounded labels from actual centers.
    close(
        20. * L::Linear.center_gain().log10(),
        -6.020599913279624,
        1e-14,
    );
    close(
        20. * L::SineThreeHalves.center_gain().log10(),
        -4.515449934959718,
        1e-14,
    );
    close(
        20. * L::Sine.center_gain().log10(),
        -3.0102999566398116,
        1e-14,
    );
}
#[test]
fn coherent_uncorrelated_energy_normalization_headroom_and_balance_are_explicit() {
    for l in LAWS {
        for n in NORMS {
            let g = G::new(0., l, n).unwrap();
            let c = g.left();
            let mut p = P::new(g, 0, 4).unwrap();
            let coherent = p.process_mono_sample(1.);
            close(coherent.iter().sum(), 2. * c, 0.);
            // Orthogonal two-frame signals: center summed energy is 2*c², whereas
            // coherent fold-down energy is (2*c)². There is no universal loudness rule.
            let a = p.process_stereo_sample([1., 0.]);
            let b = p.process_stereo_sample([0., 1.]);
            close(a.iter().chain(&b).map(|x| x * x).sum(), 2. * c * c, 0.);
            close(coherent.iter().sum::<f64>().powi(2), 4. * c * c, 0.);
            let g = G::new(-1., l, n).unwrap();
            p.set_target(g);
            assert_eq!(
                p.process_stereo_sample([0.31, -0.77]),
                [0.31 * g.left(), -0.77 * g.right()]
            );
            if g.left() > 1. {
                assert!(p.process_mono_sample(f64::MAX)[0].is_infinite());
            }
        }
    }
    for l in [
        L::Linear,
        L::SineSquared,
        L::SineThreeHalves,
        L::ThreeQuarterPower,
        L::Balanced,
    ] {
        let center = G::new(0., l, N::UnityEndpoints).unwrap();
        let power = center.left().powi(2) + center.right().powi(2);
        assert!((power - 1.).abs() > 0.01);
    }
}
macro_rules! native {
    ($name:ident,$t:ty,$p:ty,$convert:expr) => {
        #[test]
        fn $name() {
            let make = |pan, l, n| ($convert)(G::new(pan, l, n).unwrap());
            for l in LAWS {
                for n in NORMS {
                    let left = make(-1., l, n);
                    let right = make(1., l, n);
                    for duration in [0, 1, 2, 7, 257, u32::MAX] {
                        let mut p = <$p>::new(left, duration, 257).unwrap();
                        p.set_target(right);
                        let count = if duration == u32::MAX {
                            1024
                        } else {
                            duration.max(1) + 2
                        };
                        for i in 1..=count {
                            let actual = p.process_stereo_sample([0.73, -0.41]);
                            let frac = if duration == 0 {
                                1.
                            } else {
                                f64::from(i.min(duration)) / f64::from(duration)
                            };
                            let expected: [$t; 2] = std::array::from_fn(|ch| {
                                let gain = if duration == 0 || i >= duration {
                                    right.gains()[ch]
                                } else {
                                    (left.gains()[ch] as f64
                                        + (right.gains()[ch] as f64 - left.gains()[ch] as f64)
                                            * frac) as $t
                                };
                                [0.73 as $t, -0.41 as $t][ch] * gain
                            });
                            assert_eq!(actual, expected);
                            assert_eq!(p.remaining_frames(), duration.saturating_sub(i));
                            p.set_target(right);
                            assert_eq!(p.remaining_frames(), duration.saturating_sub(i));
                        }
                        p.reset();
                        assert_eq!(p.current_gains(), right.gains());
                        assert_eq!(p.remaining_frames(), 0);
                        assert_eq!(p.ramp_frames(), duration);
                        assert_eq!(p.maximum_frames(), 257);
                    }
                    let mut p = <$p>::new(left, 7, 257).unwrap();
                    p.set_target(right);
                    p.process_mono_sample(1.);
                    p.process_mono_sample(1.);
                    let start = p.current_gains();
                    let target = make(0.39, LAWS[(l as usize + 1) % 7], n);
                    p.set_target(target);
                    for i in 1..=7 {
                        let g = p.process_mono_sample(1.);
                        let expected = if i == 7 {
                            target.gains()
                        } else {
                            std::array::from_fn(|ch| {
                                (start[ch] as f64
                                    + (target.gains()[ch] as f64 - start[ch] as f64)
                                        * (i as f64 / 7.)) as $t
                            })
                        };
                        assert_eq!(g, expected);
                        assert_eq!(p.remaining_frames(), 7 - i);
                    }
                    // Identical actual endpoints with different metadata retain progress.
                    let a = make(-1., L::Linear, N::UnityEndpoints);
                    let b = make(1., L::Sine, N::UnityEndpoints);
                    let c = make(1., L::Balanced, N::UnityCenter);
                    p = <$p>::new(a, 11, 257).unwrap();
                    p.set_target(b);
                    p.process_mono_sample(1.);
                    let before = p.remaining_frames();
                    let current = p.current_gains();
                    p.set_target(c);
                    assert_eq!(p.remaining_frames(), before);
                    assert_eq!(p.current_gains(), current);
                    assert_eq!(p.target(), c);
                    // Intermediate linear gain ramp does not remain constant-power.
                    let mut s = <$p>::new(make(-1., L::Sine, N::UnityEndpoints), 2, 257).unwrap();
                    s.set_target(make(1., L::Sine, N::UnityEndpoints));
                    assert_eq!(s.process_mono_sample(1.), [0.5, 0.5]);
                }
            }
            for maximum in [0, 65537, usize::MAX] {
                assert!(matches!(
                    <$p>::new(make(0., L::Linear, N::UnityEndpoints), 0, maximum),
                    Err(E::InvalidMaximumFrames)
                ));
            }
            for maximum in [1, 65536] {
                assert!(
                    <$p>::new(make(0., L::Linear, N::UnityEndpoints), u32::MAX, maximum).is_ok()
                );
            }
        }
    };
}
native!(
    native64_exact_ramps_retarget_identical_coefficients_and_long_prefix,
    f64,
    P,
    |g: G| g
);
native!(
    native32_exact_ramps_retarget_identical_coefficients_and_long_prefix,
    f32,
    P32,
    |g: G| g.quantize_f32()
);
macro_rules! blocks {
    ($name:ident,$t:ty,$p:ty,$convert:expr) => {
        #[test]
        fn $name() {
            for l in LAWS {
                for n in NORMS {
                    let initial = ($convert)(G::new(-0.71, l, n).unwrap());
                    let target = ($convert)(G::new(0.87, l, n).unwrap());
                    let p = <$p>::new(initial, 131, 257).unwrap();
                    let mut scalar = p;
                    scalar.set_target(target);
                    let x: [Vec<$t>; 2] = std::array::from_fn(|ch| {
                        (0..257)
                            .map(|i| {
                                ((i as f64 * (0.13 + ch as f64 * 0.071)).sin()
                                    * (0.7 - ch as f64 * 0.4)) as $t
                            })
                            .collect()
                    });
                    let expected: Vec<_> = (0..257)
                        .map(|i| scalar.process_stereo_sample([x[0][i], x[1][i]]))
                        .collect();
                    for inplace in [false, true] {
                        for split in [false, true] {
                            let mut p = p;
                            p.set_target(target);
                            let mut start = 0;
                            let mut turn = 0;
                            while start < 257 {
                                let frames = if split {
                                    [1, 7, 0, 19, 2, 61][turn % 6].min(257 - start)
                                } else {
                                    257
                                };
                                let mut y = vec![vec![99. as $t; frames + 3]; 2];
                                if inplace {
                                    for ch in 0..2 {
                                        y[ch][..frames]
                                            .copy_from_slice(&x[ch][start..start + frames]);
                                    }
                                }
                                let before = p.remaining_frames();
                                let [a, b] = y.as_mut_slice() else {
                                    unreachable!()
                                };
                                let mut out = [a.as_mut_slice(), b.as_mut_slice()];
                                let measured = allocation_counter::measure(|| {
                                    if inplace {
                                        p.process_stereo_in_place(&mut out, frames).unwrap();
                                    } else {
                                        p.process_stereo(
                                            &[
                                                &x[0][start..start + frames],
                                                &x[1][start..start + frames],
                                            ],
                                            &mut out,
                                            frames,
                                        )
                                        .unwrap();
                                    }
                                });
                                assert_eq!(measured.count_total, 0);
                                assert_eq!(measured.count_current, 0);
                                for ch in 0..2 {
                                    for i in 0..frames {
                                        assert_eq!(y[ch][i], expected[start + i][ch]);
                                    }
                                    assert!(y[ch][frames..].iter().all(|&v| v == 99.));
                                }
                                assert_eq!(
                                    p.remaining_frames(),
                                    before.saturating_sub(frames as u32)
                                );
                                start += frames;
                                turn += 1;
                            }
                            assert_eq!(p.current_gains(), scalar.current_gains());
                        }
                    }
                    let mut mono = p;
                    mono.set_target(target);
                    let mut reference = mono;
                    let mut y = [[99. as $t; 260]; 2];
                    let [a, b] = &mut y;
                    mono.process_mono(&x[0], &mut [a, b], 257).unwrap();
                    for i in 0..257 {
                        let expected = reference.process_mono_sample(x[0][i]);
                        for ch in 0..2 {
                            assert_eq!(y[ch][i], expected[ch]);
                        }
                    }
                    assert!(y.iter().all(|ch| ch[257..].iter().all(|&v| v == 99.)));
                    // Late, right-hand faults must preserve output and complete ramp state.
                    let mut p = p;
                    p.set_target(target);
                    p.process_mono_sample(0.5);
                    let mut saved = p;
                    let mut y = [[99. as $t; 260]; 2];
                    {
                        let [a, b] = &mut y;
                        assert_eq!(
                            p.process_stereo(&[&x[0], &x[1][..3]], &mut [a, b], 4),
                            Err(E::BufferTooShort)
                        );
                    }
                    {
                        let [a, b] = &mut y;
                        assert_eq!(
                            p.process_stereo(&[&x[0], &x[1]], &mut [a, &mut b[..3]], 4),
                            Err(E::BufferTooShort)
                        );
                    }
                    {
                        let [a, b] = &mut y;
                        assert_eq!(
                            p.process_mono(&x[0][..3], &mut [a, b], 4),
                            Err(E::BufferTooShort)
                        );
                    }
                    {
                        let [a, b] = &mut y;
                        assert_eq!(
                            p.process_mono(&x[0], &mut [a, &mut b[..3]], 4),
                            Err(E::BufferTooShort)
                        );
                    }
                    {
                        let [a, b] = &mut y;
                        assert_eq!(
                            p.process_stereo_in_place(&mut [a, &mut b[..3]], 4),
                            Err(E::BufferTooShort)
                        );
                    }
                    {
                        let [a, b] = &mut y;
                        assert_eq!(
                            p.process_stereo(&[&x[0]], &mut [a, b], 1),
                            Err(E::ChannelCount)
                        );
                    }
                    {
                        let [a, b] = &mut y;
                        assert_eq!(
                            p.process_stereo(&[&x[0], &x[1]], &mut [a, b], 258),
                            Err(E::TooManyFrames)
                        );
                    }
                    {
                        let [a, b] = &mut y;
                        assert_eq!(
                            p.process_mono(&x[0], &mut [a, b], 258),
                            Err(E::TooManyFrames)
                        );
                    }
                    {
                        let [a, b] = &mut y;
                        assert_eq!(
                            p.process_stereo_in_place(&mut [a, b], 258),
                            Err(E::TooManyFrames)
                        );
                    }
                    for frames in [0, 1] {
                        let [a, _] = &mut y;
                        assert_eq!(
                            p.process_mono(&x[0], &mut [a], frames),
                            Err(E::ChannelCount)
                        );
                    }
                    assert!(y.iter().all(|ch| ch.iter().all(|&v| v == 99.)));
                    assert_eq!(p.current_gains(), saved.current_gains());
                    assert_eq!(p.remaining_frames(), saved.remaining_frames());
                    assert_eq!(p.target(), saved.target());
                    assert_eq!(
                        p.process_stereo_sample([0.71, -0.13]),
                        saved.process_stereo_sample([0.71, -0.13])
                    );
                    let before = p.remaining_frames();
                    p.process_stereo(&[&[], &[]], &mut [&mut [], &mut []], 0)
                        .unwrap();
                    p.process_mono(&[], &mut [&mut [], &mut []], 0).unwrap();
                    p.process_stereo_in_place(&mut [&mut [], &mut []], 0)
                        .unwrap();
                    assert_eq!(p.remaining_frames(), before);
                }
            }
            let g = ($convert)(G::new(-1., L::Linear, N::UnityEndpoints).unwrap());
            let mut p = <$p>::new(g, 0, 64).unwrap();
            let y = p.process_stereo_sample([<$t>::INFINITY, <$t>::INFINITY]);
            assert!(y[0].is_infinite() && y[1].is_nan());
            assert!(p.process_mono_sample(<$t>::NAN).iter().all(|v| v.is_nan()));
            assert_eq!(p.process_mono_sample(0.25), [0.25, 0.]); // no poisoned input history
            let boosted = ($convert)(G::new(-1., L::Linear, N::UnityCenter).unwrap());
            p.set_target(boosted);
            assert!(p.process_mono_sample(<$t>::MAX)[0].is_infinite());
            let measured = allocation_counter::measure(|| {
                for _ in 0..10000 {
                    let g = ($convert)(G::new(-0.39, L::SineThreeHalves, N::UnityCenter).unwrap());
                    let mut p = <$p>::new(g, 17, 64).unwrap();
                    p.set_target(boosted);
                    p.process_mono_sample(0.3);
                    p.reset();
                    assert!(G::new(f64::NAN, L::Linear, N::UnityEndpoints).is_err());
                    assert!(<$p>::new(g, 7, 0).is_err());
                    assert!(p.process_mono(&[], &mut [&mut [], &mut []], 1).is_err());
                }
            });
            assert_eq!(measured.count_total, 0);
            assert_eq!(measured.count_current, 0);
        }
    };
}
blocks!(
    native64_prefix_partition_late_rejections_and_nonfinite_heap_policy,
    f64,
    P,
    |g: G| g
);
blocks!(
    native32_prefix_partition_late_rejections_and_nonfinite_heap_policy,
    f32,
    P32,
    |g: G| g.quantize_f32()
);
#[test]
fn invalid_controls_extreme_valid_pan_and_requested_vs_rounded_metadata() {
    for l in LAWS {
        for n in NORMS {
            for invalid in [
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                -1.00000001,
                1.00000001,
                f64::MAX,
            ] {
                assert_eq!(G::new(invalid, l, n), Err(E::InvalidPan));
            }
            for p in [
                -1.,
                f64::from_bits(1_f64.to_bits() - 1),
                -f64::from_bits(1_f64.to_bits() - 1),
                f64::MIN_POSITIVE,
                f64::from_bits(1),
                -0.,
                0.,
                1.,
            ] {
                let g = G::new(p, l, n).unwrap();
                let q = g.quantize_f32();
                assert_eq!(q.pan(), p);
                assert!(q.gains().iter().all(|v| v.is_finite()));
                let mirror = G::new(-p, l, n).unwrap();
                assert_eq!(g.gains(), [mirror.right(), mirror.left()]);
            }
        }
    }
}
macro_rules! graph_consumer {
    ($name:ident,$t:ty,$profile:ty,$p:ty,$convert:expr,$tol:expr) => {
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
                profiles: Vec<$profile>,
                panner: $p,
                fir: [FirConvolver; 2],
                layout: AudioLayout,
                config: Option<ProcessConfig>,
                position: usize,
                frames: Arc<AtomicUsize>,
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
                    for i in 0..b.audio.frames() {
                        let index = (self.position / 29) % self.profiles.len();
                        self.panner.set_target(self.profiles[index]);
                        let left = b.audio.input(0).unwrap()[i];
                        let y = if self.layout.input_channels() == 1 {
                            self.panner.process_mono_sample(left)
                        } else {
                            self.panner
                                .process_stereo_sample([left, b.audio.input(1).unwrap()[i]])
                        };
                        for (ch, &v) in y.iter().enumerate() {
                            // Explicit consumer bridge for the existing f64-only FIR.
                            b.audio.output_mut(ch).unwrap()[i] =
                                self.fir[ch].process_sample(v as f64) as $t;
                        }
                        self.position += 1;
                    }
                    self.frames.fetch_add(b.audio.frames(), Ordering::Relaxed);
                    ProcessStatus::Continue
                }
                fn reset(&mut self) {
                    self.panner = <$p>::new(self.profiles[0], 17, 67).unwrap();
                    for f in &mut self.fir {
                        f.reset();
                    }
                    self.position = 0;
                    self.frames.store(0, Ordering::Relaxed);
                }
                fn deactivate(&mut self) {
                    if self.config.take().is_some() {
                        self.deactivations.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            for mono in [true, false] {
                let layout = AudioLayout::new(if mono { 1 } else { 2 }, 2).unwrap();
                let profiles: Vec<$profile> = LAWS
                    .iter()
                    .flat_map(|&l| {
                        NORMS.iter().map(move |&n| {
                            ($convert)(
                                G::new(if n == N::UnityEndpoints { -0.77 } else { 0.83 }, l, n)
                                    .unwrap(),
                            )
                        })
                    })
                    .collect();
                let input: [Vec<$t>; 2] = std::array::from_fn(|ch| {
                    (0..1501)
                        .map(|i| {
                            ((i as f64 * (0.13 + ch as f64 * 0.051)).sin()
                                * (0.7 - ch as f64 * 0.3)
                                + ((i * 13 % 23) as f64 - 11.) / 89.) as $t
                        })
                        .collect()
                });
                let impulses: [&[f64]; 2] = [&[0.5, -0.25, 0.1, 0.2], &[0.1, 0.7, -0.13]];
                // Independent gain interpolator with signed-u mathematical curve,
                // explicit actual native rounding and finite convolution sums.
                let curve = |index: usize| {
                    let p = profiles[index];
                    let g = oracle(p.pan(), p.law(), p.normalization());
                    g.map(|v| v as $t)
                };
                let mut current = curve(0);
                let mut target = current;
                let mut start = current;
                let mut elapsed = 17;
                let mut selected: [Vec<f64>; 2] = [vec![], vec![]];
                for i in 0..1501 {
                    let next = curve((i / 29) % profiles.len());
                    if target != next {
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
                                    + (target[ch] as f64 - start[ch] as f64)
                                        * (elapsed as f64 / 17.)) as $t
                            })
                        };
                    }
                    for ch in 0..2 {
                        selected[ch]
                            .push((input[if mono { 0 } else { ch }][i] * current[ch]) as f64);
                    }
                }
                let expected: [Vec<f64>; 2] = std::array::from_fn(|ch| {
                    (0..1501)
                        .map(|i| {
                            impulses[ch]
                                .iter()
                                .enumerate()
                                .filter(|(k, _)| *k <= i)
                                .map(|(k, &h)| h * selected[ch][i - k])
                                .sum()
                        })
                        .collect()
                });
                let frames = Arc::new(AtomicUsize::new(0));
                let deactivations = Arc::new(AtomicUsize::new(0));
                let node = GraphNode {
                    id: NodeId(91),
                    layout,
                    processor: Box::new(Consumer {
                        panner: <$p>::new(profiles[0], 17, 67).unwrap(),
                        profiles,
                        fir: [
                            FirConvolver::new(impulses[0]).unwrap(),
                            FirConvolver::new(impulses[1]).unwrap(),
                        ],
                        layout,
                        config: None,
                        position: 0,
                        frames: frames.clone(),
                        deactivations: deactivations.clone(),
                    }),
                };
                let mut routes: Vec<_> = (0..layout.input_channels())
                    .map(|channel| Connection {
                        source: Source::Input(channel),
                        destination: Destination::Node {
                            id: NodeId(91),
                            channel,
                        },
                        gain: 1.,
                    })
                    .collect();
                routes.extend((0..2).map(|channel| Connection {
                    source: Source::Node {
                        id: NodeId(91),
                        channel,
                    },
                    destination: Destination::Output(channel),
                    gain: 1.,
                }));
                let mut graph = Graph::new(layout, vec![node], &routes).unwrap();
                for cycle in 0..2 {
                    graph
                        .prepare(ProcessConfig::new(48000., 67, 0, layout).unwrap())
                        .unwrap();
                    for repeat in 0..2 {
                        let mut bad = [[99. as $t; 69]; 2];
                        let [a, b] = &mut bad;
                        let mut output = [a.as_mut_slice(), b.as_mut_slice()];
                        let all = [&input[0][..68], &input[1][..68]];
                        let inputs = &all[..layout.input_channels()];
                        let audio = AudioBuffer::separate(inputs, &mut output, 68).unwrap();
                        assert!(matches!(
                            graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                            GraphStatus::Fault { node: None, .. }
                        ));
                        assert_eq!(frames.load(Ordering::Relaxed), 0);
                        assert!(
                            bad.iter()
                                .all(|c| c[..68].iter().all(|&v| v == 0.) && c[68] == 99.)
                        );
                        let mut start = 0;
                        let mut turn = 0;
                        while start < 1501 {
                            let count = [1, 67, 0, 2, 31, 5][turn % 6].min(1501 - start);
                            let mut y = [[99. as $t; 69]; 2];
                            let measured = allocation_counter::measure(|| {
                                let all = [
                                    &input[0][start..start + count],
                                    &input[1][start..start + count],
                                ];
                                let [a, b] = &mut y;
                                let mut output = [a.as_mut_slice(), b.as_mut_slice()];
                                let audio = AudioBuffer::separate(
                                    &all[..layout.input_channels()],
                                    &mut output,
                                    count,
                                )
                                .unwrap();
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
                            for ch in 0..2 {
                                for i in 0..count {
                                    close(y[ch][i] as f64, expected[ch][start + i], $tol);
                                }
                                assert!(y[ch][count..].iter().all(|&v| v == 99.));
                            }
                            start += count;
                            turn += 1;
                            assert_eq!(frames.load(Ordering::Relaxed), start);
                        }
                        if repeat == 0 {
                            graph.reset();
                        }
                    }
                    graph.deactivate();
                    assert_eq!(deactivations.load(Ordering::Relaxed), cycle + 1);
                }
            }
        }
    };
}
graph_consumer!(
    actual_native64_mono_and_stereo_graph_fir_consumers,
    f64,
    G,
    P,
    |g: G| g,
    4e-15
);
graph_consumer!(
    actual_native32_mono_and_stereo_graph_with_explicit_f64_fir_bridge,
    f32,
    ravet_signal::PanningGains32,
    P32,
    |g: G| g.quantize_f32(),
    3e-7
);
