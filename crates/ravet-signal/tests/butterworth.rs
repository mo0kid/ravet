use ravet_signal::{ButterworthDesign as D, ButterworthFilter as F, ButterworthResponse as R};
use std::f64::consts::PI;
fn close(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b}; delta {}",
        (a - b).abs()
    );
}
fn scalar(response: R, rate: f64, order: usize, f: f64) -> f64 {
    if f == 0. {
        return if matches!(response, R::LowPass { .. } | R::BandStop { .. }) {
            1.
        } else {
            0.
        };
    }
    if f == 0.5 {
        return if matches!(response, R::HighPass { .. } | R::BandStop { .. }) {
            1.
        } else {
            0.
        };
    }
    let v = (PI * f).tan();
    let (omega, degree) = match response {
        R::LowPass { cutoff_hz } => (v / (PI * cutoff_hz / rate).tan(), order),
        R::HighPass { cutoff_hz } => ((PI * cutoff_hz / rate).tan() / v, order),
        R::BandPass { low_hz, high_hz } | R::BandStop { low_hz, high_hz } => {
            let lo = (PI * low_hz / rate).tan();
            let hi = (PI * high_hz / rate).tan();
            let x = (v * v - lo * hi) / ((hi - lo) * v);
            (
                if matches!(response, R::BandPass { .. }) {
                    x
                } else {
                    1. / x
                },
                order / 2,
            )
        }
    };
    1. / (1. + omega.abs().powi(2 * degree as i32)).sqrt()
}
fn profiles(rate: f64) -> [R; 8] {
    [
        R::LowPass {
            cutoff_hz: rate * 0.013,
        },
        R::LowPass {
            cutoff_hz: rate * 0.19,
        },
        R::HighPass {
            cutoff_hz: rate * 0.023,
        },
        R::HighPass {
            cutoff_hz: rate * 0.39,
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
fn every_order_matches_independent_warped_prototype_and_exact_reference_edges() {
    for rate in [8000., 44100., 192000.] {
        for response in profiles(rate) {
            for order in 1usize..=32 {
                let band = matches!(response, R::BandPass { .. } | R::BandStop { .. });
                if band && !order.is_multiple_of(2) {
                    continue;
                }
                let d = D::new(response, rate, order).unwrap();
                assert_eq!(d.order(), order);
                assert_eq!(d.response(), response);
                assert_eq!(d.stages().len(), order.div_ceil(2));
                for j in 0..=513 {
                    let f = 0.5 * j as f64 / 513.;
                    close(
                        d.magnitude_at(f).unwrap(),
                        scalar(response, rate, order, f),
                        2e-10,
                    );
                }
                match response {
                    R::LowPass { cutoff_hz } | R::HighPass { cutoff_hz } => {
                        close(
                            d.magnitude_at(cutoff_hz / rate).unwrap(),
                            std::f64::consts::FRAC_1_SQRT_2,
                            2e-10,
                        );
                    }
                    R::BandPass { low_hz, high_hz } | R::BandStop { low_hz, high_hz } => {
                        for f in [low_hz, high_hz] {
                            close(
                                d.magnitude_at(f / rate).unwrap(),
                                std::f64::consts::FRAC_1_SQRT_2,
                                2e-10,
                            );
                        }
                        let center = ((PI * low_hz / rate).tan() * (PI * high_hz / rate).tan())
                            .sqrt()
                            .atan()
                            / PI;
                        close(
                            d.magnitude_at(center).unwrap(),
                            if matches!(response, R::BandPass { .. }) {
                                1.
                            } else {
                                0.
                            },
                            2e-10,
                        );
                    }
                }
            }
        }
    }
}
fn multiply(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.; a.len() + b.len() - 1];
    for (i, &a) in a.iter().enumerate() {
        for (j, &b) in b.iter().enumerate() {
            out[i + j] += a * b;
        }
    }
    out
}
fn power(factor: &[f64], n: usize) -> Vec<f64> {
    let mut p = vec![1.];
    for _ in 0..n {
        p = multiply(&p, factor);
    }
    p
}
// Independent direct polynomial oracle: positive Butterworth denominator
// coefficient recurrence, then bilinear polynomial substitution. No digital
// pole construction, section coefficient inspection or production design reuse.
fn direct(response: R, rate: f64, n: usize, input: &[f64]) -> Vec<f64> {
    let (cutoff, high) = match response {
        R::LowPass { cutoff_hz } => (cutoff_hz, false),
        R::HighPass { cutoff_hz } => (cutoff_hz, true),
        _ => unreachable!(),
    };
    let w = (PI * cutoff / rate).tan();
    let mut a = vec![0.; n + 1];
    a[0] = 1.;
    for k in 1..=n {
        a[k] = a[k - 1] * (((k - 1) as f64 * PI / (2 * n) as f64).cos())
            / (k as f64 * PI / (2 * n) as f64).sin();
    }
    let mut denominator = vec![0.; n + 1];
    for (k, &coefficient) in a.iter().enumerate() {
        let p = if high {
            multiply(&power(&[1., 1.], k), &power(&[1., -1.], n - k))
        } else {
            multiply(&power(&[1., -1.], k), &power(&[1., 1.], n - k))
        };
        let scale = if high {
            w.powi(k as i32)
        } else {
            w.powi(-(k as i32))
        };
        for j in 0..=n {
            denominator[j] += coefficient * scale * p[j];
        }
    }
    let mut numerator = power(&[1., if high { -1. } else { 1. }], n);
    let normal = denominator[0];
    for v in &mut denominator {
        *v /= normal;
    }
    for v in &mut numerator {
        *v /= normal;
    }
    let mut output = vec![0.; input.len()];
    for t in 0..input.len() {
        for j in 0..=n.min(t) {
            output[t] += numerator[j] * input[t - j];
        }
        for j in 1..=n.min(t) {
            output[t] -= denominator[j] * output[t - j];
        }
    }
    output
}
#[test]
fn odd_even_time_response_matches_independent_polynomial_recurrence() {
    for n in 1..=7 {
        for response in [
            R::LowPass { cutoff_hz: 5300. },
            R::HighPass { cutoff_hz: 6700. },
        ] {
            let mut input: Vec<_> = (0..2049)
                .map(|j| (j as f64 * 0.072).sin() * 0.17 + (j as f64 * 0.91).cos() * 0.08)
                .collect();
            input[0] += 1.;
            let expected = direct(response, 48000., n, &input);
            let mut f = F::new(D::new(response, 48000., n).unwrap());
            let mut actual = input.clone();
            f.process_in_place(&mut actual);
            for (&a, &b) in actual.iter().zip(&expected) {
                close(a, b, 2e-11);
            }
            f.reset();
            let mut split = input.clone();
            let mut start = 0;
            let mut turn = 0;
            while start < split.len() {
                let end = (start + [1, 63, 2, 19, 0, 5][turn % 6]).min(split.len());
                f.process_in_place(&mut split[start..end]);
                start = end;
                turn += 1;
            }
            assert_eq!(actual, split);
        }
    }
}
#[test]
fn selected_order_replacement_resets_every_history_and_all_processing_is_heap_free() {
    let counts = allocation_counter::measure(|| {
        let mut f = F::new(D::new(R::LowPass { cutoff_hz: 7000. }, 48000., 32).unwrap());
        for response in profiles(48000.) {
            for order in 1usize..=32 {
                if matches!(response, R::BandPass { .. } | R::BandStop { .. })
                    && !order.is_multiple_of(2)
                {
                    continue;
                }
                for _ in 0..177 {
                    let _ = f.process_sample(1.);
                }
                let design = D::new(response, 48000., order).unwrap();
                f.set_design(design);
                let mut fresh = F::new(design);
                let mut a = [0.; 79];
                a[0] = 1.;
                let mut b = a;
                f.process_in_place(&mut a);
                fresh.process_in_place(&mut b);
                assert_eq!(a, b);
                f.reset();
                for sample in [0.; 7] {
                    assert_eq!(f.process_sample(sample), 0.);
                }
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
#[test]
fn specification_selection_verifies_actual_strict_bounds_and_analytic_minimum() {
    for rate in [8000., 44100., 192000.] {
        for high in [false, true] {
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
                let design = if high {
                    D::highpass_spec(rate, pass, stop, rp, atten)
                } else {
                    D::lowpass_spec(rate, pass, stop, rp, atten)
                }
                .unwrap();
                let wp = (PI * pass / rate).tan();
                let ws = (PI * stop / rate).tan();
                let q = if high { wp / ws } else { ws / wp };
                let ep = 10f64.powf(rp / 10.) - 1.;
                let es = 10f64.powf(atten / 10.) - 1.;
                let minimum = ((es / ep).ln() / (2. * q.ln())).ceil() as usize;
                assert_eq!(design.order(), minimum);
                assert!(design.magnitude_at(pass / rate).unwrap() >= 10f64.powf(-rp / 20.));
                assert!(design.magnitude_at(stop / rate).unwrap() <= 10f64.powf(-atten / 20.));
                if minimum > 1 {
                    assert!(q.powi(2 * (minimum - 1) as i32) < es / ep);
                }
                let cutoff = match design.response() {
                    R::LowPass { cutoff_hz } | R::HighPass { cutoff_hz } => cutoff_hz,
                    _ => unreachable!(),
                };
                assert!(cutoff > 0. && cutoff < rate * 0.5);
                close(
                    design.magnitude_at(pass / rate).unwrap(),
                    scalar(design.response(), rate, design.order(), pass / rate),
                    2e-10,
                );
            }
        }
    }
    // Near-integer analytic boundary. Actual rounded SOS must obey both bounds;
    // bounded retry may raise the returned order rather than weaken a constraint.
    for n in [1, 3, 7, 16, 32] {
        let rate = 48000.;
        let pass = 3500.;
        let rp = 1.;
        let atten = 20.;
        let ep = (rp * std::f64::consts::LN_10 / 10.).exp_m1();
        let es = (atten * std::f64::consts::LN_10 / 10.).exp_m1();
        let wp = (PI * pass / rate).tan();
        let stop = (wp * (es / ep).powf(1. / (2 * n) as f64)).atan() / PI * rate;
        match D::lowpass_spec(rate, pass, stop, rp, atten) {
            Ok(d) => {
                assert!(d.order() >= n && d.order() <= 32);
                assert!(d.magnitude_at(pass / rate).unwrap() >= 10f64.powf(-rp / 20.));
                assert!(d.magnitude_at(stop / rate).unwrap() <= 10f64.powf(-atten / 20.));
            }
            Err(_) => assert_eq!(n, 32),
        }
    }
}
#[test]
fn wide_real_root_narrow_conjugate_and_representability_failures_are_explicit() {
    // Degree-one band transformation: wide bands give two real analog poles;
    // narrow bands give a complex pair. Independent impulse poles prove both.
    for response in [
        R::BandPass {
            low_hz: 100.,
            high_hz: 22000.,
        },
        R::BandPass {
            low_hz: 9900.,
            high_hz: 10100.,
        },
        R::BandStop {
            low_hz: 100.,
            high_hz: 22000.,
        },
        R::BandStop {
            low_hz: 9900.,
            high_hz: 10100.,
        },
    ] {
        let (lo, hi) = match response {
            R::BandPass { low_hz, high_hz } | R::BandStop { low_hz, high_hz } => (low_hz, high_hz),
            _ => unreachable!(),
        };
        let l = (PI * lo / 48000.).tan();
        let h = (PI * hi / 48000.).tan();
        let bandwidth = h - l;
        let product = l * h;
        let discr = bandwidth * bandwidth - 4. * product;
        assert_eq!(discr > 0., lo == 100.);
        let design = D::new(response, 48000., 2).unwrap();
        for j in 0..=1000 {
            let f = j as f64 * 0.5 / 1000.;
            close(
                design.magnitude_at(f).unwrap(),
                scalar(response, 48000., 2, f),
                3e-12,
            );
        }
        let sum = 2. * (1. - product) / (1. + bandwidth + product);
        let prod = (1. - bandwidth + product) / (1. + bandwidth + product);
        let mut f = F::new(design);
        let mut y = [0.; 257];
        y[0] = f.process_sample(1.);
        y[1] = f.process_sample(0.);
        y[2] = f.process_sample(0.);
        for j in 3..y.len() {
            y[j] = f.process_sample(0.);
            close(y[j], sum * y[j - 1] - prod * y[j - 2], 1e-14);
        }
    }
    for response in [
        R::LowPass {
            cutoff_hz: f64::from_bits(1),
        },
        R::HighPass {
            cutoff_hz: f64::from_bits(1),
        },
        R::BandPass {
            low_hz: f64::from_bits(1),
            high_hz: 1e-310,
        },
        R::LowPass {
            cutoff_hz: f64::from_bits(24000f64.to_bits() - 1),
        },
    ] {
        assert!(D::new(response, 48000., 32).is_err());
    }
    for n in [0, 33, usize::MAX] {
        assert!(D::new(R::LowPass { cutoff_hz: 1000. }, 48000., n).is_err());
    }
    for n in (1..32).step_by(2) {
        assert!(
            D::new(
                R::BandPass {
                    low_hz: 1000.,
                    high_hz: 2000.
                },
                48000.,
                n
            )
            .is_err()
        );
    }
    for rate in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(D::new(R::LowPass { cutoff_hz: 1000. }, rate, 4).is_err());
    }
    for freq in [0., -1., 24000., 24001., f64::NAN, f64::INFINITY] {
        assert!(D::new(R::HighPass { cutoff_hz: freq }, 48000., 4).is_err());
    }
    for response in [
        R::BandStop {
            low_hz: 2000.,
            high_hz: 1000.,
        },
        R::BandPass {
            low_hz: 1000.,
            high_hz: 1000.,
        },
        R::BandPass {
            low_hz: f64::NAN,
            high_hz: 1000.,
        },
    ] {
        assert!(D::new(response, 48000., 4).is_err());
    }
    for (pass, stop, rp, atten) in [
        (1000., 999., 1., 40.),
        (1000., 1000., 1., 40.),
        (1000., 1000.01, 1., 100.),
        (1000., 2000., 0., 40.),
        (1000., 2000., 2., 1.),
        (1000., 2000., 1., 301.),
        (1000., 2000., f64::NAN, 40.),
        (1000., 2000., 1e-320, 40.),
    ] {
        assert!(D::lowpass_spec(48000., pass, stop, rp, atten).is_err());
    }
    let d = D::new(R::LowPass { cutoff_hz: 3000. }, 48000., 7).unwrap();
    for v in [-1., 0.50001, f64::NAN, f64::INFINITY] {
        assert!(d.magnitude_at(v).is_err());
    }
}

#[test]
fn actual_stereo_foundation_graph_selected_order_processor_preserves_history_preflight_reset_and_heap()
 {
    use ravet_foundation::{
        AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
        Processor, Transport,
    };
    use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct StereoButterworth {
        lines: [F; 2],
        config: Option<ProcessConfig>,
        frames: Arc<AtomicUsize>,
    }
    impl Processor<f64> for StereoButterworth {
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
    for (responses, orders) in [
        (
            [
                R::LowPass { cutoff_hz: 5300. },
                R::HighPass { cutoff_hz: 6700. },
            ],
            [7, 6],
        ),
        (
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
                R::LowPass { cutoff_hz: 7100. },
                R::HighPass { cutoff_hz: 8100. },
            ],
            [1, 32],
        ),
    ] {
        let designs: [D; 2] =
            std::array::from_fn(|ch| D::new(responses[ch], 48000., orders[ch]).unwrap());
        let lines = designs.map(F::new);
        let frames = Arc::new(AtomicUsize::new(0));
        let node = GraphNode {
            id: NodeId(7),
            layout: AudioLayout::STEREO,
            processor: Box::new(StereoButterworth {
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
fn independently_predicted_scalar_gain_matches_rendered_steady_tones_for_all_responses() {
    for (response, order) in [
        (R::LowPass { cutoff_hz: 3700. }, 7),
        (R::HighPass { cutoff_hz: 6900. }, 9),
        (
            R::BandPass {
                low_hz: 3900.,
                high_hz: 11300.,
            },
            10,
        ),
        (
            R::BandStop {
                low_hz: 3900.,
                high_hz: 11300.,
            },
            14,
        ),
    ] {
        for bin in [29, 257, 977, 1859] {
            let frequency = bin as f64 / 4096.;
            let mut filter = F::new(D::new(response, 48000., order).unwrap());
            let mut energy = 0.;
            for t in 0..16384 {
                let output = filter.process_sample((2. * PI * frequency * t as f64).cos());
                if t >= 8192 {
                    energy += output * output;
                }
            }
            close(
                (2. * energy / 8192.).sqrt(),
                scalar(response, 48000., order, frequency),
                2e-12,
            );
        }
    }
}
