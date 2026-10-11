use ravet_signal::{EllipticDesign as D, EllipticFilter as F, EllipticResponse as R};
use std::f64::consts::PI;
fn close(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() <= t, "{a} != {b}, delta {}", (a - b).abs());
}
fn band(r: R) -> bool {
    matches!(r, R::BandPass { .. } | R::BandStop { .. })
}
// Independent complementary-integral power series. Swap to k<=1/sqrt(2); its geometric
// tail is bounded; neither AGM nor production inverse/degree functions are used.
fn integrals(k: f64) -> (f64, f64) {
    assert!(k > 0. && k < 1.);
    if k > 0.5f64.sqrt() {
        let (a, b) = integrals(((1. - k) * (1. + k)).sqrt());
        return (b, a);
    }
    let mut coefficient = 1.;
    let mut power = 1.;
    let mut harmonic = 0.;
    let mut even = 0.;
    let mut regular = 0.;
    let mut complementary = 0.;
    for n in 0..256 {
        if n > 0 {
            let nf = n as f64;
            coefficient *= ((2. * nf - 1.) / (2. * nf)).powi(2);
            power *= k * k;
            harmonic += 1. / nf;
            even += 1. / (2. * nf - 1.) + 1. / (2. * nf);
        }
        regular += coefficient * power;
        complementary += coefficient * power * ((4. / k).ln() - 2. * (even - harmonic));
    }
    let tail = (k * k).powi(256) / (1. - k * k);
    assert!(tail < 1e-70 && tail * (4. / k).ln() < 1e-70);
    (regular * PI * 0.5, complementary)
}
fn theta(which: u8, z: f64, logq: f64) -> f64 {
    let mut sum = if which >= 3 { 1. } else { 0. };
    for n in 0usize..512 {
        let nf = n as f64;
        let term = match which {
            1 => {
                2. * (logq * (nf + 0.5).powi(2)).exp()
                    * if n.is_multiple_of(2) { 1. } else { -1. }
                    * ((2. * nf + 1.) * z).sin()
            }
            2 => 2. * (logq * (nf + 0.5).powi(2)).exp() * ((2. * nf + 1.) * z).cos(),
            3 | 4 => {
                let m = nf + 1.;
                2. * (logq * m * m).exp()
                    * if which == 4 && n.is_multiple_of(2) {
                        -1.
                    } else {
                        1.
                    }
                    * (2. * m * z).cos()
            }
            _ => unreachable!(),
        };
        sum += term;
    }
    let next = if which >= 3 { 513.0 } else { 512.5 };
    // Tail absolute bound using successive exponent differences >=2*next+1.
    let bound = 2. * (logq * next * next).exp() / (1. - (logq * (2. * next + 1.)).exp());
    assert!(bound < 1e-100);
    sum
}
struct Oracle {
    n: usize,
    ep: f64,
    k: f64,
    logq: f64,
    samples: [f64; 16],
}
impl Oracle {
    fn new(n: usize, rp: f64, atten: f64) -> Self {
        let ep = (rp * std::f64::consts::LN_10 / 10.).exp_m1().sqrt();
        let k1 = ep / (atten * std::f64::consts::LN_10 / 10.).exp_m1().sqrt();
        let (a, b) = integrals(k1);
        let logq = -PI * b / (n as f64 * a);
        let k = (theta(2, 0., logq) / theta(3, 0., logq)).powi(2);
        let mut result = Self {
            n,
            ep,
            k,
            logq,
            samples: [0.; 16],
        };
        for index in 0..n / 2 {
            let j = 2 * index + if n.is_multiple_of(2) { 1 } else { 2 };
            result.samples[index] = result.sn_fraction(j as f64 / n as f64);
        }
        result
    }
    fn sn_fraction(&self, f: f64) -> f64 {
        theta(3, 0., self.logq) / theta(2, 0., self.logq) * theta(1, PI * 0.5 * f, self.logq)
            / theta(4, PI * 0.5 * f, self.logq)
    }
    fn rational(&self, x: f64) -> f64 {
        let mut value = if self.n.is_multiple_of(2) { 1. } else { x };
        for index in 0..self.n / 2 {
            let s = self.samples[index];
            let ss = s * s;
            value *= ((x * x - ss) / (1. - self.k * self.k * ss * x * x))
                * ((1. - self.k * self.k * ss) / (1. - ss));
        }
        value
    }
    fn amplitude(&self, x: f64) -> f64 {
        if x.is_infinite() {
            if self.n.is_multiple_of(2) {
                self.amplitude(1e12)
            } else {
                0.
            }
        } else {
            1. / 1f64.hypot(self.ep * self.rational(x))
        }
    }
}
fn scalar(o: &Oracle, r: R, rate: f64, f: f64) -> f64 {
    if f == 0. {
        return o.amplitude(if matches!(r, R::LowPass { .. } | R::BandStop { .. }) {
            0.
        } else {
            f64::INFINITY
        });
    }
    if f == 0.5 {
        return o.amplitude(if matches!(r, R::HighPass { .. } | R::BandStop { .. }) {
            0.
        } else {
            f64::INFINITY
        });
    }
    let v = (PI * f).tan();
    let x = match r {
        R::LowPass { pass_edge_hz } => v / (PI * pass_edge_hz / rate).tan(),
        R::HighPass { pass_edge_hz } => (PI * pass_edge_hz / rate).tan() / v,
        R::BandPass { low_hz, high_hz } | R::BandStop { low_hz, high_hz } => {
            let lo = (PI * low_hz / rate).tan();
            let hi = (PI * high_hz / rate).tan();
            let x = (v * v - lo * hi) / ((hi - lo) * v);
            if matches!(r, R::BandPass { .. }) {
                x
            } else {
                1. / x
            }
        }
    };
    o.amplitude(x)
}
fn profiles(rate: f64) -> [R; 8] {
    [
        R::LowPass {
            pass_edge_hz: rate * 0.023,
        },
        R::LowPass {
            pass_edge_hz: rate * 0.19,
        },
        R::HighPass {
            pass_edge_hz: rate * 0.023,
        },
        R::HighPass {
            pass_edge_hz: rate * 0.39,
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
fn all_orders_responses_match_independent_nome_theta_rational_shape() {
    for rate in [8000., 44100., 192000.] {
        for (rp, atten) in [(0.5, 120.), (3., 180.), (1., 240.)] {
            for r in profiles(rate) {
                for n in 1usize..=32 {
                    if band(r) && !n.is_multiple_of(2) {
                        continue;
                    }
                    let o = Oracle::new(if band(r) { n / 2 } else { n }, rp, atten);
                    let d = D::new(r, rate, n, rp, atten)
                        .unwrap_or_else(|_| panic!("failed {rate} {rp} {atten} {r:?} {n}"));
                    assert_eq!(d.order(), n);
                    assert_eq!(d.response(), r);
                    assert_eq!(d.ripple_db(), rp);
                    assert_eq!(d.attenuation_db(), atten);
                    assert_eq!(d.stages().len(), n.div_ceil(2));
                    close(d.prototype_stop_ratio(), 1. / o.k, 2e-10 / o.k);
                    for j in 0..=257 {
                        let f = j as f64 * 0.5 / 257.;
                        close(d.magnitude_at(f).unwrap(), scalar(&o, r, rate, f), 2e-8);
                    }
                }
            }
        }
    }
}
#[test]
fn exact_parity_pass_critical_points_stop_edge_and_all_transformed_zeros() {
    for n in 1usize..=32 {
        let (rp, atten) = (1., 180.);
        let o = Oracle::new(n, rp, atten);
        for high in [false, true] {
            let edge = 7300.;
            let w = (PI * edge / 48000.).tan();
            let r = if high {
                R::HighPass { pass_edge_hz: edge }
            } else {
                R::LowPass { pass_edge_hz: edge }
            };
            let d = D::new(r, 48000., n, rp, atten).unwrap();
            for j in 0..=n {
                let x = if j == n {
                    1.
                } else {
                    o.sn_fraction(j as f64 / n as f64)
                };
                let f = if high {
                    if x == 0. { 0.5 } else { (w / x).atan() / PI }
                } else {
                    (w * x).atan() / PI
                };
                let expected = if j % 2 != n % 2 {
                    1.
                } else {
                    10f64.powf(-rp / 20.)
                };
                close(d.magnitude_at(f).unwrap(), expected, 2e-8);
            }
            let stop = if high {
                (w * o.k).atan() / PI
            } else {
                (w / o.k).atan() / PI
            };
            close(
                d.magnitude_at(stop).unwrap(),
                10f64.powf(-atten / 20.),
                2e-12,
            );
            close(
                d.magnitude_at(if high { 0. } else { 0.5 }).unwrap(),
                if n.is_multiple_of(2) {
                    10f64.powf(-atten / 20.)
                } else {
                    0.
                },
                2e-12,
            );
            for s in &o.samples[..n / 2] {
                let z = 1. / (o.k * s);
                let f = if high {
                    (w / z).atan() / PI
                } else {
                    (w * z).atan() / PI
                };
                assert!(d.magnitude_at(f).unwrap() < 3e-12);
            }
        }
    }
    for n in [2, 3, 8, 15, 16] {
        let o = Oracle::new(n, 0.5, 120.);
        let lo = (PI * 3100. / 48000.).tan();
        let hi = (PI * 14200. / 48000.).tan();
        for pass in [false, true] {
            let r = if pass {
                R::BandPass {
                    low_hz: 3100.,
                    high_hz: 14200.,
                }
            } else {
                R::BandStop {
                    low_hz: 3100.,
                    high_hz: 14200.,
                }
            };
            let d = D::new(r, 48000., 2 * n, 0.5, 120.).unwrap();
            for s in &o.samples[..n / 2] {
                let zero = 1. / (o.k * s);
                let sum = if pass {
                    (hi - lo) * zero
                } else {
                    -(hi - lo) / zero
                };
                let root = (sum + sum.hypot(2. * (lo * hi).sqrt())) * 0.5;
                let other = -lo * hi / root;
                for z in [root, other] {
                    let f = z.abs().atan() / PI;
                    assert!(d.magnitude_at(f).unwrap() < 2e-11);
                }
            }
        }
    }
}
fn recurrence(input: &[f64], b: [f64; 3], a: [f64; 2]) -> Vec<f64> {
    let mut output = vec![0.; input.len()];
    for i in 0..input.len() {
        let mut y = b[0] * input[i];
        if i >= 1 {
            y += b[1] * input[i - 1] - a[0] * output[i - 1];
        }
        if i >= 2 {
            y += b[2] * input[i - 2] - a[1] * output[i - 2];
        }
        output[i] = y;
    }
    output
}
#[test]
fn first_and_second_order_time_use_independent_closed_spectral_polynomials() {
    let input: Vec<_> = (0..3001)
        .map(|i| {
            if i == 0 {
                1.
            } else {
                0.2 * (i as f64 * 0.071).sin() + 0.17 * (i as f64 * 0.731).cos()
            }
        })
        .collect();
    for (rp, atten) in [(0.25, 40.), (1., 80.), (3., 120.)] {
        let ep = (rp * std::f64::consts::LN_10 / 10.).exp_m1().sqrt();
        let es = (atten * std::f64::consts::LN_10 / 10.).exp_m1().sqrt();
        for n in [1, 2] {
            for high in [false, true] {
                let edge = 7300.;
                let w = (PI * edge / 48000.).tan();
                let r = if high {
                    R::HighPass { pass_edge_hz: edge }
                } else {
                    R::LowPass { pass_edge_hz: edge }
                };
                let (b, a) = if n == 1 {
                    let c = if high { ep * w } else { ep / w };
                    let norm = 1. + c;
                    if high {
                        ([1. / norm, -1. / norm, 0.], [(c - 1.) / norm, 0.])
                    } else {
                        ([1. / norm, 1. / norm, 0.], [(1. - c) / norm, 0.])
                    }
                } else {
                    let k1 = ep / es;
                    let kp = (1. - k1) / (1. + k1);
                    let u = 1. - kp;
                    let v = 1. + kp;
                    let aa = (u * u + ep * ep * v * v).sqrt();
                    let cc = (1. + ep * ep).sqrt();
                    let bb = (2. * aa * cc - 2. * (u + ep * ep * v)).sqrt();
                    // Substitute s=(1-z^-1)/(w*(1+z^-1)) for LP, reciprocal for HP.
                    let (a2, a1, a0, b2, b0) = if high {
                        (cc, bb * w, aa * w * w, 1., u * w * w)
                    } else {
                        (aa, bb * w, cc * w * w, u, w * w)
                    };
                    let norm = a2 + a1 + a0;
                    (
                        [(b2 + b0) / norm, 2. * (b0 - b2) / norm, (b2 + b0) / norm],
                        [2. * (a0 - a2) / norm, (a2 - a1 + a0) / norm],
                    )
                };
                let expected = recurrence(&input, b, a);
                let d = D::new(r, 48000., n, rp, atten).unwrap();
                let mut owner = F::new(d);
                let mut output = input.clone();
                owner.process_in_place(&mut output);
                for (&x, &y) in output.iter().zip(&expected) {
                    close(x, y, 2e-12);
                }
                owner.reset();
                let mut partition = input.clone();
                let mut at = 0;
                for size in [1, 11, 2, 89, 7, 31].into_iter().cycle() {
                    if at == partition.len() {
                        break;
                    }
                    let end = (at + size).min(partition.len());
                    owner.process_in_place(&mut partition[at..end]);
                    at = end;
                }
                assert_eq!(partition, output);
                owner.set_design(d);
                let mut fresh = input.clone();
                owner.process_in_place(&mut fresh);
                assert_eq!(fresh, output);
            }
        }
    }
}
#[test]
fn both_spec_directions_verify_actual_edges_and_independent_minimum() {
    for rate in [8000., 48000., 192000.] {
        for (pf, sf, rp, atten) in [
            (0.08, 0.12, 1., 60.),
            (0.025, 0.027, 0.5, 100.),
            (0.30, 0.36, 3., 120.),
            (0.07, 0.10, 0.1, 180.),
        ] {
            for high in [false, true] {
                let (p, s) = if high {
                    (sf * rate, pf * rate)
                } else {
                    (pf * rate, sf * rate)
                };
                let d = if high {
                    D::highpass_spec(rate, p, s, rp, atten)
                } else {
                    D::lowpass_spec(rate, p, s, rp, atten)
                }
                .unwrap();
                assert!(d.magnitude_at(p / rate).unwrap() >= 10f64.powf(-rp / 20.));
                assert!(d.magnitude_at(s / rate).unwrap() <= 10f64.powf(-atten / 20.));
                let ep = (rp * std::f64::consts::LN_10 / 10.).exp_m1().sqrt();
                let es = (atten * std::f64::consts::LN_10 / 10.).exp_m1().sqrt();
                let wp = (PI * p / rate).tan();
                let ws = (PI * s / rate).tan();
                let ke = if high { ws / wp } else { wp / ws };
                let (ka, kb) = integrals(ke);
                let (k1a, k1b) = integrals(ep / es);
                let bound = ka * k1b / (kb * k1a);
                assert!(d.order() >= bound.ceil() as usize && d.order() <= 32);
                if bound.ceil() > 1. {
                    let prior = Oracle::new(bound.ceil() as usize - 1, rp, atten);
                    assert!(prior.k < ke);
                }
                assert_eq!(d.ripple_db(), rp);
                assert_eq!(d.attenuation_db(), atten);
            }
        }
    }
    for target in [1, 3, 7, 16, 32] {
        let rp = 1.;
        let atten = 180.;
        let o = Oracle::new(target, rp, atten);
        let wp = (PI * 2300. / 48000.).tan();
        for factor in [1. - 1e-8, 1. + 1e-8] {
            let stop = (wp / o.k * factor).atan() / PI * 48000.;
            match D::lowpass_spec(48000., 2300., stop, rp, atten) {
                Ok(d) => {
                    assert!(d.order() >= target && d.order() <= 32);
                    assert!(d.magnitude_at(2300. / 48000.).unwrap() >= 10f64.powf(-rp / 20.));
                    assert!(d.magnitude_at(stop / 48000.).unwrap() <= 10f64.powf(-atten / 20.));
                }
                Err(_) => assert_eq!(target, 32),
            }
        }
    }
}
#[test]
fn all_order_design_processing_replace_and_invalid_profiles_are_heap_free() {
    let baseline = D::new(
        R::LowPass {
            pass_edge_hz: 7100.,
        },
        48000.,
        7,
        1.,
        120.,
    )
    .unwrap();
    let mut f = F::new(baseline);
    let measured = allocation_counter::measure(|| {
        for r in profiles(48000.) {
            for n in 1usize..=32 {
                if band(r) && !n.is_multiple_of(2) {
                    continue;
                }
                let d = D::new(r, 48000., n, 1., 180.).unwrap();
                for _ in 0..37 {
                    let _ = f.process_sample(1.);
                }
                f.set_design(d);
                let mut fresh = F::new(d);
                let mut a = [0.; 127];
                a[0] = 1.;
                let mut b = a;
                f.process_in_place(&mut a);
                fresh.process_in_place(&mut b);
                assert_eq!(a, b);
                f.reset();
                assert_eq!(f.process_sample(0.), 0.);
            }
        }
        let r = R::LowPass {
            pass_edge_hz: 7100.,
        };
        for n in [0, 33, usize::MAX] {
            assert!(D::new(r, 48000., n, 1., 80.).is_err());
        }
        for n in (1..32).step_by(2) {
            assert!(
                D::new(
                    R::BandPass {
                        low_hz: 2100.,
                        high_hz: 9100.
                    },
                    48000.,
                    n,
                    1.,
                    80.
                )
                .is_err()
            );
        }
        for bad in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(D::new(r, bad, 7, 1., 80.).is_err());
            assert!(D::new(r, 48000., 7, bad, 80.).is_err());
            assert!(D::new(r, 48000., 7, 1., bad).is_err());
        }
        for edge in [0., -1., 24000., f64::NAN, f64::INFINITY, f64::from_bits(1)] {
            assert!(D::new(R::LowPass { pass_edge_hz: edge }, 48000., 7, 1., 80.).is_err());
        }
        for (rp, atten) in [
            (120.01, 180.),
            (1., 300.01),
            (2., 1.),
            (1., 1.),
            (1e-320, 80.),
            (1., 1.00001),
        ] {
            assert!(D::new(r, 48000., 32, rp, atten).is_err());
        }
        for r in [
            R::BandStop {
                low_hz: 9000.,
                high_hz: 1000.,
            },
            R::BandStop {
                low_hz: 4000.,
                high_hz: 4000.,
            },
            R::BandStop {
                low_hz: f64::NAN,
                high_hz: 4000.,
            },
        ] {
            assert!(D::new(r, 48000., 6, 1., 80.).is_err());
        }
        for (p, s, rp, atten) in [
            (1000., 999., 1., 80.),
            (1000., 1000.00001, 1., 300.),
            (1000., 2000., 0., 80.),
            (1000., 2000., 2., 1.),
            (1000., 2000., 120.01, 180.),
            (1000., 2000., 1., 300.01),
        ] {
            assert!(D::lowpass_spec(48000., p, s, rp, atten).is_err());
        }
        for bad in [-1., 0.50001, f64::NAN, f64::INFINITY] {
            assert!(baseline.magnitude_at(bad).is_err());
        }
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
}
#[test]
fn rendered_steady_tones_and_finite_impulse_match_independent_scalar() {
    for (r, n) in [
        (
            R::LowPass {
                pass_edge_hz: 5300.,
            },
            7,
        ),
        (
            R::HighPass {
                pass_edge_hz: 6900.,
            },
            8,
        ),
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
        let degree = if band(r) { n / 2 } else { n };
        let o = Oracle::new(degree, 1., 80.);
        let d = D::new(r, 48000., n, 1., 80.).unwrap();
        for bin in [29, 257, 977, 1859] {
            let freq = bin as f64 / 4096.;
            let mut f = F::new(d);
            let mut energy = 0.;
            for t in 0..65536 {
                let y = f.process_sample((2. * PI * freq * t as f64).cos());
                if t >= 57344 {
                    energy += y * y;
                }
            }
            close(
                (2. * energy / 8192.).sqrt(),
                scalar(&o, r, 48000., freq),
                2e-11,
            );
        }
        let mut impulse = vec![0.; 65536];
        impulse[0] = 1.;
        F::new(d).process_in_place(&mut impulse);
        let last: f64 = impulse[61440..].iter().map(|v| v.abs()).sum();
        assert!(last < 1e-24, "observed last window {last}");
        for freq in [0., 0.031, 0.137, 0.269, 0.443, 0.5] {
            let (mut re, mut im) = (0., 0.);
            for (i, &v) in impulse.iter().enumerate() {
                let p = 2. * PI * freq * i as f64;
                re += v * p.cos();
                im -= v * p.sin();
            }
            close(re.hypot(im), scalar(&o, r, 48000., freq), 2e-11);
        }
        // This observed finite-window remainder is not an infinite IIR tail certificate.
    }
}
#[test]
fn actual_stereo_foundation_graph_elliptic_profiles_preserve_history_preflight_reset_and_heap() {
    use ravet_foundation::{
        AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
        Processor, Transport,
    };
    use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct StereoElliptic {
        lines: [F; 2],
        config: Option<ProcessConfig>,
        frames: Arc<AtomicUsize>,
    }
    impl Processor<f64> for StereoElliptic {
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
    for (responses, orders, ripple, attenuation) in [
        (
            [
                R::LowPass {
                    pass_edge_hz: 5300.,
                },
                R::HighPass {
                    pass_edge_hz: 6700.,
                },
            ],
            [7, 6],
            [0.5, 1.],
            [80., 100.],
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
            [1., 3.],
            [100., 120.],
        ),
        (
            [
                R::LowPass {
                    pass_edge_hz: 7100.,
                },
                R::HighPass {
                    pass_edge_hz: 8100.,
                },
            ],
            [1, 32],
            [0.25, 1.],
            [300., 180.],
        ),
    ] {
        let designs: [D; 2] = std::array::from_fn(|ch| {
            D::new(
                responses[ch],
                48000.,
                orders[ch],
                ripple[ch],
                attenuation[ch],
            )
            .unwrap()
        });
        let lines = designs.map(F::new);
        let frames = Arc::new(AtomicUsize::new(0));
        let node = GraphNode {
            id: NodeId(7),
            layout: AudioLayout::STEREO,
            processor: Box::new(StereoElliptic {
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
fn exact_real_prototype_transforms_cover_two_real_and_conjugate_band_time() {
    for db in [0.25, 3.] {
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
                let mut filter = F::new(D::new(r, 48000., 2, db, 120.).unwrap());
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
fn log_nome_oracle_preserves_valid_first_order_when_q1_underflows() {
    let (rp, atten) = (1e-300, 300.);
    let ep = (rp * std::f64::consts::LN_10 / 10.).exp_m1().sqrt();
    let edge = ep.atan() / PI * 48000.;
    let o = Oracle::new(1, rp, atten);
    assert_eq!(o.logq.exp(), 0.);
    assert!(o.k > 0.);
    let d = D::new(R::LowPass { pass_edge_hz: edge }, 48000., 1, rp, atten).unwrap();
    close(d.prototype_stop_ratio() * o.k, 1., 2e-13);
    for freq in [0., 0.1, 0.3, 0.49] {
        close(
            d.magnitude_at(freq).unwrap(),
            scalar(&o, d.response(), 48000., freq),
            3e-13,
        );
    }
    // The higher-degree inverse here has unresolved extreme sc residual and
    // intentionally rejects. The valid N1 family is not gated on exp(logq1).
    assert!(D::new(d.response(), 48000., 7, rp, atten).is_err());
}
