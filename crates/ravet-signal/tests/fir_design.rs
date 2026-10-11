use ravet_signal::{
    FirBand as B, FirConvolver, FirDesign as D, FirDesignError as E, FirResponse as R,
    FirWindow as W, WindowConvention, WindowKind as K, WindowPlan,
};
use std::f64::consts::{PI, TAU};
fn close(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() <= t, "{a} != {b}, delta {}", (a - b).abs());
}
fn integral(mut f: impl FnMut(f64) -> f64, lo: f64, hi: f64, n: usize) -> f64 {
    let step = (hi - lo) / n as f64;
    let mut sum = f(lo) + f(hi);
    for j in 1..n {
        sum += f(lo + j as f64 * step) * if j % 2 == 0 { 2. } else { 4. };
    }
    sum * step / 3.
}
// I0 integral, independent of production power-series evaluation.
fn bessel(x: f64) -> f64 {
    integral(|t| (x * t.cos()).exp(), 0., PI, 4096) / PI
}
fn response(h: &[f64], f: f64) -> (f64, f64) {
    h.iter().enumerate().fold((0., 0.), |(r, i), (j, &h)| {
        (
            r + h * (TAU * f * j as f64).cos(),
            i - h * (TAU * f * j as f64).sin(),
        )
    })
}
fn ideal(response: R, rate: f64, t: f64) -> f64 {
    let edge = |a: f64, b: f64| {
        if t == 0. {
            2. * (b - a)
        } else {
            ((TAU * b * t).sin() - (TAU * a * t).sin()) / (PI * t)
        }
    };
    match response {
        R::LowPass { cutoff_hz } => edge(0., cutoff_hz / rate),
        R::HighPass { cutoff_hz } => edge(cutoff_hz / rate, 0.5),
        R::BandPass { low_hz, high_hz } => edge(low_hz / rate, high_hz / rate),
        R::BandStop { low_hz, high_hz } => edge(0., low_hz / rate) + edge(high_hz / rate, 0.5),
    }
}
fn window_oracle(response: R, rate: f64, n: usize, window: W) -> Vec<f64> {
    let w = match window {
        W::Cosine(k) => WindowPlan::new(k, WindowConvention::Symmetric, n)
            .unwrap()
            .coefficients()
            .to_vec(),
        W::Kaiser { beta } => {
            let normal = bessel(beta);
            (0..n)
                .map(|j| {
                    let x = (j as f64 - (n - 1) as f64 / 2.) / ((n - 1) as f64 / 2.);
                    bessel(beta * (1. - x * x).sqrt()) / normal
                })
                .collect()
        }
    };
    let middle = (n - 1) as f64 / 2.;
    let mut h: Vec<_> = w
        .iter()
        .enumerate()
        .map(|(j, &w)| w * ideal(response, rate, j as f64 - middle))
        .collect();
    let reference = match response {
        R::LowPass { .. } | R::BandStop { .. } => 0.,
        R::HighPass { .. } => 0.5,
        R::BandPass { low_hz, high_hz } => (low_hz + high_hz) / (2. * rate),
    };
    let gain: f64 = h
        .iter()
        .enumerate()
        .map(|(j, &h)| h * (TAU * reference * (j as f64 - middle)).cos())
        .sum();
    for tap in &mut h {
        *tap /= gain;
    }
    h
}
#[test]
fn windowed_all_responses_parities_and_windows_match_independent_band_integrals() {
    for rate in [8000., 44100., 192000.] {
        for response in [
            R::LowPass {
                cutoff_hz: rate * 0.14,
            },
            R::HighPass {
                cutoff_hz: rate * 0.17,
            },
            R::BandPass {
                low_hz: rate * 0.10,
                high_hz: rate * 0.27,
            },
            R::BandStop {
                low_hz: rate * 0.10,
                high_hz: rate * 0.27,
            },
        ] {
            for n in [17, 18, 63, 64] {
                if n % 2 == 0 && matches!(response, R::HighPass { .. } | R::BandStop { .. }) {
                    continue;
                }
                for kind in [
                    K::Rectangular,
                    K::Hann,
                    K::Hamming,
                    K::Blackman,
                    K::BlackmanHarris,
                ] {
                    let d = D::windowed(response, rate, n, W::Cosine(kind)).unwrap();
                    let expected = window_oracle(response, rate, n, W::Cosine(kind));
                    assert_eq!(d.order(), n - 1);
                    assert_eq!(d.group_delay_samples(), (n - 1) as f64 / 2.);
                    assert_eq!(d.tail_samples(), n - 1);
                    for (j, &expected_value) in expected.iter().enumerate() {
                        assert_eq!(d.coefficients()[j], d.coefficients()[n - 1 - j]);
                        close(d.coefficients()[j], expected_value, 4e-15);
                    }
                    for j in 0..=128 {
                        let f = j as f64 / 256.;
                        let (r, i) = response_fn(d.coefficients(), f);
                        close(d.magnitude_at(f).unwrap(), r.hypot(i), 3e-14);
                    }
                    if n % 2 == 0 {
                        close(d.magnitude_at(0.5).unwrap(), 0., 1e-14);
                    }
                }
            }
        }
    }
}
fn response_fn(h: &[f64], f: f64) -> (f64, f64) {
    response(h, f)
}
#[test]
fn kaiser_beta_endpoints_and_empirical_sizing_are_independently_measured() {
    close(bessel(2.), 2.279585302336067, 2e-14);
    close(bessel(12.), 18948.92534929631, 2e-10);
    let r = R::LowPass { cutoff_hz: 175. };
    for beta in [0., 2., 6.20426, 12., 32.] {
        let d = D::windowed(r, 1000., 31, W::Kaiser { beta }).unwrap();
        let expected = window_oracle(r, 1000., 31, W::Kaiser { beta });
        for (&a, &b) in d.coefficients().iter().zip(&expected) {
            close(a, b, 2e-15);
        }
    }
    let rectangular = D::windowed(r, 1000., 31, W::Cosine(K::Rectangular)).unwrap();
    let zero = D::windowed(r, 1000., 31, W::Kaiser { beta: 0. }).unwrap();
    assert_eq!(rectangular.coefficients(), zero.coefficients());
    let sized = D::kaiser_lowpass(1000., 163., 187., 65.).unwrap();
    assert_eq!(sized.estimated_order(), 166);
    assert_eq!(sized.design().order(), 166);
    close(sized.beta(), 6.20426, 1e-14);
    let mut stop: f64 = 0.;
    let mut pass: f64 = 0.;
    for j in 0..=32000 {
        let f = j as f64 / 64000.;
        let amplitude = sized.design().magnitude_at(f).unwrap();
        if f <= 0.163 {
            pass = pass.max((amplitude - 1.).abs());
        }
        if f >= 0.187 {
            stop = stop.max(amplitude);
        }
    }
    // Primary empirical example misses65dB at its first stop lobe. Actual DC
    // normalization is explicit; this test forbids promoting sizing into bounds.
    eprintln!(
        "empirical Kaiser65dB: actual pass deviation={pass:.12e}, stop max={stop:.12e}, stop attenuation={:.9}dB",
        -20. * stop.log10()
    );
    assert!(stop > 10f64.powf(-65. / 20.));
    assert!(stop < 0.00065);
    assert!(pass < 0.00065);
    let promoted = D::kaiser_lowpass(48000., 8000., 10000., 60.).unwrap();
    assert_eq!(promoted.design().order() % 2, 0);
    assert!(promoted.design().order() >= promoted.estimated_order());
}
fn factorial(n: usize) -> f64 {
    (1..=n).product::<usize>() as f64
}
fn choose(n: usize, j: usize) -> f64 {
    factorial(n) / (factorial(j) * factorial(n - j))
}
fn target(f: f64, cutoff: f64, width: f64, k: usize) -> f64 {
    if f <= cutoff - width / 2. {
        return 1.;
    }
    if f >= cutoff + width / 2. {
        return 0.;
    }
    let z = (f - cutoff + width / 2.) / (width / k as f64);
    1.-(0..=k).map(|j|if j%2==0{1.}else{-1.}*choose(k,j)*(z-j as f64).max(0.).powi(k as i32)).sum::<f64>()/factorial(k)
}
#[test]
fn degree_one_through_four_transition_matches_independent_frequency_quadrature() {
    for k in 1..=4 {
        for n in [17, 18, 63, 64] {
            let cutoff = 0.17;
            let width = 0.12;
            let d = D::transition_lowpass(48000., cutoff * 48000., width * 48000., n, k).unwrap();
            let middle = (n - 1) as f64 / 2.;
            let mut expected: Vec<_> = (0..n)
                .map(|j| {
                    2. * integral(
                        |f| target(f, cutoff, width, k) * (TAU * f * (j as f64 - middle)).cos(),
                        0.,
                        cutoff - width / 2.,
                        2048,
                    ) + 2.
                        * integral(
                            |f| target(f, cutoff, width, k) * (TAU * f * (j as f64 - middle)).cos(),
                            cutoff - width / 2.,
                            cutoff + width / 2.,
                            4096,
                        )
                })
                .collect();
            let sum: f64 = expected.iter().sum();
            for tap in &mut expected {
                *tap /= sum;
            }
            for (&a, &b) in d.coefficients().iter().zip(&expected) {
                close(a, b, 3e-11);
            }
            close(d.magnitude_at(0.).unwrap(), 1., 2e-15);
        }
    }
}
fn centered(d: &D, f: f64) -> f64 {
    d.coefficients()
        .iter()
        .enumerate()
        .map(|(j, &h)| h * (TAU * f * (j as f64 - d.group_delay_samples())).cos())
        .sum()
}
#[test]
fn least_squares_satisfies_independent_quadrature_normal_equations_and_weighted_objective() {
    for (taps, bands) in [
        (
            3,
            vec![B {
                low_hz: 0.,
                high_hz: 24000.,
                low_gain: 1.,
                high_gain: 1.,
                weight: 1.,
            }],
        ),
        (
            17,
            vec![
                B {
                    low_hz: 0.,
                    high_hz: 6400.,
                    low_gain: 1.,
                    high_gain: 0.85,
                    weight: 1.,
                },
                B {
                    low_hz: 10000.,
                    high_hz: 24000.,
                    low_gain: 0.,
                    high_gain: 0.,
                    weight: 10.,
                },
            ],
        ),
        (
            31,
            vec![
                B {
                    low_hz: 0.,
                    high_hz: 3000.,
                    low_gain: 0.,
                    high_gain: 0.,
                    weight: 30.,
                },
                B {
                    low_hz: 5000.,
                    high_hz: 11000.,
                    low_gain: 0.8,
                    high_gain: 1.2,
                    weight: 2.,
                },
                B {
                    low_hz: 14000.,
                    high_hz: 24000.,
                    low_gain: 0.,
                    high_gain: 0.,
                    weight: 30.,
                },
            ],
        ),
    ] {
        let d = D::least_squares(48000., taps, &bands).unwrap();
        let maximum = bands.iter().map(|b| b.weight).fold(0., f64::max);
        for k in 0..taps.div_ceil(2) {
            let residual: f64 = bands
                .iter()
                .map(|b| {
                    let lo = b.low_hz / 48000.;
                    let hi = b.high_hz / 48000.;
                    integral(
                        |f| {
                            let desired =
                                b.low_gain + (b.high_gain - b.low_gain) * (f - lo) / (hi - lo);
                            (centered(&d, f) - desired) * (TAU * k as f64 * f).cos() * b.weight
                                / maximum
                        },
                        lo,
                        hi,
                        8192,
                    )
                })
                .sum();
            close(residual, 0., 2e-11);
        }
        assert!(d.coefficients().iter().all(|v| v.is_finite()));
        for j in 0..taps {
            assert_eq!(d.coefficients()[j], d.coefficients()[taps - 1 - j]);
        }
        let baseline = D::windowed(
            R::LowPass { cutoff_hz: 8200. },
            48000.,
            taps,
            W::Cosine(K::Hamming),
        )
        .unwrap();
        let objective = |design: &D| {
            bands
                .iter()
                .map(|b| {
                    let lo = b.low_hz / 48000.;
                    let hi = b.high_hz / 48000.;
                    integral(
                        |f| {
                            let desired =
                                b.low_gain + (b.high_gain - b.low_gain) * (f - lo) / (hi - lo);
                            (centered(design, f) - desired).powi(2) * b.weight
                        },
                        lo,
                        hi,
                        8192,
                    )
                })
                .sum::<f64>()
        };
        assert!(objective(&d) < objective(&baseline));
        if taps == 3 {
            close(d.coefficients()[1], 1., 1e-15);
            close(d.coefficients()[0], 0., 1e-15);
        }
    }
    // Full-band cosine has an exact independent Fourier solution, including
    // negative gain. No normalization may erase the requested scale/sign.
    let d = D::least_squares(
        2.,
        7,
        &[B {
            low_hz: 0.,
            high_hz: 1.,
            low_gain: -2.,
            high_gain: -2.,
            weight: 1.,
        }],
    )
    .unwrap();
    close(d.coefficients()[3], -2., 2e-15);
    let zeros = D::least_squares(
        2.,
        7,
        &[B {
            low_hz: 0.,
            high_hz: 1.,
            low_gain: 0.,
            high_gain: 0.,
            weight: 1.,
        }],
    )
    .unwrap();
    assert!(zeros.coefficients().iter().all(|&h| h == 0.));
}
#[test]
fn halfband_exact_structure_complement_and_real_convolution_tail() {
    for taps in [3, 7, 31, 127] {
        for window in [W::Cosine(K::Hamming), W::Kaiser { beta: 8. }] {
            let d = D::halfband_windowed(taps, window).unwrap();
            let m = taps / 2;
            assert_eq!(d.coefficients()[m], 0.5);
            for j in 0..taps {
                assert_eq!(d.coefficients()[j], d.coefficients()[taps - 1 - j]);
                if j != m && j.abs_diff(m) % 2 == 0 {
                    assert_eq!(d.coefficients()[j], 0.);
                }
            }
            close(d.magnitude_at(0.).unwrap(), 1., 3e-15);
            close(d.magnitude_at(0.5).unwrap(), 0., 3e-15);
            for j in 0..=256 {
                let f = j as f64 / 512.;
                close(centered(&d, f) + centered(&d, 0.5 - f), 1., 2e-14);
            }
            let input: Vec<_> = (0..157)
                .map(|j| (j as f64 * 0.2).sin())
                .chain(std::iter::repeat_n(0., d.tail_samples()))
                .collect();
            let mut actual = input.clone();
            let mut filter = FirConvolver::new(d.coefficients()).unwrap();
            for block in actual.chunks_mut(13) {
                filter.process_in_place(block);
            }
            let expected: Vec<_> = (0..input.len())
                .map(|t| {
                    d.coefficients()
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j <= t)
                        .map(|(j, &h)| h * input[t - j])
                        .sum::<f64>()
                })
                .collect();
            for (&a, &b) in actual.iter().zip(&expected) {
                close(a, b, 1e-14);
            }
            filter.reset();
            assert_eq!(filter.process_sample(0.), 0.);
        }
    }
}

#[test]
fn all_validation_preflights_capacity_and_conditioning_failures_preserve_ownership() {
    let valid = R::LowPass { cutoff_hz: 5300. };
    let no_heap = allocation_counter::measure(|| {
        for n in [0, 1, 65537, usize::MAX] {
            assert_eq!(
                D::windowed(valid, 48000., n, W::Cosine(K::Hamming)).unwrap_err(),
                E::Capacity
            );
        }
        for n in [2, 4, 64] {
            for r in [
                R::HighPass { cutoff_hz: 5000. },
                R::BandStop {
                    low_hz: 2000.,
                    high_hz: 5000.,
                },
            ] {
                assert_eq!(
                    D::windowed(r, 48000., n, W::Cosine(K::Hamming)).unwrap_err(),
                    E::InvalidSpecification
                );
            }
        }
        for beta in [-1., 32.01, f64::NAN, f64::INFINITY] {
            assert_eq!(
                D::windowed(valid, 48000., 17, W::Kaiser { beta }).unwrap_err(),
                E::InvalidSpecification
            );
        }
        for rate in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(D::windowed(valid, rate, 17, W::Cosine(K::Hamming)).is_err());
        }
        for cutoff_hz in [0., -1., 24000., f64::NAN, f64::INFINITY] {
            assert!(
                D::windowed(R::LowPass { cutoff_hz }, 48000., 17, W::Cosine(K::Hamming)).is_err()
            );
        }
        for r in [
            R::BandPass {
                low_hz: 7000.,
                high_hz: 3000.,
            },
            R::BandStop {
                low_hz: 5000.,
                high_hz: 5000.,
            },
            R::BandStop {
                low_hz: f64::NAN,
                high_hz: 9000.,
            },
        ] {
            assert!(D::windowed(r, 48000., 17, W::Cosine(K::Hamming)).is_err());
        }
        for (p, s, a) in [
            (0., 2000., 60.),
            (2000., 1000., 60.),
            (1000., 2000., 7.99),
            (1000., 2000., 120.01),
            (1000., 2000., f64::NAN),
            (1000., 1000.001, 120.),
        ] {
            assert!(D::kaiser_lowpass(48000., p, s, a).is_err());
        }
        for (cutoff, width, k) in [
            (2000., 0., 1),
            (2000., 4000., 1),
            (23000., 2000., 1),
            (2000., 1000., 0),
            (2000., 1000., 5),
            (2000., f64::NAN, 1),
        ] {
            assert!(D::transition_lowpass(48000., cutoff, width, 17, k).is_err());
        }
        for n in [0, 1, 4, 5, 65536, usize::MAX] {
            assert_eq!(
                D::halfband_windowed(n, W::Cosine(K::Hamming)).unwrap_err(),
                E::Capacity
            );
        }
        let base = B {
            low_hz: 0.,
            high_hz: 24000.,
            low_gain: 1.,
            high_gain: 1.,
            weight: 1.,
        };
        for n in [0, 1, 2, 512, 513, usize::MAX] {
            assert_eq!(
                D::least_squares(48000., n, &[base]).unwrap_err(),
                E::Capacity
            );
        }
        assert_eq!(D::least_squares(48000., 7, &[]).unwrap_err(), E::Capacity);
        assert_eq!(
            D::least_squares(48000., 7, &[base; 33]).unwrap_err(),
            E::Capacity
        );
        for bad in [
            B { weight: 0., ..base },
            B {
                weight: f64::NAN,
                ..base
            },
            B {
                weight: 1e6 + 1.,
                ..base
            },
            B {
                high_hz: 24001.,
                ..base
            },
            B {
                high_gain: 16.01,
                ..base
            },
            B {
                low_gain: f64::INFINITY,
                ..base
            },
            B {
                high_hz: 0.,
                ..base
            },
            B {
                low_hz: -1.,
                ..base
            },
        ] {
            assert_eq!(
                D::least_squares(48000., 7, &[bad]).unwrap_err(),
                E::InvalidSpecification
            );
        }
        assert_eq!(
            D::least_squares(48000., 7, &[base, base]).unwrap_err(),
            E::InvalidSpecification
        );
    });
    assert_eq!(no_heap.count_total, 0);
    assert_eq!(no_heap.count_current, 0);
    // Ill-conditioned tiny selected band: matrix preparation is permitted,
    // but failure drops all private allocations and returns no unstable taps.
    let original = [B {
        low_hz: 0.,
        high_hz: 0.000001,
        low_gain: 1.,
        high_gain: 1.,
        weight: 1.,
    }];
    let before = original;
    let counts = allocation_counter::measure(|| {
        assert_eq!(
            D::least_squares(48000., 511, &original).unwrap_err(),
            E::Unrepresentable
        );
    });
    assert_eq!(original, before);
    assert_eq!(counts.count_current, 0);
    let d = D::windowed(valid, 48000., 65536, W::Cosine(K::Hamming)).unwrap();
    assert_eq!(d.coefficients().len(), 65536);
    assert!(d.coefficients().iter().all(|h| h.is_finite()));
    assert_eq!(d.group_delay_samples(), 32767.5);
    let full = D::least_squares(
        2.,
        511,
        &[B {
            low_hz: 0.,
            high_hz: 1.,
            low_gain: 1.,
            high_gain: 1.,
            weight: 1e6,
        }],
    )
    .unwrap();
    close(full.coefficients()[255], 1., 1e-14);
    for f in [-1., 0.50001, f64::NAN, f64::INFINITY] {
        assert_eq!(d.magnitude_at(f).unwrap_err(), E::InvalidSpecification);
    }
    // Nonzero Type I signal normalization does not imply every short window
    // can form a valid design: symmetric Hann length2 is exactly zero.
    assert_eq!(
        D::windowed(valid, 48000., 2, W::Cosine(K::Hann)).unwrap_err(),
        E::Unrepresentable
    );
}

#[test]
fn actual_stereo_foundation_graph_generated_fir_owners_preserve_delay_tail_and_heap() {
    use ravet_foundation::{
        AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
        Processor, Transport,
    };
    use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct StereoFir {
        lines: [FirConvolver; 2],
        config: Option<ProcessConfig>,
        frames: Arc<AtomicUsize>,
    }
    impl Processor<f64> for StereoFir {
        fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
            self.config = None;
            if c.layout() != AudioLayout::STEREO {
                return Err(PrepareError::UnsupportedLayout);
            }
            self.config = Some(c);
            self.reset();
            Ok(())
        }
        fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, f64>) -> ProcessStatus {
            if let Err(e) = self.config.unwrap().validate_block(b) {
                b.clear_outputs();
                return ProcessStatus::Fault(e);
            }
            for channel in 0..2 {
                for t in 0..b.audio.frames() {
                    let input = b.audio.input(channel).unwrap()[t];
                    b.audio.output_mut(channel).unwrap()[t] =
                        self.lines[channel].process_sample(input);
                }
            }
            self.frames.fetch_add(b.audio.frames(), Ordering::Relaxed);
            ProcessStatus::Continue
        }
        fn reset(&mut self) {
            for line in &mut self.lines {
                line.reset();
            }
            self.frames.store(0, Ordering::Relaxed);
        }
        fn deactivate(&mut self) {
            self.config = None;
        }
    }
    let input: [Vec<f64>; 2] = std::array::from_fn(|ch| {
        (0..1337)
            .map(|t| {
                if ch == 0 {
                    (t as f64 * 0.31).sin()
                } else {
                    ((t * 41 % 17) as f64 - 8.) / 11.
                }
            })
            .collect()
    });
    let designs = [
        D::windowed(
            R::LowPass { cutoff_hz: 5300. },
            48000.,
            18,
            W::Kaiser { beta: 8. },
        )
        .unwrap(),
        D::least_squares(
            48000.,
            31,
            &[
                B {
                    low_hz: 0.,
                    high_hz: 3000.,
                    low_gain: 0.,
                    high_gain: 0.,
                    weight: 10.,
                },
                B {
                    low_hz: 6000.,
                    high_hz: 18000.,
                    low_gain: 1.,
                    high_gain: 1.,
                    weight: 1.,
                },
            ],
        )
        .unwrap(),
        D::transition_lowpass(48000., 6100., 2000., 25, 3).unwrap(),
        D::halfband_windowed(31, W::Cosine(K::Hamming)).unwrap(),
    ];
    for pair in [0, 2] {
        let designs = [&designs[pair], &designs[pair + 1]];
        let lines =
            std::array::from_fn(|ch| FirConvolver::new(designs[ch].coefficients()).unwrap());
        let frames = Arc::new(AtomicUsize::new(0));
        let node = GraphNode {
            id: NodeId(7),
            layout: AudioLayout::STEREO,
            processor: Box::new(StereoFir {
                lines,
                config: None,
                frames: frames.clone(),
            }),
        };
        let connections: Vec<_> = (0..2)
            .flat_map(|channel| {
                [
                    Connection {
                        source: Source::Input(channel),
                        destination: Destination::Node {
                            id: NodeId(7),
                            channel,
                        },
                        gain: 1.,
                    },
                    Connection {
                        source: Source::Node {
                            id: NodeId(7),
                            channel,
                        },
                        destination: Destination::Output(channel),
                        gain: 1.,
                    },
                ]
            })
            .collect();
        let mut graph = Graph::new(AudioLayout::STEREO, vec![node], &connections).unwrap();
        graph
            .prepare(ProcessConfig::new(48000., 67, 0, AudioLayout::STEREO).unwrap())
            .unwrap();
        let tail = designs.iter().map(|d| d.tail_samples()).max().unwrap();
        let input: [Vec<f64>; 2] = std::array::from_fn(|ch| {
            input[ch]
                .iter()
                .copied()
                .chain(std::iter::repeat_n(0., tail))
                .collect()
        });
        let expected: [Vec<f64>; 2] = std::array::from_fn(|ch| {
            (0..input[ch].len())
                .map(|t| {
                    designs[ch]
                        .coefficients()
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j <= t)
                        .map(|(j, &h)| h * input[ch][t - j])
                        .sum()
                })
                .collect()
        });
        for repeat in 0..2 {
            let mut bad = [[99.; 69]; 2];
            let [left, right] = &mut bad;
            let mut outputs = [left.as_mut_slice(), right.as_mut_slice()];
            let inputs = [&input[0][..68], &input[1][..68]];
            let audio = AudioBuffer::separate(&inputs, &mut outputs, 68).unwrap();
            assert!(matches!(
                graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                GraphStatus::Fault { node: None, .. }
            ));
            assert_eq!(frames.load(Ordering::Relaxed), 0);
            assert!(
                bad.iter()
                    .all(|ch| ch[..68].iter().all(|&v| v == 0.) && ch[68] == 99.)
            );
            let mut start = 0;
            let mut turn = 0;
            while start < input[0].len() {
                let n = [1, 67, 2, 31, 5][turn % 5].min(input[0].len() - start);
                let mut output = [[99.; 69]; 2];
                let counts = allocation_counter::measure(|| {
                    let inputs = [&input[0][start..start + n], &input[1][start..start + n]];
                    let [left, right] = &mut output;
                    let mut outputs = [left.as_mut_slice(), right.as_mut_slice()];
                    let audio = AudioBuffer::separate(&inputs, &mut outputs, n).unwrap();
                    assert_eq!(
                        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        GraphStatus::Continue
                    );
                });
                assert_eq!(counts.count_total, 0);
                assert_eq!(counts.count_current, 0);
                for channel in 0..2 {
                    for j in 0..n {
                        close(output[channel][j], expected[channel][start + j], 1e-14);
                    }
                    assert!(output[channel][n..].iter().all(|&v| v == 99.));
                }
                start += n;
                turn += 1;
            }
            assert_eq!(frames.load(Ordering::Relaxed), input[0].len());
            if repeat == 0 {
                graph.reset();
            }
        }
        graph.deactivate();
    }
}

#[test]
fn generated_responses_compose_with_actual_partitioned_latency_and_explicit_full_tail_drain() {
    use ravet_signal::PartitionedConvolver;
    let designs = [
        D::windowed(
            R::BandPass {
                low_hz: 2100.,
                high_hz: 9400.,
            },
            48000.,
            64,
            W::Kaiser { beta: 7. },
        )
        .unwrap(),
        D::transition_lowpass(48000., 6000., 2400., 33, 4).unwrap(),
        D::halfband_windowed(63, W::Cosine(K::Blackman)).unwrap(),
    ];
    for design in designs {
        for partition in [16, 64] {
            let mut convolver =
                PartitionedConvolver::new(design.coefficients(), partition).unwrap();
            assert_eq!(convolver.latency_samples(), partition);
            assert_eq!(convolver.tail_samples(), partition + design.tail_samples());
            let source: Vec<_> = (0..173)
                .map(|j| ((j * 17 % 23) as f64 - 11.) / 13.)
                .chain(std::iter::repeat_n(0., convolver.tail_samples()))
                .collect();
            let expected: Vec<_> = (0..source.len())
                .map(|t| {
                    if t < partition {
                        0.
                    } else {
                        let t = t - partition;
                        design
                            .coefficients()
                            .iter()
                            .enumerate()
                            .filter(|(j, _)| *j <= t)
                            .map(|(j, &h)| h * source[t - j])
                            .sum()
                    }
                })
                .collect();
            for repeat in 0..2 {
                let mut actual = source.clone();
                let measured = allocation_counter::measure(|| {
                    let mut start = 0;
                    let mut turn = 0;
                    while start < actual.len() {
                        let end = (start + [1, 31, 2, 67, 0, 7][turn % 6]).min(actual.len());
                        convolver.process_in_place(&mut actual[start..end]);
                        start = end;
                        turn += 1;
                    }
                });
                assert_eq!(measured.count_total, 0);
                assert_eq!(measured.count_current, 0);
                for (&a, &b) in actual.iter().zip(&expected) {
                    close(a, b, 2e-14);
                }
                if repeat == 0 {
                    convolver.reset();
                }
            }
        }
    }
}
