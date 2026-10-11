use ravet_signal::{
    ConfiguredHalfBand2x as P, ConfiguredHalfBand2x32 as P32, HalfBandIirDesign as D,
    HalfBandIirDesign32 as D32, HalfBandIirFilter as F, HalfBandIirFilter32 as F32,
};
fn close(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() <= t, "{a} != {b}, delta {}", (a - b).abs());
}
#[test]
fn selected_full_odd_family_precision_coefficients_and_actual_peak_are_bounded() {
    for n in (1..=63).step_by(2) {
        let d = D::new(n, 0.01).unwrap_or_else(|e| panic!("order{n} {e:?}"));
        let q = d.quantize_f32().unwrap();
        assert_eq!(d.order(), n);
        assert_eq!(
            d.direct_coefficients().len() + d.delayed_coefficients().len(),
            (n - 1) / 2
        );
        assert_eq!(d.direct_coefficients().len(), ((n - 1) / 2).div_ceil(2));
        for f in [0., 0.01, 0.1, 0.2, 0.245, 0.25, 0.255, 0.3, 0.49, 0.5] {
            let a = d.magnitude_at(f).unwrap();
            let b = d.magnitude_at(0.5 - f).unwrap();
            close(a * a + b * b, 1., 2e-13);
            let a = q.magnitude_at(f).unwrap();
            let b = q.magnitude_at(0.5 - f).unwrap();
            close(a * a + b * b, 1., 2e-13);
        }
        for f in [
            d.sampled_stopband_peak().normalized_frequency,
            q.sampled_stopband_peak().normalized_frequency,
        ] {
            assert!((d.stop_edge()..=0.5).contains(&f));
        }
        let p = d.sampled_stopband_peak();
        assert_eq!(p.magnitude, d.magnitude_at(p.normalized_frequency).unwrap());
        let p = q.sampled_stopband_peak();
        assert_eq!(p.magnitude, q.magnitude_at(p.normalized_frequency).unwrap());
    }
}
#[test]
fn exact_first_order_and_both_unequal_branch_polyphase_conventions_match_full_rate() {
    for n in [1, 3, 5, 7, 9, 15, 31, 63] {
        let d = D::new(n, 0.01).unwrap();
        let q = d.quantize_f32().unwrap();
        let mut p = P::new(d, d);
        let mut full = F::new(d);
        let mut down = F::new(d);
        let mut p32 = P32::new(q, q);
        let mut full32 = F32::new(q);
        let mut down32 = F32::new(q);
        for i in 0..2048 {
            let x = if i == 0 {
                1.
            } else {
                (i as f64 * 0.117).sin() * 0.1
            };
            let up = p.upsample(x);
            let expected = [
                2. * full.process_sample(x)[0],
                2. * full.process_sample(0.)[0],
            ];
            assert_eq!(up, expected);
            let expected_down = down.process_sample(up[0])[0];
            down.process_sample(up[1]);
            assert_eq!(p.downsample(up), expected_down);
            let x = x as f32;
            let up = p32.upsample(x);
            let expected = [
                2. * full32.process_sample(x)[0],
                2. * full32.process_sample(0.)[0],
            ];
            assert_eq!(up, expected);
            let expected_down = down32.process_sample(up[0])[0];
            down32.process_sample(up[1]);
            assert_eq!(p32.downsample(up), expected_down);
        }
        p.reset();
        full.reset();
        down.reset();
        p32.reset();
        full32.reset();
        down32.reset();
        assert_eq!(p.upsample(0.), [0., 0.]);
        assert_eq!(p.downsample([0., 0.]), 0.);
        assert_eq!(p32.upsample(0.), [0., 0.]);
        assert_eq!(p32.downsample([0., 0.]), 0.);
    }
    let d = D::new(1, 0.1).unwrap();
    let mut f = F::new(d);
    let mut previous = 0.;
    for x in [1., 0., -0.5, 0.3] {
        assert_eq!(
            f.process_sample(x),
            [(x + previous) / 2., (x - previous) / 2.]
        );
        previous = x;
    }
}
#[test]
fn exact_fixed_profile_origin_and_strict_actual_precision_edge_sizing() {
    let d = D::new(15, 0.05).unwrap();
    let expected = [
        0.045728148016233136,
        0.3325011117394729,
        0.6632020224193994,
        0.9338558037446271,
        0.16808754571240941,
        0.504485750282933,
        0.803780867911122,
    ];
    for (&a, &b) in d
        .direct_coefficients()
        .iter()
        .chain(d.delayed_coefficients())
        .zip(&expected)
    {
        close(a, b, 8e-15);
    }
    for width in [0.001, 0.01, 0.05, 0.1, 0.2] {
        for db in [12., 30., 60., 90., 120.] {
            let a = D::from_attenuation(width, db).unwrap_or_else(|e| panic!("{width} {db} {e:?}"));
            assert!(a.stop_edge_magnitude().unwrap() <= 10f64.powf(-db / 20.));
            let a = D32::from_attenuation(width, db)
                .unwrap_or_else(|e| panic!("f32{width} {db} {e:?}"));
            assert!(a.stop_edge_magnitude().unwrap() <= 10f64.powf(-db / 20.));
        }
    }
}

use std::f64::consts::PI;
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

#[test]
fn every_order_matches_independent_complete_series_theta_poles_and_rational_spectrum() {
    for width in [0.01, 0.05, 0.15] {
        let pass = 0.25 - width / 2.;
        let warp = (PI * pass).tan();
        let k = warp * warp;
        let (complete, complement) = integrals(k);
        let logq = -PI * complement / complete;
        let t2 = theta(2, 0., logq);
        let t3 = theta(3, 0., logq);
        let t4 = theta(4, 0., logq);
        for n in (1usize..=63).step_by(2) {
            let logq1 = n as f64 * logq;
            let k1 = (theta(2, 0., logq1) / theta(3, 0., logq1)).powi(2);
            let attenuation = 10. / std::f64::consts::LN_10 * ((1. + k1).ln() - k1.ln());
            if attenuation > 300. {
                assert!(D::new(n, width).is_err());
                continue;
            }
            let d = D::new(n, width).unwrap();
            let q = d.quantize_f32().unwrap();
            close(d.nominal_attenuation_db(), attenuation, 2e-9);
            close(
                d.nominal_ripple_db(),
                10. / std::f64::consts::LN_10 * k1.ln_1p(),
                2e-12,
            );
            let mut spectral = [0.; 31];
            for j in 1..=(n - 1) / 2 {
                let z = PI * j as f64 / n as f64;
                let denominator = theta(4, z, logq);
                let sn = t3 / t2 * theta(1, z, logq) / denominator;
                let cn = t4 / t2 * theta(2, z, logq) / denominator;
                let dn = t4 / t3 * theta(3, z, logq) / denominator;
                spectral[j - 1] = sn;
                // Independent full complex prototype pole/bilinear map, rather
                // than production's rationalized scalar radius formula.
                let sv = 1. / (1. + k).sqrt();
                let cv = (k / (1. + k)).sqrt();
                let dv = k.sqrt();
                let denominator = 1. - dn * dn * sv * sv;
                let re = -cn * dn * sv * cv / denominator * warp;
                let im = sn * dv / denominator * warp;
                let norm = (1. - re) * (1. - re) + im * im;
                let digital_re = (1. - re * re - im * im) / norm;
                close(digital_re, 0., 3e-13);
                let radius = ((1. + re) * (1. + re) + im * im) / norm;
                let actual = if j % 2 == 1 {
                    d.direct_coefficients()[(j - 1) / 2]
                } else {
                    d.delayed_coefficients()[(j - 1) / 2]
                };
                close(actual, radius, 3e-13);
            }
            for j in 0..=1024 {
                let f = j as f64 * 0.5 / 1024.;
                let expected = if f == 0.5 {
                    0.
                } else {
                    let x = (PI * f).tan() / warp;
                    let mut rational = x;
                    for &sn in &spectral[..(n - 1) / 2] {
                        let ss = sn * sn;
                        rational *= ((x * x - ss) / (1. - k * k * ss * x * x))
                            * ((1. - k * k * ss) / (1. - ss));
                    }
                    1. / 1f64.hypot(k1.sqrt() * rational)
                };
                close(d.magnitude_at(f).unwrap(), expected, 2e-10);
                // Actual f32 coefficients retain a separate, measured response.
                let rounded = q.magnitude_at(f).unwrap();
                assert!(rounded.is_finite());
            }
        }
    }
}

fn multiply(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.; a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        for (j, &y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    out
}
struct Polynomial {
    b: Vec<f64>,
    a: Vec<f64>,
    x: Vec<f64>,
    y: Vec<f64>,
}
impl Polynomial {
    fn new(d: D) -> Self {
        let branch = |coefficients: &[f64]| {
            let mut b = vec![1.];
            let mut a = vec![1.];
            for &v in coefficients {
                b = multiply(&b, &[v, 0., 1.]);
                a = multiply(&a, &[1., 0., v]);
            }
            (b, a)
        };
        let (b0, a0) = branch(d.direct_coefficients());
        let (b1, a1) = branch(d.delayed_coefficients());
        let a = multiply(&a0, &a1);
        let direct = multiply(&b0, &a1);
        let delayed = multiply(&b1, &a0);
        let mut b = vec![0.; direct.len().max(delayed.len() + 1)];
        for (i, x) in direct.iter().enumerate() {
            b[i] += x / 2.;
        }
        for (i, x) in delayed.iter().enumerate() {
            b[i + 1] += x / 2.;
        }
        let count = b.len().max(a.len());
        Self {
            b,
            a,
            x: vec![0.; count],
            y: vec![0.; count],
        }
    }
    fn sample(&mut self, input: f64) -> f64 {
        self.x.rotate_right(1);
        self.y.rotate_right(1);
        self.x[0] = input;
        let value = self.b.iter().zip(&self.x).map(|(b, x)| b * x).sum::<f64>()
            - self
                .a
                .iter()
                .skip(1)
                .zip(self.y.iter().skip(1))
                .map(|(a, y)| a * y)
                .sum::<f64>();
        self.y[0] = value;
        value
    }
}
#[test]
fn independent_rational_polynomial_time_and_full_callback_lifecycle_match() {
    for n in [1, 3, 5, 7, 9] {
        let d = D::new(n, 0.1).unwrap();
        let mut p = Polynomial::new(d);
        let mut f = F::new(d);
        let source: Vec<_> = (0..3072)
            .map(|i| {
                if i == 0 {
                    1.
                } else if i < 1024 {
                    (i as f64 * 0.237).sin() * 0.2 + ((i * 29 % 13) as f64 - 6.) / 53.
                } else {
                    0.
                }
            })
            .collect();
        let expected: Vec<_> = source.iter().map(|&x| p.sample(x)).collect();
        let mut output = source.clone();
        f.process_in_place_lowpass(&mut output);
        for (&a, &b) in output.iter().zip(&expected) {
            close(a, b, 3e-13);
        }
        assert!(output[2048..].iter().map(|x| x.abs()).sum::<f64>() < 1e-30);
        let mut block = source.clone();
        f.reset();
        let measured = allocation_counter::measure(|| {
            let mut at = 0;
            let mut turn = 0;
            while at < block.len() {
                let end = (at + [1, 17, 0, 31, 2, 7][turn % 6]).min(block.len());
                f.process_in_place_lowpass(&mut block[at..end]);
                at = end;
                turn += 1;
            }
        });
        assert_eq!(measured.count_total, 0);
        assert_eq!(block, output);
        let mut out = [73.; 3];
        let before = f.clone();
        assert!(f.process_block_lowpass(&[1.; 2], &mut out).is_err());
        assert_eq!(out, [73.; 3]);
        let mut oracle = before;
        for _ in 0..71 {
            assert_eq!(f.process_sample(0.17), oracle.process_sample(0.17));
        }
        f.replace_design(d);
        let mut fresh = F::new(d);
        for &x in &source {
            assert_eq!(f.process_sample(x), fresh.process_sample(x));
        }
    }
}
#[test]
fn independently_nested_polynomial_oversampling_profiles_and_nonlinear_time_order_match() {
    use ravet_signal::{ConfiguredOversampler as O, OversamplingFactor as R};
    let profiles = [
        (D::new(3, 0.1).unwrap(), D::new(5, 0.08).unwrap()),
        (D::new(5, 0.05).unwrap(), D::new(3, 0.15).unwrap()),
        (D::new(7, 0.04).unwrap(), D::new(5, 0.07).unwrap()),
    ];
    let source: Vec<_> = (0..1536)
        .map(|i| {
            if i == 0 {
                0.7
            } else {
                (i as f64 * 0.083).sin() * 0.31 + (i as f64 * 0.59).cos() * 0.17
            }
        })
        .collect();
    for factor in [R::One, R::Two, R::Four, R::Eight] {
        let count = match factor {
            R::One => 0,
            R::Two => 1,
            R::Four => 2,
            R::Eight => 3,
        };
        let mut up = profiles.map(|p| Polynomial::new(p.0));
        let mut down = profiles.map(|p| Polynomial::new(p.1));
        let mut expected = Vec::new();
        let mut call_order = Vec::new();
        for &x in &source {
            let mut samples = vec![x];
            for stage in &mut up[..count] {
                samples = samples
                    .iter()
                    .flat_map(|&x| [2. * stage.sample(x), 2. * stage.sample(0.)])
                    .collect();
            }
            for x in &mut samples {
                call_order.push(*x);
                *x = (2. * *x).tanh();
            }
            for stage in down[..count].iter_mut().rev() {
                samples = samples
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| {
                        let y = stage.sample(p[0]);
                        stage.sample(p[1]);
                        y
                    })
                    .collect();
            }
            expected.push(samples[0]);
        }
        let mut o = O::from_stages(factor, profiles);
        let mut actual = source.clone();
        let mut calls = Vec::with_capacity(source.len() * factor.factor());
        let measured = allocation_counter::measure(|| {
            o.process_in_place(&mut actual, |x| {
                calls.push(x);
                (2. * x).tanh()
            })
        });
        assert_eq!(measured.count_total, 0);
        assert_eq!(calls.len(), source.len() * factor.factor());
        for (&a, &b) in calls.iter().zip(&call_order) {
            close(a, b, 3e-13);
        }
        for (&a, &b) in actual.iter().zip(&expected) {
            close(a, b, 5e-13);
        }
        let mut output = [17.; 7];
        let mut callback_count = 0;
        let mut prior = o.clone();
        assert!(
            o.process_block(&[0.; 6], &mut output, |x| {
                callback_count += 1;
                x
            })
            .is_err()
        );
        assert_eq!(callback_count, 0);
        assert_eq!(output, [17.; 7]);
        for x in [0.1, -0.2, 0.7] {
            assert_eq!(
                o.process_sample(x, |x| x * x),
                prior.process_sample(x, |x| x * x)
            );
        }
        o.reset();
        let mut partition = source.clone();
        let mut at = 0;
        let mut turn = 0;
        while at < partition.len() {
            let end = (at + [7, 0, 1, 23, 2, 31][turn % 6]).min(partition.len());
            o.process_in_place(&mut partition[at..end], |x| (2. * x).tanh());
            at = end;
            turn += 1;
        }
        assert_eq!(partition, actual);
    }
}

macro_rules! native_graph_fixture {
    ($name:ident,$sample:ty,$design:ty,$oversampler:ty) => {
        #[test]
        fn $name() {
            use ravet_foundation::{
                AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
                Processor, Transport,
            };
            use ravet_graph::{
                Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source,
            };
            use ravet_signal::OversamplingFactor as R;
            use std::sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            };
            struct Stereo {
                lines: [$oversampler; 2],
                config: Option<ProcessConfig>,
                frames: Arc<AtomicUsize>,
                deactivations: Arc<AtomicUsize>,
            }
            impl Processor<$sample> for Stereo {
                fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
                    self.config = None;
                    if c.layout() != AudioLayout::STEREO {
                        return Err(PrepareError::UnsupportedLayout);
                    }
                    self.config = Some(c);
                    self.reset();
                    Ok(())
                }
                fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                    if let Err(e) = self.config.unwrap().validate_block(b) {
                        b.clear_outputs();
                        return ProcessStatus::Fault(e);
                    }
                    for ch in 0..2 {
                        for i in 0..b.audio.frames() {
                            let x = b.audio.input(ch).unwrap()[i];
                            b.audio.output_mut(ch).unwrap()[i] =
                                self.lines[ch].process_sample(x, |x| (2. * x).tanh());
                        }
                    }
                    self.frames.fetch_add(b.audio.frames(), Ordering::Relaxed);
                    ProcessStatus::Continue
                }
                fn reset(&mut self) {
                    for p in &mut self.lines {
                        p.reset();
                    }
                    self.frames.store(0, Ordering::Relaxed);
                }
                fn deactivate(&mut self) {
                    if self.config.take().is_some() {
                        self.deactivations.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            let profiles = [
                (
                    <$design>::from_attenuation(0.05, 60.).unwrap(),
                    <$design>::from_attenuation(0.1, 40.).unwrap(),
                ),
                (
                    <$design>::from_attenuation(0.02, 30.).unwrap(),
                    <$design>::from_attenuation(0.07, 50.).unwrap(),
                ),
                (
                    <$design>::from_attenuation(0.04, 50.).unwrap(),
                    <$design>::from_attenuation(0.05, 30.).unwrap(),
                ),
            ];
            let mut lines = [
                <$oversampler>::from_stages(R::Four, profiles),
                <$oversampler>::from_stages(R::Eight, [profiles[2], profiles[0], profiles[1]]),
            ];
            let input: [Vec<$sample>; 2] = std::array::from_fn(|ch| {
                (0..1501)
                    .map(|i| {
                        ((i as f64 * (0.17 + ch as f64 * 0.037)).sin() * 0.31
                            + ((i * 13 % 23) as f64 - 11.) / 73.) as $sample
                    })
                    .collect()
            });
            let expected: [Vec<$sample>; 2] = std::array::from_fn(|ch| {
                input[ch]
                    .iter()
                    .map(|&x| lines[ch].process_sample(x, |x| (2. * x).tanh()))
                    .collect()
            });
            for p in &mut lines {
                p.reset();
            }
            let frames = Arc::new(AtomicUsize::new(0));
            let deactivations = Arc::new(AtomicUsize::new(0));
            let node = GraphNode {
                id: NodeId(19),
                layout: AudioLayout::STEREO,
                processor: Box::new(Stereo {
                    lines,
                    config: None,
                    frames: frames.clone(),
                    deactivations: deactivations.clone(),
                }),
            };
            let routes: Vec<_> = (0..2)
                .flat_map(|channel| {
                    [
                        Connection {
                            source: Source::Input(channel),
                            destination: Destination::Node {
                                id: NodeId(19),
                                channel,
                            },
                            gain: 1.,
                        },
                        Connection {
                            source: Source::Node {
                                id: NodeId(19),
                                channel,
                            },
                            destination: Destination::Output(channel),
                            gain: 1.,
                        },
                    ]
                })
                .collect();
            let mut graph = Graph::new(AudioLayout::STEREO, vec![node], &routes).unwrap();
            for cycle in 0..2 {
                graph
                    .prepare(ProcessConfig::new(48000., 67, 0, AudioLayout::STEREO).unwrap())
                    .unwrap();
                for repeat in 0..2 {
                    let mut bad = [[99.; 69]; 2];
                    let [l, r] = &mut bad;
                    let mut outputs = [l.as_mut_slice(), r.as_mut_slice()];
                    let inputs = [&input[0][..68], &input[1][..68]];
                    let audio = AudioBuffer::separate(&inputs, &mut outputs, 68).unwrap();
                    assert!(matches!(
                        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        GraphStatus::Fault { node: None, .. }
                    ));
                    assert_eq!(frames.load(Ordering::Relaxed), 0);
                    assert!(
                        bad.iter()
                            .all(|c| c[..68].iter().all(|&x| x == 0.) && c[68] == 99.)
                    );
                    let mut start = 0;
                    let mut turn = 0;
                    while start < input[0].len() {
                        let n = [1, 67, 2, 0, 31, 5][turn % 6].min(input[0].len() - start);
                        let mut output = [[99.; 69]; 2];
                        let measured = allocation_counter::measure(|| {
                            let inputs = [&input[0][start..start + n], &input[1][start..start + n]];
                            let [l, r] = &mut output;
                            let mut outputs = [l.as_mut_slice(), r.as_mut_slice()];
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, n).unwrap();
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
                            assert_eq!(&output[ch][..n], &expected[ch][start..start + n]);
                            assert!(output[ch][n..].iter().all(|&x| x == 99.));
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
                assert_eq!(deactivations.load(Ordering::Relaxed), cycle + 1);
            }
        }
    };
}
native_graph_fixture!(
    actual_canonical_native64_graph_preflight_lifecycle_and_mixed_profiles,
    f64,
    D,
    ravet_signal::ConfiguredOversampler
);
native_graph_fixture!(
    actual_canonical_native32_graph_preflight_lifecycle_and_mixed_profiles,
    f32,
    D32,
    ravet_signal::ConfiguredOversampler32
);

#[test]
fn invalid_unresolved_quantized_profiles_and_all_design_runtime_lifecycles_preserve_owners_without_heap()
 {
    use ravet_signal::{
        ConfiguredOversampler as O, ConfiguredOversampler32 as O32, OversamplingFactor as R,
    };
    let before = D::new(15, 0.05).unwrap();
    let mut f = F::new(before);
    f.process_sample(0.7);
    let mut retained = f.clone();
    let measured = allocation_counter::measure(|| {
        for n in [0, 2, 4, 62, 64, 65, usize::MAX] {
            assert!(D::new(n, 0.1).is_err());
        }
        for width in [0., -1., 0.5, 1., f64::NAN, f64::INFINITY, 1e-320] {
            assert!(D::new(3, width).is_err());
            assert!(D::from_attenuation(width, 50.).is_err());
            assert!(D32::from_attenuation(width, 50.).is_err());
        }
        for db in [0., 3., 3.010299956639812, 301., f64::NAN, f64::INFINITY] {
            assert!(D::from_attenuation(0.1, db).is_err());
            assert!(D32::from_attenuation(0.1, db).is_err());
        }
        assert!(D::new(63, 0.15).is_err()); // derived nominal attenuation exceeds explicit300dB profile
        assert!(D::from_attenuation(1e-6, 300.).is_err()); // required odd order exceeds63
        for n in (1..=63).step_by(2) {
            let d = D::new(n, 0.01).unwrap();
            let q = d.quantize_f32().unwrap();
            let mut o = O::new(R::Eight, d, d);
            let mut o32 = O32::new(R::Four, q, q);
            let mut p = P::new(d, d);
            let mut p32 = P32::new(q, q);
            let mut filter = F::new(d);
            let mut filter32 = F32::new(q);
            for _ in 0..31 {
                let _ = o.process_sample(0.17, |x| x * x);
                let _ = o32.process_sample(0.17, |x| x * x);
                let up = p.upsample(0.3);
                p.downsample(up);
                let up = p32.upsample(0.3);
                p32.downsample(up);
                filter.process_sample(0.2);
                filter32.process_sample(0.2);
            }
            o.reset();
            o32.reset();
            p.reset();
            p32.reset();
            filter.replace_design(d);
            filter32.replace_design(q);
            assert_eq!(o.process_sample(0., |x| x), 0.);
            assert_eq!(o32.process_sample(0., |x| x), 0.);
            assert_eq!(filter.process_sample(0.), [0., 0.]);
            assert_eq!(filter32.process_sample(0.), [0., 0.]);
        }
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
    for x in [0.1, -0.2, 0.3] {
        assert_eq!(f.process_sample(x), retained.process_sample(x));
    }
    assert_eq!(*f.design(), before);
    let d = D::new(63, 1e-8).unwrap();
    assert!(d.quantize_f32().is_err()); // largest positive stable coefficient rounds1
}

fn phase_magnitude(
    direct: impl Iterator<Item = f64>,
    delayed: impl Iterator<Item = f64>,
    f: f64,
) -> f64 {
    if f == 0. {
        return 1.;
    }
    if f == 0.5 {
        return 0.;
    }
    let t = (2. * PI * f).tan();
    let p = |a: f64| -2. * (((1. - a) / (1. + a)) * t).atan();
    ((direct.map(p).sum::<f64>() - delayed.map(p).sum::<f64>() + 2. * PI * f) / 2.)
        .cos()
        .abs()
}
#[test]
fn actual_sampled_stop_peaks_and_sizing_boundaries_have_independent_phase_and_degree_oracles() {
    for n in [1, 3, 5, 13, 15, 31, 63] {
        for width in [0.001, 0.01, 0.05] {
            let Ok(d) = D::new(n, width) else {
                continue;
            };
            let q = d.quantize_f32().unwrap();
            let mut peak = 0f64;
            let mut peak32 = 0f64;
            for j in 0..4097 {
                let t = j as f64 / 4096.;
                let f = (1. - t) * d.stop_edge() + t * 0.5;
                peak = peak.max(phase_magnitude(
                    d.direct_coefficients().iter().copied(),
                    d.delayed_coefficients().iter().copied(),
                    f,
                ));
                peak32 = peak32.max(phase_magnitude(
                    q.direct_coefficients().iter().copied().map(f64::from),
                    q.delayed_coefficients().iter().copied().map(f64::from),
                    f,
                ));
            }
            close(peak, d.sampled_stopband_peak().magnitude, 8e-14);
            close(peak32, q.sampled_stopband_peak().magnitude, 8e-14);
        }
    }
    for width in [0.001, 0.01, 0.05, 0.1] {
        for db in [12., 30., 60., 90., 120.] {
            let p = (PI * (0.25 - width / 2.)).tan();
            let k = p * p;
            let delta = 10f64.powf(-db / 20.);
            let k1 = delta * delta / (1. - delta * delta);
            let (a, b) = integrals(k);
            let (a1, b1) = integrals(k1);
            let minimum = (b1 / a1 / (b / a)).ceil() as usize;
            let minimum = if minimum.is_multiple_of(2) {
                minimum + 1
            } else {
                minimum
            };
            let d = D::from_attenuation(width, db).unwrap();
            let q = D32::from_attenuation(width, db).unwrap();
            assert!(d.order() >= minimum && q.order() >= minimum);
            if minimum > 1 {
                let logq = -PI * b / a * (minimum - 2) as f64;
                let k1 = (theta(2, 0., logq) / theta(3, 0., logq)).powi(2);
                let prior = 10. / std::f64::consts::LN_10 * (k1.ln_1p() - k1.ln());
                assert!(prior < db);
            }
        }
    }
    let width = 0.001;
    let p = (PI * (0.25 - width / 2.)).tan();
    let (a, b) = integrals(p * p);
    let logq = -PI * b / a * 63.;
    let k1 = (theta(2, 0., logq) / theta(3, 0., logq)).powi(2);
    let boundary = 10. / std::f64::consts::LN_10 * (k1.ln_1p() - k1.ln());
    let candidate = D::new(63, width).unwrap();
    let actual_db = -20. * candidate.stop_edge_magnitude().unwrap().log10();
    // The last-order ideal degree endpoint need not be admitted by actual
    // rounded coefficients. Explicitly prove strict admission/refusal on each
    // side of its measured edge, retaining the independent theoretical ceiling.
    let admitted_db = actual_db.min(boundary) - 1e-6;
    let d = D::from_attenuation(width, admitted_db).unwrap();
    assert_eq!(d.order(), 63);
    assert!(d.stop_edge_magnitude().unwrap() <= 10f64.powf(-admitted_db / 20.));
    assert!(D::from_attenuation(width, actual_db.max(boundary) + 1e-6).is_err());
    if actual_db < boundary - 1e-6 {
        assert!(D::from_attenuation(width, boundary - 1e-6).is_err());
    }
    assert!(D::from_attenuation(width, boundary + 1e-6).is_err());
}
fn amplitude(samples: &[f64], f: f64) -> f64 {
    let (re, im) = samples
        .iter()
        .enumerate()
        .fold((0., 0.), |(re, im), (i, &x)| {
            let (s, c) = (2. * PI * f * i as f64).sin_cos();
            (re + x * c, im + x * s)
        });
    2. * re.hypot(im) / samples.len() as f64
}
#[test]
fn actual_interpolation_images_decimation_alias_and_nonlinear_cascade_tones_are_measured() {
    use ravet_signal::{
        ConfiguredOversampler as O, ConfiguredOversampler32 as O32, OversamplingFactor as R,
    };
    let d = D::new(15, 0.05).unwrap();
    let q = d.quantize_f32().unwrap();
    let low = 112. / 256.;
    let high = low / 2.;
    let image = 0.5 - high;
    let mut p = P::new(d, d);
    let mut p32 = P32::new(q, q);
    let mut signal = Vec::new();
    let mut signal32 = Vec::new();
    for i in 0..32768 {
        let x = (2. * PI * low * i as f64).sin();
        let up = p.upsample(x);
        let up32 = p32.upsample(x as f32);
        if i >= 16384 {
            signal.extend(up);
            signal32.extend(up32.map(f64::from));
        }
    }
    close(
        amplitude(&signal, high),
        d.magnitude_at(high).unwrap(),
        3e-11,
    );
    close(
        amplitude(&signal, image),
        d.magnitude_at(image).unwrap(),
        3e-11,
    );
    close(
        amplitude(&signal32, high),
        q.magnitude_at(high).unwrap(),
        3e-7,
    );
    close(
        amplitude(&signal32, image),
        q.magnitude_at(image).unwrap(),
        3e-7,
    );
    assert!(amplitude(&signal32, image) < 3e-5);
    for f in [high, image] {
        let mut p = P::new(d, d);
        let mut p32 = P32::new(q, q);
        let mut output = Vec::new();
        let mut output32 = Vec::new();
        for i in 0..32768 {
            let pair = [
                (2. * PI * f * (2 * i) as f64).sin(),
                (2. * PI * f * (2 * i + 1) as f64).sin(),
            ];
            let y = p.downsample(pair);
            let y32 = p32.downsample(pair.map(|x| x as f32));
            if i >= 16384 {
                output.push(y);
                output32.push(y32 as f64);
            }
        }
        close(amplitude(&output, low), d.magnitude_at(f).unwrap(), 3e-11);
        close(amplitude(&output32, low), q.magnitude_at(f).unwrap(), 3e-7);
    }
    let f = 53. / 256.;
    let alias = 97. / 256.;
    let mut one = O::new(R::One, d, d);
    let mut eight = O::new(R::Eight, d, d);
    let mut eight32 = O32::new(R::Eight, q, q);
    let mut baseline = Vec::new();
    let mut over = Vec::new();
    let mut over32 = Vec::new();
    for i in 0..32768 {
        let x = (2. * PI * f * i as f64).sin();
        let a = one.process_sample(x, |x| (3. * x).tanh());
        let b = eight.process_sample(x, |x| (3. * x).tanh());
        let c = eight32.process_sample(x as f32, |x| (3. * x).tanh());
        if i >= 16384 {
            baseline.push(a);
            over.push(b);
            over32.push(c as f64);
        }
    }
    assert!(amplitude(&baseline, alias) > 0.1);
    assert!(amplitude(&over, alias) < 1e-4);
    assert!(amplitude(&over32, alias) < 1e-4);
    assert!(amplitude(&over, f) > 1.);
}
