use ravet_signal::{CombError, CombFilter, CombKind};

const KINDS: [CombKind; 2] = [CombKind::Feedforward, CombKind::Feedback];

#[test]
fn impulse_timing_polarity_and_decay_match_both_transfer_functions() {
    for kind in KINDS {
        for delay in [1, 2, 7, 32] {
            for g in [-0.5_f64, 0.0, 0.5] {
                let mut comb = CombFilter::new(kind, delay, g).unwrap();
                for n in 0..delay * 64 {
                    let expected = if n == 0 {
                        1.0
                    } else if kind == CombKind::Feedforward {
                        if n == delay { g } else { 0.0 }
                    } else if n % delay == 0 {
                        g.powi((n / delay) as i32)
                    } else {
                        0.0
                    };
                    assert_eq!(
                        comb.process_sample(if n == 0 { 1.0 } else { 0.0 }),
                        expected
                    );
                }
            }
        }
    }
    // Feedforward is finite impulse response even at unity or larger gain.
    for g in [-2.0, -1.0, 1.0, 2.0] {
        let mut comb = CombFilter::new(CombKind::Feedforward, 1, g).unwrap();
        assert_eq!(comb.process_sample(1.0), 1.0);
        assert_eq!(comb.process_sample(0.0), g);
        assert_eq!(comb.process_sample(0.0), 0.0);
    }
}

#[test]
fn independent_full_history_recurrence_matches_wraps_and_live_edits() {
    for kind in KINDS {
        for delay in [1, 3, 17, 256] {
            let mut comb = CombFilter::new(kind, delay, 0.0).unwrap();
            let mut inputs = vec![0.0; 4096];
            let mut outputs = inputs.clone();
            for n in 0..inputs.len() {
                let g = [0.0, 0.4, -0.6, 0.9][(n / 97) % 4];
                comb.set_coefficient(g).unwrap();
                inputs[n] = (n as f64 * 0.037).sin() + (n as f64 * 0.083).cos() * 0.25;
                let delayed = if n < delay {
                    0.0
                } else if kind == CombKind::Feedforward {
                    inputs[n - delay]
                } else {
                    outputs[n - delay]
                };
                outputs[n] = inputs[n] + g * delayed;
                assert_eq!(comb.process_sample(inputs[n]), outputs[n]);
            }
        }
    }
}

#[test]
fn rendered_frequency_response_matches_analytic_complex_gain() {
    for kind in KINDS {
        for delay in [1, 7, 31] {
            for g in [-0.8, 0.0, 0.8] {
                for frequency in [0.0, 0.013, 0.125, 0.37, 0.5] {
                    let mut comb = CombFilter::new(kind, delay, g).unwrap();
                    let omega = std::f64::consts::TAU * frequency;
                    let cos = (omega * delay as f64).cos();
                    let sin = (omega * delay as f64).sin();
                    let (real, imag) = if kind == CombKind::Feedforward {
                        (1.0 + g * cos, -g * sin)
                    } else {
                        let denom = 1.0 - 2.0 * g * cos + g * g;
                        ((1.0 - g * cos) / denom, -g * sin / denom)
                    };
                    for n in 0..8192 {
                        let phase = omega * n as f64;
                        let actual = comb.process_sample(phase.cos());
                        if n >= 4096 {
                            let expected = real * phase.cos() - imag * phase.sin();
                            assert!((actual - expected).abs() < 1e-10);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn partition_empty_blocks_reset_and_zero_coefficient_retain_contracts() {
    for kind in KINDS {
        let source: Vec<_> = (0..4096).map(|n| (n as f64 * 0.017).sin()).collect();
        let mut whole = CombFilter::new(kind, 17, -0.7).unwrap();
        let mut split = CombFilter::new(kind, 17, -0.7).unwrap();
        let mut a = source.clone();
        let mut b = source.clone();
        whole.process_in_place(&mut a);
        for chunk in b.chunks_mut(73) {
            split.process_in_place(&mut []);
            split.process_in_place(chunk);
        }
        assert_eq!(a, b);
        whole.set_coefficient(0.3).unwrap();
        whole.reset();
        assert_eq!(whole.kind(), kind);
        assert_eq!(whole.delay_samples(), 17);
        assert_eq!(whole.coefficient(), 0.3);
        let mut fresh = CombFilter::new(kind, 17, 0.3).unwrap();
        for x in &source {
            assert_eq!(whole.process_sample(*x), fresh.process_sample(*x));
        }

        let mut comb = CombFilter::new(kind, 3, 0.0).unwrap();
        assert_eq!(comb.process_sample(1.0), 1.0);
        assert_eq!(comb.process_sample(0.0), 0.0);
        assert_eq!(comb.process_sample(0.0), 0.0);
        comb.set_coefficient(0.5).unwrap();
        assert_eq!(comb.process_sample(0.0), 0.5);
        comb.reset();
        comb.process_sample(f64::NAN);
        comb.set_coefficient(0.0).unwrap();
        for _ in 0..6 {
            assert_eq!(comb.process_sample(0.25), 0.25);
        }
        comb.reset();
        comb.set_coefficient(0.5).unwrap();
        assert_eq!(comb.process_sample(0.0), 0.0);
    }
}

#[test]
fn invalid_preparation_and_edits_reject_atomically_before_allocation() {
    for kind in KINDS {
        let mut edited = CombFilter::new(kind, 7, 0.5).unwrap();
        let mut reference = CombFilter::new(kind, 7, 0.5).unwrap();
        for n in 0..32 {
            let x = n as f64 * 0.01;
            assert_eq!(edited.process_sample(x), reference.process_sample(x));
        }
        let counts = allocation_counter::measure(|| {
            for delay in [0, 1048577, usize::MAX] {
                assert!(matches!(
                    CombFilter::new(kind, delay, 0.5),
                    Err(CombError::InvalidDelay)
                ));
            }
            for g in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                assert!(matches!(
                    CombFilter::new(kind, 7, g),
                    Err(CombError::InvalidCoefficient)
                ));
                assert_eq!(
                    edited.set_coefficient(g),
                    Err(CombError::InvalidCoefficient)
                );
            }
            if kind == CombKind::Feedback {
                for g in [-f64::MAX, -1.01, -1.0, 1.0, 1.01, f64::MAX] {
                    assert!(matches!(
                        CombFilter::new(kind, 7, g),
                        Err(CombError::InvalidCoefficient)
                    ));
                    assert_eq!(
                        edited.set_coefficient(g),
                        Err(CombError::InvalidCoefficient)
                    );
                }
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
        assert_eq!(edited.coefficient(), 0.5);
        for _ in 0..128 {
            assert_eq!(edited.process_sample(0.0), reference.process_sample(0.0));
        }
    }
    for g in [-1.0_f64.next_down(), 1.0_f64.next_down()] {
        assert!(CombFilter::new(CombKind::Feedback, 1, g).is_ok());
    }
}

#[test]
fn maximum_delay_is_exact_and_prepared_workloads_do_not_touch_heap() {
    for kind in KINDS {
        let counts = allocation_counter::measure(|| {
            let comb = CombFilter::new(kind, 1048576, 0.5).unwrap();
            assert_eq!(comb.delay_samples(), 1048576);
        });
        assert_eq!(counts.count_total, 1);
        assert_eq!(counts.bytes_total, 8 * 1048576);
        assert_eq!(counts.count_current, 0);

        let mut maximum = CombFilter::new(kind, 1048576, 0.5).unwrap();
        assert_eq!(maximum.process_sample(1.0), 1.0);
        for _ in 1..1048576 {
            assert_eq!(maximum.process_sample(0.0), 0.0);
        }
        assert_eq!(maximum.process_sample(0.0), 0.5);

        let mut comb = CombFilter::new(kind, 17, 0.5).unwrap();
        let mut block = [0.25; 32];
        let counts = allocation_counter::measure(|| {
            maximum.reset();
            for n in 0..10000 {
                comb.set_coefficient(if n % 2 == 0 { 0.5 } else { -0.5 })
                    .unwrap();
                block.fill(0.25);
                comb.process_in_place(&mut block);
                if n % 100 == 0 {
                    comb.reset();
                }
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
        assert!(block.iter().all(|sample| sample.is_finite()));
    }
}
