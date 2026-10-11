use ravet_signal::{
    EquirippleFirDesign as D, EquirippleFirResponse as R, EquirippleOptions as O, FirBand,
};
fn close(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() <= t, "{a} != {b}, delta {}", (a - b).abs());
}
#[test]
fn reduced_three_tap_halfband_has_independent_closed_minimax() {
    for width in [0.05, 0.1, 0.2, 0.4] {
        let d = D::halfband(3, width, O::default()).unwrap();
        let p = 0.25 - width / 2.;
        let c = (std::f64::consts::TAU * p).cos();
        let h = 1. / (2. * (1. + c));
        let delta = (1. - c) / (2. * (1. + c));
        let taps = d.design().coefficients();
        assert_eq!(taps[1], 0.5);
        close(taps[0], h, 3e-15);
        close(taps[2], h, 3e-15);
        close(d.normalized_grid_error(), delta, 4e-15);
        assert_eq!(d.extrema().len(), 2);
    }
}
#[test]
fn complete_named_response_profiles_converge_with_actual_error_receipts() {
    for taps in [15usize, 16, 31, 32, 63, 64] {
        for r in [
            R::LowPass {
                pass_hz: 7100.,
                stop_hz: 9200.,
            },
            R::HighPass {
                stop_hz: 5200.,
                pass_hz: 7300.,
            },
            R::BandPass {
                low_stop_hz: 3100.,
                low_pass_hz: 5200.,
                high_pass_hz: 12300.,
                high_stop_hz: 14400.,
            },
            R::BandStop {
                low_pass_hz: 3100.,
                low_stop_hz: 5200.,
                high_stop_hz: 12300.,
                high_pass_hz: 14400.,
            },
        ] {
            if taps.is_multiple_of(2) && matches!(r, R::HighPass { .. } | R::BandStop { .. }) {
                continue;
            }
            let d = D::new(r, 48000., taps, 1., 3., O::default())
                .unwrap_or_else(|e| panic!("{r:?} {taps} {e:?}"));
            assert_eq!(d.design().coefficients().len(), taps);
            assert_eq!(d.design().group_delay_samples(), (taps - 1) as f64 / 2.);
            assert!(d.scratch_capacity_bytes() <= 4 * 1024 * 1024);
            assert!(
                d.normalized_grid_error()
                    <= d.normalized_ripple() * (1. + 1e-8) + d.normalized_rounding_allowance()
            );
            assert_eq!(d.extrema().len(), taps.div_ceil(2) + 1);
        }
    }
}
#[test]
fn nonconstant_long_profiles_retain_actual_alternating_ripple() {
    for taps in [511usize, 512] {
        let r = R::LowPass {
            pass_hz: 7000.,
            stop_hz: 7240.,
        };
        let d = D::new(r, 48000., taps, 1., 2., O::default())
            .unwrap_or_else(|e| panic!("long {taps} {e:?}"));
        assert_eq!(d.extrema().len(), taps.div_ceil(2) + 1);
        assert!(d.normalized_ripple() > 1e-8);
        assert!(d.normalized_grid_error() < 0.02);
        assert!(
            d.normalized_grid_error()
                <= d.normalized_ripple() * (1. + 1e-8) + d.normalized_rounding_allowance()
        );
        assert!(
            d.design()
                .coefficients()
                .iter()
                .filter(|x| x.abs() > 1e-8)
                .count()
                > 200
        );
    }
}
#[test]
fn exact_signed_constant_zero_and_global_weight_scaling_preserve_actual_coefficients() {
    for gain in [-1.25, 0., 0.75] {
        let b = FirBand {
            low_hz: 0.,
            high_hz: 24000.,
            low_gain: gain,
            high_gain: gain,
            weight: 1.,
        };
        let d = D::from_bands(48000., 31, &[b], O::default()).unwrap();
        close(d.design().coefficients()[15], gain, 1e-12);
        assert!(d.normalized_grid_error() < 1e-12);
    }
    let r = R::LowPass {
        pass_hz: 7000.,
        stop_hz: 9500.,
    };
    let a = D::new(r, 48000., 31, 1., 2., O::default()).unwrap();
    let b = D::new(r, 48000., 31, 2., 4., O::default()).unwrap();
    assert_eq!(a.design().coefficients(), b.design().coefficients());
    assert_eq!(2. * a.weighted_grid_error(), b.weighted_grid_error());
}
fn centered(h: &[f64], f: f64) -> f64 {
    let m = (h.len() - 1) as f64 / 2.;
    h.iter()
        .enumerate()
        .map(|(i, &x)| x * (std::f64::consts::TAU * f * (i as f64 - m)).cos())
        .sum()
}
fn points(taps: usize, bands: &[FirBand], density: usize) -> Vec<(f64, f64, f64)> {
    let step = 1. / (2. * (taps + 1) as f64 * density as f64);
    let scale = bands.iter().map(|b| b.weight).fold(0f64, f64::max);
    bands
        .iter()
        .flat_map(|b| {
            let n = ((b.high_hz - b.low_hz) / step).ceil() as usize + 1;
            (0..n).map(move |i| {
                let t = i as f64 / (n - 1) as f64;
                (
                    (1. - t) * b.low_hz + t * b.high_hz,
                    (1. - t) * b.low_gain + t * b.high_gain,
                    b.weight / scale,
                )
            })
        })
        .collect()
}
fn solve(mut a: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Option<Vec<f64>> {
    let n = rhs.len();
    for col in 0..n {
        let pivot = (col..n)
            .max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))
            .unwrap();
        if a[pivot][col].abs() < 1e-13 {
            return None;
        }
        a.swap(col, pivot);
        rhs.swap(col, pivot);
        let p = a[col][col];
        for value in &mut a[col][col..] {
            *value /= p;
        }
        rhs[col] /= p;
        let pivot_rhs = rhs[col];
        let (upper, lower) = a.split_at_mut(col + 1);
        let pivot_row = &upper[col];
        for (row, row_rhs) in lower.iter_mut().zip(&mut rhs[col + 1..]) {
            let factor = row[col];
            for (value, &pivot) in row[col..].iter_mut().zip(&pivot_row[col..]) {
                *value -= factor * pivot;
            }
            *row_rhs -= factor * pivot_rhs;
        }
    }
    for col in (0..n).rev() {
        for row in 0..col {
            rhs[row] -= a[row][col] * rhs[col];
        }
    }
    Some(rhs)
}
fn features(taps: usize, f: f64, j: usize) -> f64 {
    let omega = std::f64::consts::TAU * f;
    let q = if taps.is_multiple_of(2) {
        (omega / 2.).cos()
    } else {
        1.
    };
    q * (j as f64 * omega).cos()
}
#[test]
fn small_variable_minimax_matches_independent_linear_program_vertex_enumeration() {
    for taps in [3usize, 4] {
        for bands in [
            vec![
                FirBand {
                    low_hz: 0.,
                    high_hz: 0.12,
                    low_gain: 1.,
                    high_gain: 1.,
                    weight: 1.,
                },
                FirBand {
                    low_hz: 0.20,
                    high_hz: 0.5,
                    low_gain: 0.,
                    high_gain: 0.,
                    weight: 3.,
                },
            ],
            vec![
                FirBand {
                    low_hz: 0.,
                    high_hz: 0.12,
                    low_gain: 0.3,
                    high_gain: 0.7,
                    weight: 1.,
                },
                FirBand {
                    low_hz: 0.20,
                    high_hz: 0.5,
                    low_gain: -0.5,
                    high_gain: if taps == 3 { -1.2 } else { 0. },
                    weight: 4.,
                },
            ],
        ] {
            let o = O {
                grid_density: 8,
                ..O::default()
            };
            let d = D::from_bands(1., taps, &bands, o).unwrap();
            let grid = points(taps, &bands, 8);
            assert_eq!(grid.len(), d.grid_points());
            let mut best = f64::INFINITY;
            for i in 0..grid.len() {
                for j in i + 1..grid.len() {
                    for k in j + 1..grid.len() {
                        if taps.is_multiple_of(2) && grid[k].0 == 0.5 {
                            continue;
                        }
                        let support = [grid[i], grid[j], grid[k]];
                        let matrix: Vec<_> = support
                            .iter()
                            .enumerate()
                            .map(|(index, &(f, _, w))| {
                                vec![
                                    w * features(taps, f, 0),
                                    w * features(taps, f, 1),
                                    if index.is_multiple_of(2) { 1. } else { -1. },
                                ]
                            })
                            .collect();
                        let Some(solution) = solve(
                            matrix,
                            support.iter().map(|&(_, target, w)| target * w).collect(),
                        ) else {
                            continue;
                        };
                        let delta = solution[2].abs();
                        if delta >= best {
                            continue;
                        }
                        let actual = grid
                            .iter()
                            .map(|&(f, target, w)| {
                                w * (target
                                    - solution[0] * features(taps, f, 0)
                                    - solution[1] * features(taps, f, 1))
                                .abs()
                            })
                            .fold(0f64, f64::max);
                        if actual <= delta + 2e-12 {
                            best = delta;
                        }
                    }
                }
            }
            assert!(best.is_finite());
            close(d.normalized_grid_error(), best, 5e-11);
            let actual = grid
                .iter()
                .map(|&(f, t, w)| w * (t - centered(d.design().coefficients(), f)).abs())
                .fold(0f64, f64::max);
            close(actual, d.normalized_grid_error(), 2e-15);
        }
    }
}
#[test]
fn constrained_halfband_structure_and_long_actual_response_are_independent() {
    for (taps, width) in [(7, 0.10), (31, 0.08), (63, 0.04), (127, 0.02), (511, 0.01)] {
        let d = D::halfband(taps, width, O::default())
            .unwrap_or_else(|e| panic!("half {taps} {width} {e:?}"));
        let h = d.design().coefficients();
        let m = taps / 2;
        assert_eq!(h[m], 0.5);
        for i in 0..taps {
            assert_eq!(h[i], h[taps - 1 - i]);
            if i != m && (i as isize - m as isize) % 2 == 0 {
                assert_eq!(h[i], 0.);
            }
        }
        close(centered(h, 0.25), 0.5, 4e-15);
        assert!(d.normalized_ripple() > 1e-9);
        assert_eq!(d.extrema().len(), (taps + 1) / 4 + 1);
        let p = 0.25 - width / 2.;
        let mut max = 0f64;
        for j in 0..=16384 {
            let f = p * j as f64 / 16384.;
            let a = centered(h, f);
            let b = centered(h, 0.5 - f);
            close(a + b, 1., 8e-14);
            max = max.max((1. - a).abs()).max(b.abs());
        }
        // Denser observations can exceed exchange grid ripple slightly, not a
        // continuous minimax certificate; require observed gap to stay small.
        assert!(
            max <= d.normalized_grid_error() * 1.01 + 1e-10,
            "{taps}: {max} vs{}",
            d.normalized_grid_error()
        );
        let reflected: Vec<_> = d.reflected_halfband_extrema().collect();
        assert_eq!(reflected.len(), d.extrema().len());
        for (a, b) in d.extrema().iter().zip(reflected.iter().rev()) {
            close(a.frequency_hz + b.frequency_hz, 0.5, 1e-16);
            assert_eq!(b.weighted_error, -centered(h, b.frequency_hz));
            close(
                a.weighted_error,
                -b.weighted_error,
                d.normalized_rounding_allowance(),
            );
        }
    }
}
#[test]
fn meaningful_long_profiles_have_independent_polynomial_basis_dual_lower_bounds() {
    for taps in [511usize, 512] {
        let d = D::new(
            R::LowPass {
                pass_hz: 7000.,
                stop_hz: 7240.,
            },
            48000.,
            taps,
            1.,
            2.,
            O::default(),
        )
        .unwrap();
        let support = d.extrema();
        let n = support.len();
        let vars = taps.div_ceil(2);
        let mut matrix = vec![vec![0.; n]; n];
        let mut rhs = vec![0.; n];
        rhs[vars] = 1.;
        for (i, e) in support.iter().enumerate() {
            let f = e.frequency_hz / 48000.;
            let w = if e.frequency_hz <= 7000. + 1e-8 {
                0.5
            } else {
                1.
            };
            let sign = e.weighted_error.signum();
            for (j, row) in matrix.iter_mut().enumerate().take(vars) {
                row[i] = sign * w * features(taps, f, j);
            }
            matrix[vars][i] = 1.;
        }
        let dual = solve(matrix, rhs).unwrap();
        assert!(dual.iter().all(|&x| x >= -3e-12));
        close(dual.iter().sum(), 1., 2e-12);
        let lower: f64 = support
            .iter()
            .zip(&dual)
            .map(|(e, &lambda)| {
                if e.frequency_hz <= 7000. + 1e-8 {
                    lambda * e.weighted_error.signum() * 0.5
                } else {
                    0.
                }
            })
            .sum();
        close(lower.abs(), d.normalized_ripple(), 1e-11);
        assert!(d.normalized_grid_error() - lower.abs() < 1e-10);
        for j in 0..vars {
            let moment: f64 = support
                .iter()
                .zip(&dual)
                .map(|(e, &l)| {
                    let w = if e.frequency_hz <= 7000. + 1e-8 {
                        0.5
                    } else {
                        1.
                    };
                    l * e.weighted_error.signum() * w * features(taps, e.frequency_hz / 48000., j)
                })
                .sum();
            assert!(moment.abs() < 4e-13);
        }
    }
}

#[test]
fn original_weight_restoration_forced_nyquist_and_underflow_metadata_are_truthful() {
    let r = R::LowPass {
        pass_hz: 7000.,
        stop_hz: 9500.,
    };
    let a = D::new(r, 48000., 31, 1., 2., O::default()).unwrap();
    let b = D::new(r, 48000., 31, 1e-323, 2e-323, O::default()).unwrap();
    assert_eq!(a.design().coefficients(), b.design().coefficients());
    assert_eq!(a.normalized_grid_error(), b.normalized_grid_error());
    assert!(b.normalized_grid_error() > 0.);
    assert_eq!(b.weighted_grid_error(), 0.);
    assert_eq!(b.weight_scale(), 2e-323);
    for (a, b) in a.extrema().iter().zip(b.extrema()) {
        assert_eq!(a.normalized_error, b.normalized_error);
        assert_eq!(a.weighted_error.signum(), b.weighted_error.signum());
    }
    let even = D::new(r, 48000., 32, 1., 2., O::default()).unwrap();
    assert!(even.forced_nyquist());
    assert!(even.extrema().iter().all(|e| e.frequency_hz < 24000.));
    assert!(!a.forced_nyquist());
}
#[test]
fn actual_stereo_foundation_graph_equiripple_fir_owners_preserve_delay_tail_and_heap() {
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
        D::new(
            R::LowPass {
                pass_hz: 5300.,
                stop_hz: 7200.,
            },
            48000.,
            18,
            1.,
            2.,
            O::default(),
        )
        .unwrap()
        .into_design(),
        D::new(
            R::HighPass {
                stop_hz: 4300.,
                pass_hz: 6000.,
            },
            48000.,
            31,
            1.,
            3.,
            O::default(),
        )
        .unwrap()
        .into_design(),
        D::new(
            R::BandPass {
                low_stop_hz: 2300.,
                low_pass_hz: 3900.,
                high_pass_hz: 8700.,
                high_stop_hz: 10300.,
            },
            48000.,
            64,
            1.,
            2.,
            O::default(),
        )
        .unwrap()
        .into_design(),
        D::halfband(63, 0.08, O::default()).unwrap().into_design(),
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
fn equiripple_responses_compose_with_actual_partitioned_latency_and_explicit_full_tail_drain() {
    use ravet_signal::PartitionedConvolver;
    let designs = [
        D::new(
            R::BandPass {
                low_stop_hz: 2300.,
                low_pass_hz: 3900.,
                high_pass_hz: 8700.,
                high_stop_hz: 10300.,
            },
            48000.,
            64,
            1.,
            2.,
            O::default(),
        )
        .unwrap()
        .into_design(),
        D::new(
            R::LowPass {
                pass_hz: 5300.,
                stop_hz: 7200.,
            },
            48000.,
            31,
            1.,
            3.,
            O::default(),
        )
        .unwrap()
        .into_design(),
        D::halfband(63, 0.08, O::default()).unwrap().into_design(),
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
#[test]
fn invalid_preflight_capacity_numerical_and_stalled_exchange_preserve_callers() {
    use ravet_signal::{EquirippleError as E, FirDesignError as B};
    let r = R::LowPass {
        pass_hz: 7000.,
        stop_hz: 7240.,
    };
    let valid = O::default();
    let counts = allocation_counter::measure(|| {
        for rate in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(D::new(r, rate, 31, 1., 2., valid).is_err());
        }
        for taps in [0, 1, 513, usize::MAX] {
            assert!(D::new(r, 48000., taps, 1., 2., valid).is_err());
        }
        for density in [0, 7, 65, usize::MAX] {
            assert!(
                D::new(
                    r,
                    48000.,
                    31,
                    1.,
                    2.,
                    O {
                        grid_density: density,
                        ..valid
                    }
                )
                .is_err()
            );
        }
        for iterations in [0, 129, usize::MAX] {
            assert!(
                D::new(
                    r,
                    48000.,
                    31,
                    1.,
                    2.,
                    O {
                        max_iterations: iterations,
                        ..valid
                    }
                )
                .is_err()
            );
        }
        for tolerance in [0., -1., 1e-13, 1.01e-3, f64::NAN, f64::INFINITY] {
            assert!(
                D::new(
                    r,
                    48000.,
                    31,
                    1.,
                    2.,
                    O {
                        relative_tolerance: tolerance,
                        ..valid
                    }
                )
                .is_err()
            );
        }
        for weight in [0., -1., 1e6 + 1., f64::NAN, f64::INFINITY] {
            assert!(D::new(r, 48000., 31, weight, 2., valid).is_err());
            assert!(D::new(r, 48000., 31, 1., weight, valid).is_err());
        }
        for width in [0., -1., 0.5, f64::NAN, f64::INFINITY] {
            assert!(D::halfband(31, width, valid).is_err());
        }
        for taps in [0, 1, 2, 4, 512, usize::MAX] {
            assert!(D::halfband(taps, 0.1, valid).is_err());
        }
        assert!(
            D::new(
                R::HighPass {
                    stop_hz: 4000.,
                    pass_hz: 7000.
                },
                48000.,
                32,
                1.,
                2.,
                valid
            )
            .is_err()
        );
        assert!(
            D::new(
                R::BandStop {
                    low_pass_hz: 3000.,
                    low_stop_hz: 5000.,
                    high_stop_hz: 10000.,
                    high_pass_hz: 13000.
                },
                48000.,
                32,
                1.,
                2.,
                valid
            )
            .is_err()
        );
        assert_eq!(
            D::halfband(31, 1e-320, valid).unwrap_err(),
            E::Design(B::Unrepresentable)
        );
        let whole = FirBand {
            low_hz: 0.,
            high_hz: 24000.,
            low_gain: 1.,
            high_gain: 1.,
            weight: 1.,
        };
        assert!(D::from_bands(48000., 32, &[whole], valid).is_err());
        assert!(D::from_bands(48000., 31, &[], valid).is_err());
        for b in [
            FirBand {
                low_hz: f64::NAN,
                ..whole
            },
            FirBand {
                low_hz: 24000.,
                ..whole
            },
            FirBand {
                high_gain: 16.01,
                ..whole
            },
            FirBand {
                low_gain: f64::INFINITY,
                ..whole
            },
        ] {
            assert!(D::from_bands(48000., 31, &[b], valid).is_err());
        }
        assert!(D::from_bands(48000., 31, &[whole, whole], valid).is_err());
        assert_eq!(
            D::from_bands(
                48000.,
                512,
                &[FirBand {
                    low_gain: 0.,
                    high_gain: 0.,
                    ..whole
                }],
                O {
                    grid_density: 64,
                    ..valid
                }
            )
            .unwrap_err(),
            E::Design(B::Capacity)
        );
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(
        D::new(
            r,
            48000.,
            63,
            1.,
            2.,
            O {
                max_iterations: 1,
                ..valid
            }
        )
        .unwrap_err(),
        E::NonConvergence { iterations: 1 }
    );
    let original = [FirBand {
        low_hz: 1e-9,
        high_hz: 2e-9,
        low_gain: 1.,
        high_gain: 1.,
        weight: 1.,
    }];
    let before = original;
    assert_eq!(
        D::from_bands(48000., 2, &original, valid).unwrap_err(),
        E::Design(B::Unrepresentable)
    );
    assert_eq!(original, before);
    let retained = D::new(r, 48000., 31, 1., 2., valid).unwrap();
    let copy = retained.design().coefficients().to_vec();
    assert!(
        D::new(
            r,
            48000.,
            31,
            1.,
            2.,
            O {
                max_iterations: 1,
                ..valid
            }
        )
        .is_err()
    );
    assert_eq!(retained.design().coefficients(), copy);
    let mut output = None;
    let allocated = allocation_counter::measure(|| {
        output = Some(D::new(r, 48000., 511, 1., 2., valid).unwrap());
    });
    let d = output.unwrap();
    assert!(allocated.bytes_total < 4 * 1024 * 1024);
    assert!(d.scratch_capacity_bytes() <= 4 * 1024 * 1024);
}
#[test]
fn varied_rates_transitions_weights_and_signed_linear_bands_have_actual_ripple() {
    for rate in [8000., 44100., 192000.] {
        for (pass_weight, stop_weight) in [(1., 3.), (7., 1.), (0.1, 10.)] {
            for taps in [31usize, 32, 63] {
                for r in [
                    R::LowPass {
                        pass_hz: rate * 0.035,
                        stop_hz: rate * 0.08,
                    },
                    R::HighPass {
                        stop_hz: rate * 0.32,
                        pass_hz: rate * 0.36,
                    },
                    R::BandPass {
                        low_stop_hz: rate * 0.03,
                        low_pass_hz: rate * 0.055,
                        high_pass_hz: rate * 0.25,
                        high_stop_hz: rate * 0.29,
                    },
                    R::BandStop {
                        low_pass_hz: rate * 0.085,
                        low_stop_hz: rate * 0.135,
                        high_stop_hz: rate * 0.19,
                        high_pass_hz: rate * 0.26,
                    },
                ] {
                    if taps.is_multiple_of(2)
                        && matches!(r, R::HighPass { .. } | R::BandStop { .. })
                    {
                        continue;
                    }
                    let d = D::new(r, rate, taps, pass_weight, stop_weight, O::default())
                        .unwrap_or_else(|e| panic!("{rate}/{taps}/{r:?} {e:?}"));
                    assert!(
                        d.normalized_grid_error()
                            <= d.normalized_ripple() * (1. + 1e-8)
                                + d.normalized_rounding_allowance()
                    );
                    assert_eq!(d.extrema().len(), taps.div_ceil(2) + 1);
                    let mut previous = 0.;
                    for e in d.extrema() {
                        assert!(e.normalized_error != 0.);
                        assert_ne!(previous, e.normalized_error.signum());
                        previous = e.normalized_error.signum();
                    }
                }
            }
            for taps in [31, 32] {
                let bands = [
                    FirBand {
                        low_hz: 0.,
                        high_hz: rate * 0.10,
                        low_gain: -1.,
                        high_gain: -0.5,
                        weight: 2.,
                    },
                    FirBand {
                        low_hz: rate * 0.15,
                        high_hz: rate * 0.30,
                        low_gain: 0.3,
                        high_gain: 1.,
                        weight: 1.,
                    },
                    FirBand {
                        low_hz: rate * 0.38,
                        high_hz: rate * 0.5,
                        low_gain: 0.,
                        high_gain: 0.,
                        weight: 3.,
                    },
                ];
                let d = D::from_bands(rate, taps, &bands, O::default()).unwrap();
                for b in &bands {
                    let mut maximum = 0f64;
                    for j in 0..=1024 {
                        let t = j as f64 / 1024.;
                        let f = ((1. - t) * b.low_hz + t * b.high_hz) / rate;
                        let goal = (1. - t) * b.low_gain + t * b.high_gain;
                        maximum = maximum.max(
                            (goal - centered(d.design().coefficients(), f)).abs() * b.weight / 3.,
                        );
                    }
                    assert!(maximum <= d.normalized_grid_error() * 1.01 + 1e-10);
                }
            }
        }
    }
}

#[test]
fn normalized_domain_preserves_original_boundaries_and_exact_typeii_nyquist_before_allocation() {
    use ravet_signal::{EquirippleError as E, FirDesignError as B};
    let band = FirBand {
        low_hz: 0.,
        high_hz: 0.5,
        low_gain: 0.,
        high_gain: 0.,
        weight: 1.,
    };
    let measured = allocation_counter::measure(|| {
        // Odd subnormal rates round Nyquist both upward and downward. Neither
        // the original requested endpoint nor the normalized boundary survives.
        for bits in [1u64, 3, 253, 255, 4093, 4095, (1u64 << 51) + 1] {
            let rate = f64::from_bits(bits);
            for taps in [3, 4] {
                assert_eq!(
                    D::from_bands(
                        rate,
                        taps,
                        &[FirBand {
                            high_hz: rate * 0.5,
                            ..band
                        }],
                        O {
                            grid_density: 8,
                            ..O::default()
                        }
                    )
                    .unwrap_err(),
                    E::Design(B::Unrepresentable)
                );
            }
        }
        let low = f64::from_bits(14400f64.to_bits() + 2);
        let high = f64::from_bits(low.to_bits() + 1);
        assert!(low < high);
        assert_eq!(low / 48000., high / 48000.);
        // Preserve an original positive-width band and a separated original gap.
        assert_eq!(
            D::from_bands(
                48000.,
                3,
                &[FirBand {
                    low_hz: low,
                    high_hz: high,
                    ..band
                }],
                O::default()
            )
            .unwrap_err(),
            E::Design(B::Unrepresentable)
        );
        assert_eq!(
            D::from_bands(
                48000.,
                3,
                &[
                    FirBand {
                        high_hz: low,
                        ..band
                    },
                    FirBand {
                        low_hz: high,
                        high_hz: 24000.,
                        ..band
                    }
                ],
                O::default()
            )
            .unwrap_err(),
            E::Design(B::Unrepresentable)
        );
        assert_eq!(
            D::from_bands(
                f64::MAX,
                3,
                &[FirBand {
                    high_hz: f64::MIN_POSITIVE,
                    ..band
                }],
                O::default()
            )
            .unwrap_err(),
            E::Design(B::Unrepresentable)
        );
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
    for rate in [f64::from_bits(254), 48000.] {
        let nyquist = rate * 0.5;
        assert_eq!(nyquist / rate, 0.5);
        let d = D::from_bands(
            rate,
            4,
            &[FirBand {
                high_hz: nyquist,
                ..band
            }],
            O {
                grid_density: 8,
                ..O::default()
            },
        )
        .unwrap();
        assert!(d.forced_nyquist());
        assert_eq!(d.normalized_grid_error(), 0.);
    }
    // A representable original endpoint immediately below Nyquist stays ordinary.
    let high = f64::from_bits(24000f64.to_bits() - 1);
    let d = D::from_bands(
        48000.,
        4,
        &[FirBand {
            high_hz: high,
            ..band
        }],
        O::default(),
    )
    .unwrap();
    assert!(!d.forced_nyquist());
}

#[test]
fn halfband_receipts_evaluate_both_actual_rounded_grid_halves_and_reflected_supports() {
    for (taps, width) in [(3, 0.4), (7, 0.1), (31, 0.08), (63, 0.04), (511, 0.01)] {
        let d = D::halfband(taps, width, O::default()).unwrap();
        let pass = 0.25 - width / 2.;
        let step = 1. / (2. * (taps + 1) as f64 * d.grid_density() as f64);
        let n = (pass / step).ceil() as usize + 1;
        assert_eq!(d.grid_points(), n * 2);
        let h = d.design().coefficients();
        let mut actual = 0f64;
        for j in 0..n {
            let f = j as f64 / (n - 1) as f64 * pass;
            actual = actual.max((1. - centered(h, f)).abs());
            // Evaluate the rounded stop frequency itself; do not infer its error
            // by negating the lowband error or complementary ideal polynomial.
            actual = actual.max(centered(h, 0.5 - f).abs());
        }
        assert_eq!(d.normalized_grid_error(), actual);
        for (low, stop) in d.extrema().iter().rev().zip(d.reflected_halfband_extrema()) {
            assert_eq!(stop.frequency_hz, 0.5 - low.frequency_hz);
            assert_eq!(stop.normalized_error, -centered(h, stop.frequency_hz));
            assert_eq!(stop.weighted_error, stop.normalized_error);
            assert_eq!(
                stop.normalized_error.signum(),
                -low.normalized_error.signum()
            );
        }
    }
}
