use ravet_signal::{
    ChebyshevDesign as D, ChebyshevFilter as F, ChebyshevKind as K, ChebyshevResponse as R,
    ChebyshevType as T,
};
use std::f64::consts::PI;
fn close(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() <= t, "{a} != {b}, delta {}", (a - b).abs());
}
fn band(r: R) -> bool {
    matches!(r, R::BandPass { .. } | R::BandStop { .. })
}
fn polynomial(n: usize, x: f64) -> f64 {
    let x = x.abs();
    if x > 1. {
        return (n as f64 * x.acosh()).cosh();
    }
    let mut a = 1.;
    let mut b = x;
    for _ in 1..n {
        let c = 2. * x * b - a;
        a = b;
        b = c;
    }
    b
}
fn amplitude(k: K, n: usize, x: f64) -> f64 {
    match k {
        K::Type1 { ripple_db } => {
            1. / 1f64.hypot((10f64.powf(ripple_db / 10.) - 1.).sqrt() * polynomial(n, x))
        }
        K::Type2 { attenuation_db } => {
            if x == 0. {
                1.
            } else {
                1. / 1f64
                    .hypot((10f64.powf(attenuation_db / 10.) - 1.).sqrt() / polynomial(n, 1. / x))
            }
        }
    }
}
fn scalar(k: K, r: R, rate: f64, order: usize, f: f64) -> f64 {
    let n = if band(r) { order / 2 } else { order };
    let endpoint = |zero: bool| {
        if zero {
            amplitude(k, n, 0.)
        } else {
            match k {
                K::Type1 { .. } => 0.,
                K::Type2 { attenuation_db } => {
                    if n.is_multiple_of(2) {
                        10f64.powf(-attenuation_db / 20.)
                    } else {
                        0.
                    }
                }
            }
        }
    };
    if f == 0. {
        return endpoint(matches!(r, R::LowPass { .. } | R::BandStop { .. }));
    }
    if f == 0.5 {
        return endpoint(matches!(r, R::HighPass { .. } | R::BandStop { .. }));
    }
    let v = (PI * f).tan();
    let x = match r {
        R::LowPass { edge_hz } => v / (PI * edge_hz / rate).tan(),
        R::HighPass { edge_hz } => (PI * edge_hz / rate).tan() / v,
        R::BandPass { low_hz, high_hz } | R::BandStop { low_hz, high_hz } => {
            let lo = (PI * low_hz / rate).tan();
            let hi = (PI * high_hz / rate).tan();
            let value = (v * v - lo * hi) / ((hi - lo) * v);
            if matches!(r, R::BandPass { .. }) {
                value
            } else {
                1. / value
            }
        }
    };
    amplitude(k, n, x)
}
fn profiles(rate: f64) -> [R; 8] {
    [
        R::LowPass {
            edge_hz: rate * 0.023,
        },
        R::LowPass {
            edge_hz: rate * 0.19,
        },
        R::HighPass {
            edge_hz: rate * 0.023,
        },
        R::HighPass {
            edge_hz: rate * 0.39,
        },
        R::BandPass {
            low_hz: rate * 0.04,
            high_hz: rate * 0.37,
        },
        R::BandStop {
            low_hz: rate * 0.04,
            high_hz: rate * 0.37,
        },
        R::BandPass {
            low_hz: rate * 0.20,
            high_hz: rate * 0.23,
        },
        R::BandStop {
            low_hz: rate * 0.20,
            high_hz: rate * 0.23,
        },
    ]
}
#[test]
fn both_types_all_orders_responses_match_independent_chebyshev_polynomial() {
    for rate in [8000., 44100., 192000.] {
        for k in [
            K::Type1 { ripple_db: 0.5 },
            K::Type1 { ripple_db: 3. },
            K::Type2 {
                attenuation_db: 40.,
            },
            K::Type2 {
                attenuation_db: 90.,
            },
        ] {
            for r in profiles(rate) {
                for order in 1usize..=32 {
                    if band(r) && !order.is_multiple_of(2) {
                        continue;
                    }
                    let d = D::new(k, r, rate, order).unwrap();
                    assert_eq!(d.kind(), k);
                    assert_eq!(d.response(), r);
                    assert_eq!(d.order(), order);
                    assert_eq!(d.stages().len(), order.div_ceil(2));
                    for j in 0..=513 {
                        let f = j as f64 * 0.5 / 513.;
                        close(
                            d.magnitude_at(f).unwrap(),
                            scalar(k, r, rate, order, f),
                            8e-10,
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn first_order_time_has_independent_closed_form_and_partition_identity() {
    for k in [
        K::Type1 { ripple_db: 0.25 },
        K::Type1 { ripple_db: 12. },
        K::Type2 {
            attenuation_db: 20.,
        },
        K::Type2 {
            attenuation_db: 80.,
        },
    ] {
        for high in [false, true] {
            let edge_hz = 7100.;
            let r = if high {
                R::HighPass { edge_hz }
            } else {
                R::LowPass { edge_hz }
            };
            let db = match k {
                K::Type1 { ripple_db } => ripple_db,
                K::Type2 { attenuation_db } => attenuation_db,
            };
            let eps = (10f64.powf(db / 10.) - 1.).sqrt();
            let w = (PI * edge_hz / 48000.).tan();
            let c = if high { w * eps } else { w / eps };
            let a = (c - 1.) / (c + 1.);
            let b = if high { 1. / (1. + c) } else { c / (1. + c) };
            let sign = if high { -1. } else { 1. };
            let input: Vec<_> = (0..2049)
                .map(|j| ((j * 31 % 19) as f64 - 9.) / 13.)
                .collect();
            let mut expected = vec![0.; input.len()];
            for t in 0..input.len() {
                expected[t] = b * input[t]
                    + if t > 0 {
                        sign * b * input[t - 1] - a * expected[t - 1]
                    } else {
                        0.
                    };
            }
            let mut f = F::new(D::new(k, r, 48000., 1).unwrap());
            let mut output = input.clone();
            f.process_in_place(&mut output);
            for (&a, &b) in output.iter().zip(&expected) {
                close(a, b, 2e-13);
            }
            f.reset();
            let mut split = input.clone();
            for block in split.chunks_mut(37) {
                f.process_in_place(&mut []);
                f.process_in_place(block);
            }
            assert_eq!(output, split);
        }
    }
}
#[test]
fn explicit_pass_ripple_stop_notches_and_far_endpoint_parity_are_preserved() {
    for n in 1usize..=32 {
        let edge = 0.17;
        let w = (PI * edge).tan();
        let rp = 1.25;
        let atten = 60.;
        let one = D::new(
            K::Type1 { ripple_db: rp },
            R::LowPass {
                edge_hz: edge * 48000.,
            },
            48000.,
            n,
        )
        .unwrap();
        for j in 0..=n {
            let x = (PI * j as f64 / n as f64).cos().abs();
            let f = (w * x).atan() / PI;
            close(one.magnitude_at(f).unwrap(), 10f64.powf(-rp / 20.), 3e-10);
        }
        for j in 0..n {
            let x = (PI * (2 * j + 1) as f64 / (2 * n) as f64).cos().abs();
            let f = (w * x).atan() / PI;
            close(one.magnitude_at(f).unwrap(), 1., 3e-10);
        }
        let two = D::new(
            K::Type2 {
                attenuation_db: atten,
            },
            R::LowPass {
                edge_hz: edge * 48000.,
            },
            48000.,
            n,
        )
        .unwrap();
        close(
            two.magnitude_at(edge).unwrap(),
            10f64.powf(-atten / 20.),
            1e-11,
        );
        close(
            two.magnitude_at(0.5).unwrap(),
            if n.is_multiple_of(2) {
                10f64.powf(-atten / 20.)
            } else {
                0.
            },
            1e-11,
        );
        for j in 0..n / 2 {
            let x = 1. / (PI * (2 * j + 1) as f64 / (2 * n) as f64).cos();
            let f = (w * x).atan() / PI;
            close(two.magnitude_at(f).unwrap(), 0., 1e-11);
        }
    }
    // Type II finite prototype zeros each generate two band zeros. Explicitly
    // check both roots at independently solved imaginary frequencies.
    for stop in [false, true] {
        for n in [2usize, 3, 8, 15, 16] {
            let r = if stop {
                R::BandStop {
                    low_hz: 2500.,
                    high_hz: 11000.,
                }
            } else {
                R::BandPass {
                    low_hz: 2500.,
                    high_hz: 11000.,
                }
            };
            let l = (PI * 2500. / 48000.).tan();
            let h = (PI * 11000. / 48000.).tan();
            let b = h - l;
            let p = l * h;
            let d = D::new(
                K::Type2 {
                    attenuation_db: 50.,
                },
                r,
                48000.,
                2 * n,
            )
            .unwrap();
            for j in 0..n / 2 {
                let a = 1. / (PI * (2 * j + 1) as f64 / (2 * n) as f64).cos();
                let q = if stop { -b / a } else { b * a };
                let discr = (q * q + 4. * p).sqrt();
                for y in [(q + discr) / 2., (q - discr) / 2.] {
                    let f = y.abs().atan() / PI;
                    close(d.magnitude_at(f).unwrap(), 0., 3e-11);
                }
            }
        }
    }
}

#[test]
fn both_family_specifications_verify_actual_strict_edges_and_minimum_intervals() {
    for family in [T::Type1, T::Type2] {
        for high in [false, true] {
            for rate in [8000., 44100., 192000.] {
                for (p, s, rp, atten) in [
                    (0.07, 0.13, 1., 40.),
                    (0.16, 0.23, 0.25, 90.),
                    (0.08, 0.33, 3., 180.),
                    (0.21, 0.29, 0.01, 30.),
                ] {
                    let (pass, stop) = if high {
                        ((0.5 - p) * rate, (0.5 - s) * rate)
                    } else {
                        (p * rate, s * rate)
                    };
                    let d = if high {
                        D::highpass_spec(family, rate, pass, stop, rp, atten)
                    } else {
                        D::lowpass_spec(family, rate, pass, stop, rp, atten)
                    }
                    .unwrap();
                    let ep = 10f64.powf(rp / 10.) - 1.;
                    let es = 10f64.powf(atten / 10.) - 1.;
                    let wp = (PI * pass / rate).tan();
                    let ws = (PI * stop / rate).tan();
                    let q = if high { wp / ws } else { ws / wp };
                    let root = (es / ep).sqrt().acosh();
                    let n = (root / q.acosh()).ceil() as usize;
                    assert_eq!(d.order(), n);
                    assert!(d.magnitude_at(pass / rate).unwrap() >= 10f64.powf(-rp / 20.));
                    assert!(d.magnitude_at(stop / rate).unwrap() <= 10f64.powf(-atten / 20.));
                    assert_eq!(
                        d.kind(),
                        if family == T::Type1 {
                            K::Type1 { ripple_db: rp }
                        } else {
                            K::Type2 {
                                attenuation_db: atten,
                            }
                        }
                    );
                    if n > 1 {
                        assert!((root / (n - 1) as f64).cosh() > q);
                    }
                    for j in 0..=128 {
                        let f = j as f64 / 256.;
                        close(
                            d.magnitude_at(f).unwrap(),
                            scalar(d.kind(), d.response(), rate, d.order(), f),
                            5e-10,
                        );
                    }
                }
            }
        }
    }
    for family in [T::Type1, T::Type2] {
        for n in [1, 3, 7, 16, 32] {
            let rate = 48000.;
            let pass = 3500.;
            let rp = 1.;
            let atten = 20.;
            let ep = (rp * std::f64::consts::LN_10 / 10.).exp_m1();
            let es = (atten * std::f64::consts::LN_10 / 10.).exp_m1();
            let wp = (PI * pass / rate).tan();
            let stop = (wp * ((es / ep).sqrt().acosh() / n as f64).cosh()).atan() / PI * rate;
            match D::lowpass_spec(family, rate, pass, stop, rp, atten) {
                Ok(d) => {
                    assert!(d.order() >= n && d.order() <= 32);
                    assert!(d.magnitude_at(pass / rate).unwrap() >= 10f64.powf(-rp / 20.));
                    assert!(d.magnitude_at(stop / rate).unwrap() <= 10f64.powf(-atten / 20.));
                }
                Err(_) => assert_eq!(n, 32),
            }
        }
    }
}
#[test]
fn all_order_replacement_reset_and_invalid_construction_are_heap_free() {
    let measured = allocation_counter::measure(|| {
        let mut owner = F::new(
            D::new(
                K::Type1 { ripple_db: 1. },
                R::LowPass { edge_hz: 7100. },
                48000.,
                32,
            )
            .unwrap(),
        );
        for kind in [
            K::Type1 { ripple_db: 0.5 },
            K::Type2 {
                attenuation_db: 70.,
            },
        ] {
            for r in profiles(48000.) {
                for n in 1usize..=32 {
                    if band(r) && !n.is_multiple_of(2) {
                        continue;
                    }
                    for _ in 0..71 {
                        let _ = owner.process_sample(1.);
                    }
                    let d = D::new(kind, r, 48000., n).unwrap();
                    owner.set_design(d);
                    let mut fresh = F::new(d);
                    let mut a = [0.; 133];
                    a[0] = 1.;
                    let mut b = a;
                    owner.process_in_place(&mut a);
                    fresh.process_in_place(&mut b);
                    assert_eq!(a, b);
                    owner.reset();
                    assert_eq!(owner.process_sample(0.), 0.);
                }
            }
        }
        for n in [0, 33, usize::MAX] {
            assert!(
                D::new(
                    K::Type1 { ripple_db: 1. },
                    R::LowPass { edge_hz: 7100. },
                    48000.,
                    n
                )
                .is_err()
            );
        }
        for n in (1..32).step_by(2) {
            assert!(
                D::new(
                    K::Type2 {
                        attenuation_db: 60.
                    },
                    R::BandPass {
                        low_hz: 2000.,
                        high_hz: 9000.
                    },
                    48000.,
                    n
                )
                .is_err()
            );
        }
        for bad in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(
                D::new(
                    K::Type1 { ripple_db: bad },
                    R::LowPass { edge_hz: 7100. },
                    48000.,
                    3
                )
                .is_err()
            );
            assert!(
                D::new(
                    K::Type2 {
                        attenuation_db: bad
                    },
                    R::LowPass { edge_hz: 7100. },
                    48000.,
                    3
                )
                .is_err()
            );
            assert!(
                D::new(
                    K::Type1 { ripple_db: 1. },
                    R::LowPass { edge_hz: 7100. },
                    bad,
                    3
                )
                .is_err()
            );
        }
        assert!(
            D::new(
                K::Type1 { ripple_db: 120.01 },
                R::LowPass { edge_hz: 7100. },
                48000.,
                3
            )
            .is_err()
        );
        assert!(
            D::new(
                K::Type2 {
                    attenuation_db: 300.01
                },
                R::LowPass { edge_hz: 7100. },
                48000.,
                3
            )
            .is_err()
        );
        for edge_hz in [0., -1., 24000., f64::NAN, f64::INFINITY, f64::from_bits(1)] {
            assert!(
                D::new(
                    K::Type2 {
                        attenuation_db: 60.
                    },
                    R::LowPass { edge_hz },
                    48000.,
                    3
                )
                .is_err()
            );
        }
        for r in [
            R::BandStop {
                low_hz: 8000.,
                high_hz: 2000.,
            },
            R::BandStop {
                low_hz: 5000.,
                high_hz: 5000.,
            },
            R::BandStop {
                low_hz: f64::NAN,
                high_hz: 5000.,
            },
        ] {
            assert!(D::new(K::Type1 { ripple_db: 1. }, r, 48000., 6).is_err());
        }
        for family in [T::Type1, T::Type2] {
            for (p, s, rp, atten) in [
                (1000., 999., 1., 40.),
                (1000., 1000.00001, 1., 300.),
                (1000., 2000., 0., 40.),
                (1000., 2000., 2., 1.),
                (1000., 2000., 120.01, 180.),
                (1000., 2000., 1., 300.01),
                (1000., 2000., f64::NAN, 40.),
                (1000., 2000., 1e-320, 40.),
            ] {
                assert!(D::lowpass_spec(family, 48000., p, s, rp, atten).is_err());
            }
        }
        let d = D::new(
            K::Type1 { ripple_db: 1. },
            R::LowPass { edge_hz: 7100. },
            48000.,
            3,
        )
        .unwrap();
        for f in [-1., 0.50001, f64::NAN, f64::INFINITY] {
            assert!(d.magnitude_at(f).is_err());
        }
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
}
#[test]
fn all_response_rendered_steady_tones_match_independent_scalar_shape() {
    for (kind, r, n) in [
        (K::Type1 { ripple_db: 1. }, R::LowPass { edge_hz: 5300. }, 7),
        (
            K::Type1 { ripple_db: 2. },
            R::HighPass { edge_hz: 6900. },
            8,
        ),
        (
            K::Type2 {
                attenuation_db: 50.,
            },
            R::BandPass {
                low_hz: 3900.,
                high_hz: 11300.,
            },
            10,
        ),
        (
            K::Type2 {
                attenuation_db: 70.,
            },
            R::BandStop {
                low_hz: 3900.,
                high_hz: 11300.,
            },
            14,
        ),
    ] {
        for bin in [29, 257, 977, 1859] {
            let f = bin as f64 / 4096.;
            let mut filter = F::new(D::new(kind, r, 48000., n).unwrap());
            let mut energy = 0.;
            for t in 0..32768 {
                let y = filter.process_sample((2. * PI * f * t as f64).cos());
                if t >= 24576 {
                    energy += y * y;
                }
            }
            close(
                (2. * energy / 8192.).sqrt(),
                scalar(kind, r, 48000., n, f),
                3e-12,
            );
        }
    }
}

#[test]
fn actual_stereo_foundation_graph_chebyshev_profiles_preserve_history_preflight_reset_and_heap() {
    use ravet_foundation::{
        AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
        Processor, Transport,
    };
    use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct StereoChebyshev {
        lines: [F; 2],
        config: Option<ProcessConfig>,
        frames: Arc<AtomicUsize>,
    }
    impl Processor<f64> for StereoChebyshev {
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
    for (kinds, responses, orders) in [
        (
            [
                K::Type1 { ripple_db: 0.5 },
                K::Type2 {
                    attenuation_db: 50.,
                },
            ],
            [
                R::LowPass { edge_hz: 5300. },
                R::HighPass { edge_hz: 6700. },
            ],
            [7, 6],
        ),
        (
            [
                K::Type2 {
                    attenuation_db: 60.,
                },
                K::Type1 { ripple_db: 1. },
            ],
            [
                R::BandPass {
                    low_hz: 2000.,
                    high_hz: 9000.,
                },
                R::BandStop {
                    low_hz: 3100.,
                    high_hz: 7600.,
                },
            ],
            [10, 14],
        ),
        (
            [
                K::Type1 { ripple_db: 0.25 },
                K::Type2 {
                    attenuation_db: 70.,
                },
            ],
            [
                R::LowPass { edge_hz: 7100. },
                R::HighPass { edge_hz: 8100. },
            ],
            [1, 32],
        ),
    ] {
        let designs: [D; 2] =
            std::array::from_fn(|ch| D::new(kinds[ch], responses[ch], 48000., orders[ch]).unwrap());
        let lines = designs.map(F::new);
        let frames = Arc::new(AtomicUsize::new(0));
        let node = GraphNode {
            id: NodeId(7),
            layout: AudioLayout::STEREO,
            processor: Box::new(StereoChebyshev {
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
        let expected: [Vec<f64>; 2] = std::array::from_fn(|channel| {
            let mut filter = F::new(designs[channel]);
            input[channel]
                .iter()
                .map(|&v| filter.process_sample(v))
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
fn second_order_time_uses_independent_polynomial_factor_and_bilinear_oracle() {
    for kind in [
        K::Type1 { ripple_db: 0.25 },
        K::Type1 { ripple_db: 6. },
        K::Type2 {
            attenuation_db: 20.,
        },
        K::Type2 {
            attenuation_db: 60.,
        },
    ] {
        let db = match kind {
            K::Type1 { ripple_db } => ripple_db,
            K::Type2 { attenuation_db } => attenuation_db,
        };
        let e = 10f64.powf(db / 10.) - 1.;
        // Direct denominator from T2(x)=2x²-1 spectral polynomial. No
        // complex-pole/SOS construction or production coefficient inspection.
        let (numerator, denominator) = match kind {
            K::Type1 { .. } => {
                let z = (1. + 1. / e).sqrt();
                let c0 = z / 2.;
                ([c0 / (1. + e).sqrt(), 0., 0.], [c0, (z - 1.).sqrt(), 1.])
            }
            K::Type2 { .. } => {
                let z = (1. + e).sqrt();
                let c0 = z / 2.;
                (
                    [1. / c0, 0., 1. / (2. * c0)],
                    [1. / c0, (z - 1.).sqrt() / c0, 1.],
                )
            }
        };
        for high in [false, true] {
            let w = (PI * 7100. / 48000.).tan();
            let transform = |c: [f64; 3]| {
                let c = if high {
                    [c[2] * w * w, c[1] * w, c[0]]
                } else {
                    [c[0] * w * w, c[1] * w, c[2]]
                };
                [c[0] + c[1] + c[2], 2. * (c[0] - c[2]), c[0] - c[1] + c[2]]
            };
            let b = transform(numerator);
            let a = transform(denominator);
            let input: Vec<_> = (0..4097)
                .map(|t| {
                    if t == 0 {
                        1.
                    } else {
                        (t as f64 * 0.19).sin() * 0.1
                    }
                })
                .collect();
            let mut expected = vec![0.; input.len()];
            for t in 0..input.len() {
                for j in 0..=2.min(t) {
                    expected[t] += b[j] / a[0] * input[t - j];
                }
                for j in 1..=2.min(t) {
                    expected[t] -= a[j] / a[0] * expected[t - j];
                }
            }
            let r = if high {
                R::HighPass { edge_hz: 7100. }
            } else {
                R::LowPass { edge_hz: 7100. }
            };
            let mut filter = F::new(D::new(kind, r, 48000., 2).unwrap());
            let mut actual = input;
            filter.process_in_place(&mut actual);
            for (&a, &b) in actual.iter().zip(&expected) {
                close(a, b, 3e-13);
            }
        }
    }
}
#[test]
fn exact_real_prototype_transforms_cover_two_real_and_conjugate_band_time() {
    for kind in [
        K::Type1 { ripple_db: 0.25 },
        K::Type2 {
            attenuation_db: 20.,
        },
    ] {
        let db = match kind {
            K::Type1 { ripple_db } => ripple_db,
            K::Type2 { attenuation_db } => attenuation_db,
        };
        let epsilon = (10f64.powf(db / 10.) - 1.).sqrt();
        for (low, high, wide) in [(100., 22000., true), (9900., 10100., false)] {
            for stop in [false, true] {
                let l = (PI * low / 48000.).tan();
                let h = (PI * high / 48000.).tan();
                let product = l * h;
                let effective = if stop {
                    (h - l) * epsilon
                } else {
                    (h - l) / epsilon
                };
                assert_eq!(effective * effective - 4. * product > 0., wide);
                let normal = 1. + effective + product;
                let a = [
                    1.,
                    2. * (product - 1.) / normal,
                    (1. - effective + product) / normal,
                ];
                let b = if stop {
                    [
                        (1. + product) / normal,
                        2. * (product - 1.) / normal,
                        (1. + product) / normal,
                    ]
                } else {
                    [effective / normal, 0., -effective / normal]
                };
                let input: Vec<_> = (0..2051)
                    .map(|j| ((j * 31 % 23) as f64 - 11.) / 17.)
                    .collect();
                let mut expected = vec![0.; input.len()];
                for t in 0..input.len() {
                    for j in 0..=2.min(t) {
                        expected[t] += b[j] * input[t - j];
                    }
                    for j in 1..=2.min(t) {
                        expected[t] -= a[j] * expected[t - j];
                    }
                }
                let r = if stop {
                    R::BandStop {
                        low_hz: low,
                        high_hz: high,
                    }
                } else {
                    R::BandPass {
                        low_hz: low,
                        high_hz: high,
                    }
                };
                let mut filter = F::new(D::new(kind, r, 48000., 2).unwrap());
                let mut actual = input;
                for block in actual.chunks_mut(41) {
                    filter.process_in_place(block);
                }
                for (&a, &b) in actual.iter().zip(&expected) {
                    close(a, b, 4e-13);
                }
            }
        }
    }
}

#[test]
fn rendered_impulse_dtft_matches_scalar_profiles_with_explicit_finite_tail_observation() {
    for (kind, r, n) in [
        (K::Type1 { ripple_db: 1. }, R::LowPass { edge_hz: 5300. }, 7),
        (
            K::Type1 { ripple_db: 2. },
            R::HighPass { edge_hz: 6900. },
            8,
        ),
        (
            K::Type2 {
                attenuation_db: 50.,
            },
            R::BandPass {
                low_hz: 3900.,
                high_hz: 11300.,
            },
            10,
        ),
        (
            K::Type2 {
                attenuation_db: 70.,
            },
            R::BandStop {
                low_hz: 3900.,
                high_hz: 11300.,
            },
            14,
        ),
    ] {
        let mut filter = F::new(D::new(kind, r, 48000., n).unwrap());
        let impulse: Vec<_> = (0..32768)
            .map(|t| filter.process_sample(if t == 0 { 1. } else { 0. }))
            .collect();
        // This is an observed final window for these fixed profiles, not an
        // infinite-tail bound or general IIR flush completion criterion.
        assert!(impulse[30720..].iter().map(|h| h.abs()).sum::<f64>() < 1e-20);
        for f in [0., 0.027, 0.17, 0.29, 0.43, 0.5] {
            let (re, im) = impulse
                .iter()
                .enumerate()
                .fold((0., 0.), |(re, im), (t, &h)| {
                    (
                        re + h * (2. * PI * f * t as f64).cos(),
                        im - h * (2. * PI * f * t as f64).sin(),
                    )
                });
            close(re.hypot(im), scalar(kind, r, 48000., n, f), 5e-12);
        }
    }
}
