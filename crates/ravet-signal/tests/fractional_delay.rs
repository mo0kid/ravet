use ravet_signal::{DelayInterpolation as K, FractionalDelay, FractionalDelayError as E};

const KINDS: [K; 4] = [K::Integer, K::Linear, K::Lagrange3rd, K::Thiran1st];
fn at(source: &[f64], time: usize, delay: usize) -> f64 {
    time.checked_sub(delay).map_or(0., |j| source[j])
}
fn polynomial(values: &[f64], x: f64) -> f64 {
    values
        .iter()
        .enumerate()
        .map(|(j, v)| {
            v * (0..values.len())
                .filter(|&i| i != j)
                .map(|i| (x - i as f64) / (j as f64 - i as f64))
                .product::<f64>()
        })
        .sum()
}
fn oracle(kind: K, source: &[f64], delays: &[f64]) -> Vec<f64> {
    let mut previous_v = 0.;
    let mut previous_y = 0.;
    source
        .iter()
        .enumerate()
        .map(|(t, _)| {
            let delay = delays[t];
            let n = delay.floor() as usize;
            let f = delay - n as f64;
            let v = at(source, t, n);
            let y = if f == 0. || kind == K::Integer {
                v
            } else {
                match kind {
                    K::Integer => unreachable!(),
                    K::Linear => v * (1. - f) + at(source, t, n + 1) * f,
                    K::Lagrange3rd => polynomial(
                        &[
                            v,
                            at(source, t, n + 1),
                            at(source, t, n + 2),
                            at(source, t, n + 3),
                        ],
                        f,
                    ),
                    K::Thiran1st => {
                        let a = (1. - f) / (1. + f);
                        previous_v + a * (v - previous_y)
                    }
                }
            };
            previous_v = v;
            previous_y = y;
            y
        })
        .collect()
}
fn close(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b}, delta {}",
        (a - b).abs()
    );
}
#[test]
fn all_policies_match_absolute_history_oracles_at_wraps_boundaries_and_block_partitions() {
    let source: Vec<_> = (0..521)
        .map(|j| ((j * 7919 % 53) as f64 - 26.) / 23.)
        .collect();
    for kind in KINDS {
        for maximum in [0, 1, 2, 3, 7, 13] {
            for delay in [
                0.,
                0.25,
                0.5,
                0.75,
                1.,
                2.25,
                maximum as f64 - 0.125,
                maximum as f64,
            ] {
                if delay < 0. || delay > maximum as f64 {
                    continue;
                }
                let mut whole = FractionalDelay::new(maximum, kind).unwrap();
                assert_eq!(whole.maximum_delay_samples(), maximum);
                assert_eq!(whole.interpolation(), kind);
                assert_eq!(whole.delay_samples(), 0.);
                whole.set_delay_samples(delay).unwrap();
                let mut a = source.clone();
                whole.process_in_place(&mut a);
                let expected = oracle(kind, &source, &vec![delay; source.len()]);
                for (&a, &b) in a.iter().zip(&expected) {
                    close(a, b, 8e-15);
                }
                let mut split = FractionalDelay::new(maximum, kind).unwrap();
                split.set_delay_samples(delay).unwrap();
                let mut b = vec![99.; source.len()];
                let mut start = 0;
                let mut turn = 0;
                while start < source.len() {
                    let end = (start + [1, 17, 2, 31, 3][turn % 5]).min(source.len());
                    split.process_separate(&[], &mut []).unwrap();
                    split
                        .process_separate(&source[start..end], &mut b[start..end])
                        .unwrap();
                    start = end;
                    turn += 1;
                }
                assert_eq!(a, b, "{kind:?} max{maximum} delay{delay}");
                split.reset();
                let mut again = source.clone();
                split.process_in_place(&mut again);
                assert_eq!(again, a);
            }
        }
    }
}

#[test]
fn causal_cubic_reproduces_polynomials_and_explicit_impulse_overshoot() {
    for delay in [0.125, 0.5, 0.75, 1.999, 4.5, 8.] {
        for degree in 0..=3 {
            let input: Vec<_> = (0..149)
                .map(|j| ((j as f64 - 70.) / 149.).powi(degree))
                .collect();
            let mut line = FractionalDelay::new(8, K::Lagrange3rd).unwrap();
            line.set_delay_samples(delay).unwrap();
            let mut actual = input.clone();
            line.process_in_place(&mut actual);
            for (t, &sample) in actual.iter().enumerate().skip(12) {
                close(
                    sample,
                    ((t as f64 - delay - 70.) / 149.).powi(degree),
                    5e-16,
                );
            }
        }
    }
    let mut line = FractionalDelay::new(3, K::Lagrange3rd).unwrap();
    line.set_delay_samples(0.5).unwrap();
    let mut impulse = [1., 0., 0., 0., 0.];
    line.process_in_place(&mut impulse);
    assert_eq!(impulse, [5. / 16., 15. / 16., -5. / 16., 1. / 16., 0.]);
    line.reset();
    line.set_delay_samples(0.75).unwrap();
    let mut impulse = [1., 0., 0., 0., 0.];
    line.process_in_place(&mut impulse);
    assert_eq!(impulse[1], 135. / 128.);
    assert!(impulse[1] > 1.);
}

#[test]
fn retuning_across_integer_branches_retains_recursive_history_and_integer_bypass_updates_it() {
    let input: Vec<_> = (0..997).map(|j| (j as f64 * 0.17).sin() + 0.03).collect();
    let delays: Vec<_> = (0..input.len())
        .map(|j| [0., 0.25, 2.9, 1., 1.001, 7., 6.875, 0.5][j / 11 % 8])
        .collect();
    for kind in KINDS {
        let expected = oracle(kind, &input, &delays);
        let mut line = FractionalDelay::new(7, kind).unwrap();
        for t in 0..input.len() {
            line.set_delay_samples(delays[t]).unwrap();
            close(line.process_sample(input[t]), expected[t], 2e-14);
        }
        assert_eq!(line.delay_samples(), *delays.last().unwrap());
    }
    let mut line = FractionalDelay::new(3, K::Thiran1st).unwrap();
    line.set_delay_samples(0.5).unwrap();
    close(line.process_sample(1.), 1. / 3., 1e-16);
    line.set_delay_samples(0.).unwrap();
    assert_eq!(line.process_sample(4.), 4.);
    line.set_delay_samples(0.5).unwrap();
    close(line.process_sample(2.), 4. + (2. - 4.) / 3., 1e-15);
}

fn transfer(kind: K, d: f64, w: f64) -> (f64, f64) {
    // Independent complex division using scalar pairs, not kernel output.
    let n = d.floor();
    let f = d - n;
    let (r, i) = match kind {
        K::Integer => (1., 0.),
        K::Linear => (1. - f + f * w.cos(), -f * w.sin()),
        K::Lagrange3rd => {
            let weights: [f64; 4] = std::array::from_fn(|j| {
                (0..4)
                    .filter(|&k| k != j)
                    .map(|k| (f - k as f64) / (j as f64 - k as f64))
                    .product()
            });
            let r = weights
                .iter()
                .enumerate()
                .map(|(j, &h)| h * (w * j as f64).cos())
                .sum();
            let i = weights
                .iter()
                .enumerate()
                .map(|(j, &h)| -h * (w * j as f64).sin())
                .sum();
            (r, i)
        }
        K::Thiran1st if f == 0. => (1., 0.),
        K::Thiran1st => {
            let a = (1. - f) / (1. + f);
            let nr = a + w.cos();
            let ni = -w.sin();
            let dr = 1. + a * w.cos();
            let di = -a * w.sin();
            let norm = dr * dr + di * di;
            ((nr * dr + ni * di) / norm, (ni * dr - nr * di) / norm)
        }
    };
    (
        r * (w * n).cos() + i * (w * n).sin(),
        i * (w * n).cos() - r * (w * n).sin(),
    )
}
#[test]
fn fixed_profiles_match_transfer_oracles_and_thiran_geometric_tail_not_a_finite_flush_claim() {
    for kind in KINDS {
        for delay in [0., 0.1, 0.5, 0.9, 2.5, 7.] {
            let mut line = FractionalDelay::new(7, kind).unwrap();
            line.set_delay_samples(delay).unwrap();
            let n = delay.floor() as usize;
            let f = delay - n as f64;
            let a = (1. - f) / (1. + f);
            let len = 4096;
            let mut impulse = vec![0.; len];
            impulse[0] = 1.;
            line.process_in_place(&mut impulse);
            if kind == K::Thiran1st && f > 0. {
                close(impulse[n], a, 2e-16);
                for j in 1..80 {
                    close(
                        impulse[n + j],
                        (1. - a * a) * (-a).powi(j as i32 - 1),
                        2e-16,
                    );
                }
                let energy = impulse.iter().map(|v| v * v).sum::<f64>();
                let remainder = (1. - a * a) * a.powi(2 * (len - n - 1) as i32);
                close(energy + remainder, 1., 2e-14);
            }
            for w in [0., 0.001, 0.17, 0.63, 1.4, std::f64::consts::PI] {
                let actual = (
                    impulse
                        .iter()
                        .enumerate()
                        .map(|(j, &h)| h * (w * j as f64).cos())
                        .sum::<f64>(),
                    impulse
                        .iter()
                        .enumerate()
                        .map(|(j, &h)| -h * (w * j as f64).sin())
                        .sum::<f64>(),
                );
                let expected = transfer(kind, delay, w);
                close(actual.0, expected.0, 3e-14);
                close(actual.1, expected.1, 3e-14);
                if kind == K::Thiran1st {
                    close(actual.0.hypot(actual.1), 1., 3e-14);
                }
            }
            if kind == K::Thiran1st {
                let w = 1e-6;
                let (r, i) = transfer(kind, delay, w);
                close(-i.atan2(r) / w, delay, 1e-9);
            }
        }
    }
    // A small-f tail remains after a finite block; retain its exact remainder.
    let f = 1e-4;
    let a = (1. - f) / (1. + f);
    let len = 1024;
    let mut line = FractionalDelay::new(1, K::Thiran1st).unwrap();
    line.set_delay_samples(f).unwrap();
    let mut impulse = vec![0.; len];
    impulse[0] = 1.;
    line.process_in_place(&mut impulse);
    let remainder = (1. - a * a) * a.powi(2 * (len - 1) as i32);
    assert!(remainder > 0.0002);
    close(
        impulse.iter().map(|v| v * v).sum::<f64>() + remainder,
        1.,
        2e-14,
    );
    assert!(impulse[len - 1].abs() > 0.0003);
}

#[test]
fn all_preflight_failures_preserve_control_output_and_live_history_with_nonfinite_reset_policy() {
    for kind in KINDS {
        let mut line = FractionalDelay::new(7, kind).unwrap();
        let mut reference = FractionalDelay::new(7, kind).unwrap();
        line.set_delay_samples(2.5).unwrap();
        reference.set_delay_samples(2.5).unwrap();
        for x in [1., 2., 3., 4., 5.] {
            assert_eq!(line.process_sample(x), reference.process_sample(x));
        }
        for bad in [-1., 7.000001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(line.set_delay_samples(bad), Err(E::InvalidDelay));
            assert_eq!(line.delay_samples(), 2.5);
        }
        if kind == K::Thiran1st {
            for bad in [f64::from_bits(1), f64::MIN_POSITIVE, 1e-18] {
                assert_eq!(line.set_delay_samples(bad), Err(E::InvalidDelay));
                assert_eq!(line.delay_samples(), 2.5);
            }
        }
        let mut output = [97.; 3];
        assert_eq!(
            line.process_separate(&[1., 2.], &mut output),
            Err(E::LengthMismatch)
        );
        assert_eq!(output, [97.; 3]);
        assert_eq!(
            line.process_separate(&[1., 2., 3.], &mut output[..2]),
            Err(E::LengthMismatch)
        );
        assert_eq!(output, [97.; 3]);
        for j in 0..129 {
            let x = (j as f64 * 0.19).sin();
            assert_eq!(line.process_sample(x), reference.process_sample(x));
        }
        line.set_delay_samples(0.5).unwrap();
        line.process_sample(f64::NAN);
        let next = line.process_sample(1.);
        if kind == K::Integer {
            assert_eq!(next, 1.);
        } else {
            assert!(next.is_nan());
        }
        line.reset();
        reference.reset();
        reference.set_delay_samples(0.5).unwrap();
        for x in [1., 2., 3., 4.] {
            assert_eq!(line.process_sample(x), reference.process_sample(x));
        }
        // Exact integer support ignores unrelated NaN history, while retaining it.
        line.set_delay_samples(0.).unwrap();
        assert_eq!(line.process_sample(9.), 9.);
    }
}

#[test]
fn capacity_rejection_precedes_allocation_and_prepared_callback_edits_reset_blocks_are_heap_free() {
    let counts = allocation_counter::measure(|| {
        for size in [4_194_301, usize::MAX] {
            assert_eq!(
                FractionalDelay::new(size, K::Linear).unwrap_err(),
                E::Capacity
            );
        }
    });
    assert_eq!(counts.count_total, 0);
    let mut maximum = FractionalDelay::new(4_194_300, K::Thiran1st).unwrap();
    maximum.set_delay_samples(4_194_300.).unwrap();
    assert_eq!(maximum.process_sample(1.), 0.);
    maximum.reset();
    assert_eq!(maximum.delay_samples(), 4_194_300.);
    drop(maximum);
    let mut lines = KINDS.map(|k| FractionalDelay::new(41, k).unwrap());
    let mut output = [99.; 31];
    let counts = allocation_counter::measure(|| {
        for j in 0..1000 {
            for line in &mut lines {
                line.set_delay_samples([0., 0.125, 1., 5.75, 41.][j % 5])
                    .unwrap();
                std::hint::black_box(line.process_sample(j as f64));
                let input = [j as f64; 31];
                line.process_separate(&input, &mut output).unwrap();
                line.process_in_place(&mut output);
                line.process_separate(&[], &mut []).unwrap();
                if j % 13 == 0 {
                    line.reset();
                }
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(counts.bytes_current, 0);
}

#[test]
fn actual_stereo_foundation_graph_processor_preserves_channel_history_preflight_reset_and_heap() {
    use ravet_foundation::{
        AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
        Processor, Transport,
    };
    use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct StereoDelay {
        lines: [FractionalDelay; 2],
        config: Option<ProcessConfig>,
        frames: Arc<AtomicUsize>,
    }
    impl Processor<f64> for StereoDelay {
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
    for kind in KINDS {
        let delays = [0.75, 2.5];
        let mut lines = std::array::from_fn(|_| FractionalDelay::new(7, kind).unwrap());
        for channel in 0..2 {
            lines[channel].set_delay_samples(delays[channel]).unwrap();
        }
        let frames = Arc::new(AtomicUsize::new(0));
        let node = GraphNode {
            id: NodeId(7),
            layout: AudioLayout::STEREO,
            processor: Box::new(StereoDelay {
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
            oracle(
                kind,
                &input[channel],
                &vec![delays[channel]; input[channel].len()],
            )
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
