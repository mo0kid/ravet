use ravet_signal::{InterpolationError, InterpolationKind as K, StreamInterpolator};

const KINDS: [K; 5] = [
    K::ZeroOrderHold,
    K::Linear,
    K::CatmullRom,
    K::Lagrange3rd,
    K::Lagrange4th,
];
const RATES: [(u32, u32); 8] = [
    (1, 16),
    (16, 1),
    (1, 1),
    (3, 5),
    (5, 3),
    (44100, 48000),
    (48000, 44100),
    (u32::MAX, u32::MAX - 1),
];
fn source_value(input: &[f64], index: i128) -> f64 {
    if index < 0 { 0. } else { input[index as usize] }
}
fn lagrange(values: &[f64], x: f64) -> f64 {
    values
        .iter()
        .enumerate()
        .map(|(i, value)| {
            value
                * (0..values.len())
                    .filter(|&j| j != i)
                    .map(|j| (x - j as f64) / (i as f64 - j as f64))
                    .product::<f64>()
        })
        .sum()
}
fn oracle(kind: K, input: &[f64], input_rate: u32, output_rate: u32) -> Vec<f64> {
    if input.is_empty() {
        return vec![];
    }
    let count = 1 + (input.len() as u128 - 1) * u128::from(output_rate) / u128::from(input_rate);
    (0..count)
        .map(|j| {
            let numerator = j * u128::from(input_rate);
            let divisor = u128::from(output_rate);
            let ceil = (numerator / divisor + u128::from(numerator % divisor != 0)) as i128;
            let f = ((divisor - numerator % divisor) % divisor) as f64 / f64::from(output_rate);
            let at = |offset| source_value(input, ceil - offset);
            match kind {
                K::ZeroOrderHold => at(i128::from(f > 0.)),
                K::Linear => (1. - f) * at(0) + f * at(1),
                K::CatmullRom => {
                    let t = 1. - f;
                    let h00 = 2. * t.powi(3) - 3. * t.powi(2) + 1.;
                    let h10 = t.powi(3) - 2. * t.powi(2) + t;
                    let h01 = -2. * t.powi(3) + 3. * t.powi(2);
                    let h11 = t.powi(3) - t.powi(2);
                    h00 * at(2)
                        + h10 * (at(1) - at(3)) / 2.
                        + h01 * at(1)
                        + h11 * (at(0) - at(2)) / 2.
                }
                K::Lagrange3rd => lagrange(&[at(3), at(2), at(1), at(0)], 2. - f),
                K::Lagrange4th => lagrange(&[at(4), at(3), at(2), at(1), at(0)], 2. - f),
            }
        })
        .collect()
}
fn render(mut stream: StreamInterpolator, input: &[f64], partition: bool) -> Vec<f64> {
    let mut consumed = 0;
    let mut result = Vec::new();
    let mut turn = 0;
    loop {
        let end = (consumed
            + if partition {
                [1, 7, 2, 29, 3][turn % 5]
            } else {
                input.len()
            })
        .min(input.len());
        let capacity = if partition {
            [1, 13, 2, 31, 5][turn % 5]
        } else {
            4096
        };
        let mut output = vec![987.; capacity + 2];
        if partition {
            let p = stream.process(&input[consumed..end], &mut []);
            assert_eq!((p.consumed, p.produced), (0, 0));
        }
        let p = stream.process(&input[consumed..end], &mut output[..capacity]);
        assert!(p.consumed <= end - consumed && p.produced <= capacity);
        assert!(output[p.produced..].iter().all(|&v| v == 987.));
        consumed += p.consumed;
        result.extend_from_slice(&output[..p.produced]);
        turn += 1;
        if p.consumed == 0 && p.produced == 0 {
            assert_eq!(consumed, input.len());
            break;
        }
    }
    result
}
fn close(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() < tolerance,
        "{a} != {b} (error {})",
        (a - b).abs()
    );
}

#[test]
fn every_family_matches_independent_coordinate_and_basis_oracles_under_partitions() {
    for kind in KINDS {
        for (i, o) in RATES {
            for n in [0, 1, 2, 5, 17, 313] {
                let input: Vec<_> = (0..n)
                    .map(|j| ((j * 7919 % 41) as f64 - 20.) / 17.)
                    .collect();
                let stream = StreamInterpolator::new_integer_rates(kind, i, o).unwrap();
                let whole = render(stream.clone(), &input, false);
                let chunks = render(stream, &input, true);
                assert_eq!(whole, chunks, "kind={kind:?} rates={i}/{o} n={n}");
                let expected = oracle(kind, &input, i, o);
                assert_eq!(whole.len(), expected.len());
                for (&a, &b) in whole.iter().zip(&expected) {
                    close(a, b, 3e-14);
                }
            }
        }
    }
}

#[test]
fn polynomial_reproduction_unity_latency_and_initial_zero_history_are_explicit() {
    for kind in KINDS {
        let stream = StreamInterpolator::new_integer_rates(kind, 3, 7).unwrap();
        assert_eq!(stream.kind(), kind);
        let delay = stream.latency_input_samples();
        let degree = match kind {
            K::ZeroOrderHold => 0,
            K::Linear => 1,
            K::CatmullRom => 2,
            K::Lagrange3rd => 3,
            K::Lagrange4th => 4,
        };
        for power in 0..=degree {
            let input: Vec<_> = (0..200)
                .map(|j| ((j as f64 - 50.) / 200.).powi(power))
                .collect();
            let output = render(stream.clone(), &input, true);
            for (j, &sample) in output.iter().enumerate().skip(24) {
                let t = j as f64 * 3. / 7. - delay;
                close(sample, ((t - 50.) / 200.).powi(power), 4e-15);
            }
        }
        let input = [1., 2., -3., 4., 5., 6., 7.];
        let output = render(
            StreamInterpolator::new_integer_rates(kind, 1, 1).unwrap(),
            &input,
            true,
        );
        let delay = delay as usize;
        assert_eq!(&output[..delay], vec![0.; delay]);
        assert_eq!(&output[delay..], &input[..input.len() - delay]);
    }
    // Cardinal cubic's local slopes overshoot a step; no output clamp.
    let overshoot = render(
        StreamInterpolator::new_integer_rates(K::CatmullRom, 1, 8).unwrap(),
        &[0., 0., 1., 1., 1., 1.],
        false,
    );
    assert!(overshoot.iter().any(|&v| v < 0.) && overshoot.iter().any(|&v| v > 1.));
}

#[test]
fn common_hold_future_bracket_pending_empty_input_and_explicit_padding() {
    let mut stream = StreamInterpolator::new_integer_rates(K::ZeroOrderHold, 1, 4).unwrap();
    let mut output = [99.; 2];
    let p = stream.process(&[10.], &mut output);
    assert_eq!((p.consumed, p.produced), (1, 1));
    assert_eq!(output, [10., 99.]);
    // A second source bracket is needed, although floor(t) still uses 10.
    let p = stream.process(&[], &mut output);
    assert_eq!((p.consumed, p.produced), (0, 0));
    let p = stream.process(&[20.], &mut output[..1]);
    assert_eq!((p.consumed, p.produced), (1, 1));
    assert_eq!(output[0], 10.);
    let mut pending = [99.; 4];
    let p = stream.process(&[], &mut pending);
    assert_eq!((p.consumed, p.produced), (0, 3));
    assert_eq!(pending, [10., 10., 20., 99.]);
    for kind in KINDS {
        let without = render(
            StreamInterpolator::new_integer_rates(kind, 1, 4).unwrap(),
            &[1., 2.],
            false,
        );
        let padded = render(
            StreamInterpolator::new_integer_rates(kind, 1, 4).unwrap(),
            &[1., 2., 0., 0., 0., 0.],
            false,
        );
        assert_eq!(without.len(), 5);
        assert_eq!(padded.len(), 21);
        assert_eq!(padded, oracle(kind, &[1., 2., 0., 0., 0., 0.], 1, 4));
    }
}

#[test]
fn additive_preflight_suffix_gain_zero_and_reset_preserve_exact_state() {
    for kind in KINDS {
        let original = StreamInterpolator::new_integer_rates(kind, 3, 8).unwrap();
        let mut stream = original.clone();
        let mut output = [-0.; 37];
        for gain in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                stream.process_add(&[1., 2.], &mut output, gain),
                Err(InterpolationError::InvalidGain)
            );
            assert!(output.iter().all(|v| v.to_bits() == (-0f64).to_bits()));
        }
        let source = [2., -1., 3., 4.];
        let expected = render(original.clone(), &source, false);
        let p = stream.process_add(&source, &mut output, 0.25).unwrap();
        assert_eq!(p.produced, expected.len());
        for j in 0..p.produced {
            assert_eq!(output[j], expected[j] * 0.25);
        }
        assert!(
            output[p.produced..]
                .iter()
                .all(|v| v.to_bits() == (-0f64).to_bits())
        );
        stream.reset();
        assert_eq!(render(stream.clone(), &source, true), expected);
        let mut replace = stream.clone();
        let mut untouched = [-0.; 2];
        let p = stream
            .process_add(&[f64::NAN, 2.], &mut untouched, 0.)
            .unwrap();
        let mut discarded = [0.; 2];
        assert_eq!(p, replace.process(&[f64::NAN, 2.], &mut discarded));
        assert!(untouched.iter().all(|v| v.to_bits() == (-0f64).to_bits()));
        let mut a = [0.; 23];
        let mut b = [0.; 23];
        let pa = stream.process(&source, &mut a);
        let pb = replace.process(&source, &mut b);
        assert_eq!(pa, pb);
        for j in 0..pa.produced {
            assert!(a[j] == b[j] || a[j].is_nan() && b[j].is_nan());
        }
        stream.reset();
        assert_eq!(render(stream, &source, false), expected);
    }
}

#[test]
fn rates_and_floating_profile_boundaries_are_validated_without_unintended_exact_claim() {
    for kind in KINDS {
        for (i, o) in [(0, 1), (1, 0), (1, 17), (17, 1), (u32::MAX, 1)] {
            assert_eq!(
                StreamInterpolator::new_integer_rates(kind, i, o).unwrap_err(),
                InterpolationError::InvalidRates
            );
        }
        for (i, o) in [
            (0., 1.),
            (-1., 1.),
            (1., 0.),
            (f64::NAN, 1.),
            (1., f64::INFINITY),
            (1., 17.),
            (17., 1.),
            (f64::MIN_POSITIVE, f64::MAX),
        ] {
            assert_eq!(
                StreamInterpolator::new(kind, i, o).unwrap_err(),
                InterpolationError::InvalidRates
            );
        }
        for (i, o) in [(1, 16), (16, 1), (3, 8), (48000, 44100)] {
            let input: Vec<_> = (0..503).map(|j| (j as f64 * 0.13).sin()).collect();
            let exact = render(
                StreamInterpolator::new_integer_rates(kind, i, o).unwrap(),
                &input,
                true,
            );
            let float = render(
                StreamInterpolator::new(kind, f64::from(i), f64::from(o)).unwrap(),
                &input,
                true,
            );
            // Hold can differ at rounded integer boundaries; compare non-boundary points.
            for j in 0..exact.len().min(float.len()) {
                if kind != K::ZeroOrderHold
                    || !(j as u64 * u64::from(i)).is_multiple_of(u64::from(o))
                {
                    close(exact[j], float[j], 2e-10);
                }
            }
        }
    }
}

#[test]
fn low_frequency_reconstruction_and_downsampling_alias_are_measured_truthfully() {
    for kind in KINDS {
        let source: Vec<_> = (0..4096)
            .map(|j| (std::f64::consts::TAU * 0.005 * j as f64).sin())
            .collect();
        let stream = StreamInterpolator::new_integer_rates(kind, 3, 7).unwrap();
        let delay = stream.latency_input_samples();
        let output = render(stream, &source, true);
        let mse = output
            .iter()
            .enumerate()
            .skip(40)
            .map(|(j, &v)| {
                (v - (std::f64::consts::TAU * 0.005 * (j as f64 * 3. / 7. - delay)).sin()).powi(2)
            })
            .sum::<f64>()
            / (output.len() - 40) as f64;
        let rms = mse.sqrt();
        println!("{kind:?} tone0.005 reconstruction_rmse={rms}");
        let limit = match kind {
            K::ZeroOrderHold => 0.023,
            K::Linear => 0.00013,
            K::CatmullRom => 2e-6,
            K::Lagrange3rd => 3e-8,
            K::Lagrange4th => 4e-10,
        };
        assert!(rms < limit, "{kind:?}: {rms}");
        let source: Vec<_> = (0..8192)
            .map(|j| (std::f64::consts::TAU * 0.375 * j as f64).sin())
            .collect();
        let output = render(
            StreamInterpolator::new_integer_rates(kind, 2, 1).unwrap(),
            &source,
            false,
        );
        let rms =
            (output[10..].iter().map(|v| v * v).sum::<f64>() / (output.len() - 10) as f64).sqrt();
        // Delayed exact source nodes alias 0.375 input cycles into 0.25 output cycles.
        println!("{kind:?} downsampled_out_of_band_rms={rms}");
        assert!(rms > 0.70 && rms < 0.71);
    }
}

#[test]
fn whole_stack_lifecycle_and_callback_paths_never_allocate_or_deallocate() {
    let counts = allocation_counter::measure(|| {
        for kind in KINDS {
            let mut stream = StreamInterpolator::new_integer_rates(kind, 44100, 48000).unwrap();
            let mut output = [0.; 31];
            for iteration in 0..1000 {
                let source = [iteration as f64; 17];
                std::hint::black_box(stream.process(&source, &mut output));
                std::hint::black_box(stream.process_add(&source, &mut output, 0.3).unwrap());
                std::hint::black_box(stream.process(&[], &mut output));
                std::hint::black_box(stream.process(&source, &mut []));
                if iteration % 7 == 0 {
                    stream.reset();
                }
            }
            std::hint::black_box(stream.clone());
            std::hint::black_box(StreamInterpolator::new(kind, 1., 2.).unwrap());
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(counts.bytes_current, 0);
}

#[test]
fn actual_stereo_foundation_sources_and_prepared_graph_fir_consume_exact_prefixes() {
    use ravet_foundation::{
        AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
        Processor, Transport,
    };
    use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
    use ravet_signal::FirConvolver;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct SourceProcessor {
        source: Vec<f64>,
        cursor: usize,
        stream: StreamInterpolator,
        config: Option<ProcessConfig>,
        committed: Arc<AtomicUsize>,
    }
    impl Processor<f64> for SourceProcessor {
        fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
            self.config = None;
            if c.layout() != AudioLayout::MONO_INSTRUMENT || c.sample_rate() != 48000. {
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
            let output = b.audio.output_mut(0).unwrap();
            let p = self.stream.process(&self.source[self.cursor..], output);
            self.cursor += p.consumed;
            self.committed.fetch_add(p.consumed, Ordering::Relaxed);
            // This consumer's explicit finite-prefix policy: after pending data,
            // emit zero. It does not fabricate input or flush kernel tail.
            output[p.produced..].fill(0.);
            ProcessStatus::Continue
        }
        fn reset(&mut self) {
            self.cursor = 0;
            self.stream.reset();
            self.committed.store(0, Ordering::Relaxed);
        }
        fn deactivate(&mut self) {
            self.config = None;
        }
    }
    struct StereoFir {
        filters: [FirConvolver; 2],
        config: Option<ProcessConfig>,
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
                for j in 0..b.audio.frames() {
                    let input = b.audio.input(channel).unwrap()[j];
                    b.audio.output_mut(channel).unwrap()[j] =
                        self.filters[channel].process_sample(input);
                }
            }
            ProcessStatus::Continue
        }
        fn reset(&mut self) {
            for filter in &mut self.filters {
                filter.reset();
            }
        }
        fn deactivate(&mut self) {
            self.config = None;
        }
    }
    let taps = [0.5, -0.25, 0.125];
    for kind in KINDS {
        let sources: [Vec<f64>; 2] = std::array::from_fn(|channel| {
            (0..1021)
                .map(|j| {
                    if channel == 0 {
                        (j as f64 * 0.131).sin() + 0.01
                    } else {
                        (j as f64 * 0.071).cos() * 0.33
                    }
                })
                .collect()
        });
        let counters = [Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))];
        let mut nodes = Vec::new();
        for channel in 0..2 {
            nodes.push(GraphNode {
                id: NodeId(channel as u32),
                layout: AudioLayout::MONO_INSTRUMENT,
                processor: Box::new(SourceProcessor {
                    source: sources[channel].clone(),
                    cursor: 0,
                    stream: StreamInterpolator::new_integer_rates(kind, 44100, 48000).unwrap(),
                    config: None,
                    committed: counters[channel].clone(),
                }),
            });
        }
        nodes.push(GraphNode {
            id: NodeId(2),
            layout: AudioLayout::STEREO,
            processor: Box::new(StereoFir {
                filters: std::array::from_fn(|_| FirConvolver::new(&taps).unwrap()),
                config: None,
            }),
        });
        let mut connections = Vec::new();
        for channel in 0..2 {
            connections.push(Connection {
                source: Source::Node {
                    id: NodeId(channel as u32),
                    channel: 0,
                },
                destination: Destination::Node {
                    id: NodeId(2),
                    channel,
                },
                gain: 1.,
            });
            connections.push(Connection {
                source: Source::Node {
                    id: NodeId(2),
                    channel,
                },
                destination: Destination::Output(channel),
                gain: 1.,
            });
        }
        let mut graph = Graph::new(AudioLayout::STEREO_INSTRUMENT, nodes, &connections).unwrap();
        graph
            .prepare(ProcessConfig::new(48000., 67, 0, AudioLayout::STEREO_INSTRUMENT).unwrap())
            .unwrap();
        let expected: [Vec<f64>; 2] = std::array::from_fn(|channel| {
            let mut converted = oracle(kind, &sources[channel], 44100, 48000);
            converted.extend_from_slice(&[0.; 3]);
            (0..converted.len())
                .map(|n| {
                    taps.iter()
                        .enumerate()
                        .filter(|&(k, _)| k <= n)
                        .map(|(k, &tap)| tap * converted[n - k])
                        .sum()
                })
                .collect()
        });
        for repeat in 0..2 {
            let mut position = 0;
            let mut turn = 0;
            while position < expected[0].len() {
                let frames = [1, 67, 2, 31, 5][turn % 5].min(expected[0].len() - position);
                let mut output = [[91.; 69]; 2];
                let counts = allocation_counter::measure(|| {
                    let [left, right] = &mut output;
                    let mut channels = [left.as_mut_slice(), right.as_mut_slice()];
                    let audio = AudioBuffer::separate(&[], &mut channels, frames).unwrap();
                    assert_eq!(
                        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        GraphStatus::Continue
                    );
                });
                assert_eq!(counts.count_total, 0);
                assert_eq!(counts.count_current, 0);
                for channel in 0..2 {
                    for j in 0..frames {
                        close(output[channel][j], expected[channel][position + j], 4e-14);
                    }
                    assert!(output[channel][frames..].iter().all(|&v| v == 91.));
                }
                position += frames;
                turn += 1;
            }
            assert!(
                counters
                    .iter()
                    .all(|c| c.load(Ordering::Relaxed) == sources[0].len())
            );
            if repeat == 0 {
                graph.reset();
            }
        }
        graph.deactivate();
    }
}
