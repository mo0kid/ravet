use ravet_signal::{ChorusControls, ChorusError, StereoChorus};
#[test]
fn independent_full_history_fractional_feedback_recurrence_matches_modulation() {
    let rate = 8000.;
    let controls = ChorusControls {
        rate_hz: 1.23,
        depth: 0.8,
        center_delay_ms: 7.5,
        feedback: -0.4,
        mix: 0.3,
        stereo_phase_cycles: 0.25,
    };
    let mut chorus = StereoChorus::new(rate, controls).unwrap();
    let mut history = [vec![0.; 32768], vec![0.; 32768]];
    for n in 0..32768 {
        let input = [(n as f64 * 0.057).sin(), (n as f64 * 0.033).cos() * 0.5];
        let mut expected = [0.; 2];
        for channel in 0..2 {
            let phase = (n as f64 * controls.rate_hz / rate
                + if channel == 0 {
                    0.
                } else {
                    controls.stereo_phase_cycles
                })
            .fract();
            let d = (controls.center_delay_ms
                * rate
                * 0.001
                * (1. + controls.depth * (std::f64::consts::TAU * phase).sin()))
            .max(1.);
            let whole = d.floor() as usize;
            let fraction = d - whole as f64;
            let recent = if n >= whole {
                history[channel][n - whole]
            } else {
                0.
            };
            let previous = if n > whole {
                history[channel][n - whole - 1]
            } else {
                0.
            };
            let wet = recent * (1. - fraction) + previous * fraction;
            history[channel][n] = input[channel] + controls.feedback * wet;
            expected[channel] = (1. - controls.mix) * input[channel] + controls.mix * wet;
        }
        let output = chorus.process_sample(input[0], input[1]);
        for i in 0..2 {
            assert!((output[i] - expected[i]).abs() < 1e-9, "sample {n}/{i}");
        }
    }
}
#[test]
fn static_fractional_impulse_feedback_polarity_and_stereo_phase_landmarks() {
    let controls = ChorusControls {
        rate_hz: 0.,
        depth: 0.,
        center_delay_ms: 1.3125,
        feedback: 0.,
        mix: 1.,
        stereo_phase_cycles: 0.,
    };
    let mut chorus = StereoChorus::new(8000., controls).unwrap();
    for n in 0..32 {
        let expected = if n == 10 || n == 11 { 0.5 } else { 0. };
        assert_eq!(
            chorus.process_sample(if n == 0 { 1. } else { 0. }, 0.),
            [expected, 0.]
        );
    }
    for feedback in [-0.5, 0.5] {
        let mut chorus = StereoChorus::new(
            8000.,
            ChorusControls {
                center_delay_ms: 1.,
                feedback,
                ..controls
            },
        )
        .unwrap();
        for n in 0..128 {
            let expected = if n >= 8 && n % 8 == 0 {
                feedback.powi(n / 8 - 1)
            } else {
                0.
            };
            assert_eq!(
                chorus.process_sample(if n == 0 { 1. } else { 0. }, 0.),
                [expected, 0.]
            );
        }
    }
    let mut chorus = StereoChorus::new(
        8000.,
        ChorusControls {
            depth: 0.5,
            center_delay_ms: 1.,
            stereo_phase_cycles: 0.25,
            ..controls
        },
    )
    .unwrap();
    for n in 0..24 {
        let x = if n == 0 { 1. } else { 0. };
        assert_eq!(
            chorus.process_sample(x, x),
            [if n == 8 { 1. } else { 0. }, if n == 12 { 1. } else { 0. }]
        );
    }
}
#[test]
fn partition_reset_dry_history_and_control_edits_retain_phase() {
    let controls = ChorusControls::default();
    let mut whole = StereoChorus::new(48000., controls).unwrap();
    let mut split = StereoChorus::new(48000., controls).unwrap();
    let source: Vec<_> = (0..4096).map(|n| (n as f64 * 0.023).sin()).collect();
    let mut a = source.clone();
    let mut b = source.clone();
    let mut c = source.clone();
    let mut d = source.clone();
    whole.process_in_place(&mut a, &mut b).unwrap();
    for (left, right) in c.chunks_mut(73).zip(d.chunks_mut(73)) {
        split.process_in_place(left, right).unwrap();
    }
    assert_eq!(a, c);
    assert_eq!(b, d);
    assert_eq!(whole.phase(), split.phase());
    whole.reset();
    let mut fresh = StereoChorus::new(48000., controls).unwrap();
    for x in &source {
        assert_eq!(whole.process_sample(*x, -*x), fresh.process_sample(*x, -*x));
    }
    let phase = whole.phase();
    let frozen = ChorusControls {
        rate_hz: 0.,
        ..controls
    };
    whole.set_controls(frozen).unwrap();
    for _ in 0..32 {
        whole.process_sample(0., 0.);
        assert_eq!(whole.phase(), phase);
    }
    let controls = ChorusControls {
        rate_hz: 0.,
        depth: 0.,
        center_delay_ms: 1.,
        feedback: 0.,
        mix: 0.,
        ..controls
    };
    let mut dry = StereoChorus::new(8000., controls).unwrap();
    for n in 0..4 {
        let x = if n == 0 { 1. } else { 0. };
        assert_eq!(dry.process_sample(x, -x), [x, -x]);
    }
    dry.set_controls(ChorusControls {
        mix: 1.,
        ..controls
    })
    .unwrap();
    for n in 4..16 {
        let x = if n == 8 { 1. } else { 0. };
        assert_eq!(dry.process_sample(0., 0.), [x, -x]);
    }
}
#[test]
fn invalid_construction_edits_and_shapes_preserve_state_without_heap() {
    let c = ChorusControls::default();
    let invalid = [
        ChorusControls { rate_hz: -0.1, ..c },
        ChorusControls {
            rate_hz: 100.01,
            ..c
        },
        ChorusControls {
            rate_hz: f64::NAN,
            ..c
        },
        ChorusControls { depth: -0.1, ..c },
        ChorusControls { depth: 1.01, ..c },
        ChorusControls {
            depth: f64::INFINITY,
            ..c
        },
        ChorusControls {
            center_delay_ms: 0.99,
            ..c
        },
        ChorusControls {
            center_delay_ms: 100.01,
            ..c
        },
        ChorusControls {
            center_delay_ms: f64::NAN,
            ..c
        },
        ChorusControls {
            feedback: -0.9901,
            ..c
        },
        ChorusControls {
            feedback: 0.9901,
            ..c
        },
        ChorusControls {
            feedback: f64::NAN,
            ..c
        },
        ChorusControls { mix: -0.1, ..c },
        ChorusControls { mix: 1.01, ..c },
        ChorusControls {
            mix: f64::INFINITY,
            ..c
        },
        ChorusControls {
            stereo_phase_cycles: -0.1,
            ..c
        },
        ChorusControls {
            stereo_phase_cycles: 1.01,
            ..c
        },
        ChorusControls {
            stereo_phase_cycles: f64::NAN,
            ..c
        },
    ];
    let mut edited = StereoChorus::new(48000., c).unwrap();
    let mut unchanged = StereoChorus::new(48000., c).unwrap();
    for _ in 0..1024 {
        assert_eq!(
            edited.process_sample(0.25, -0.5),
            unchanged.process_sample(0.25, -0.5)
        );
    }
    let mut left = [91.; 4];
    let mut right = [92.; 3];
    let phase = edited.phase();
    let counts = allocation_counter::measure(|| {
        for c in invalid {
            assert!(matches!(
                StereoChorus::new(48000., c),
                Err(ChorusError::InvalidControls)
            ));
            assert_eq!(edited.set_controls(c), Err(ChorusError::InvalidControls));
        }
        for rate in [7999., 192001., 0., f64::NAN, f64::INFINITY] {
            assert!(matches!(
                StereoChorus::new(rate, c),
                Err(ChorusError::InvalidSampleRate)
            ));
        }
        assert_eq!(
            edited.process_in_place(&mut left, &mut right),
            Err(ChorusError::LengthMismatch)
        );
        edited.process_in_place(&mut [], &mut []).unwrap();
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(edited.controls(), c);
    assert_eq!(edited.phase(), phase);
    assert_eq!(left, [91.; 4]);
    assert_eq!(right, [92.; 3]);
    for _ in 0..1024 {
        assert_eq!(
            edited.process_sample(0., 0.),
            unchanged.process_sample(0., 0.)
        );
    }
}
#[test]
fn maximum_preparation_bounds_and_processing_reset_edits_are_heap_free() {
    let c = ChorusControls {
        rate_hz: 100.,
        depth: 1.,
        center_delay_ms: 100.,
        feedback: -0.99,
        mix: 1.,
        stereo_phase_cycles: 1.,
    };
    let mut chorus = StereoChorus::new(192000., c).unwrap();
    assert_eq!(chorus.delay_capacity(), 38401);
    assert_eq!(chorus.sample_rate(), 192000.);
    let mut left = [0.25; 32];
    let mut right = [-0.5; 32];
    let counts = allocation_counter::measure(|| {
        for n in 0..10000 {
            chorus
                .set_controls(if n % 2 == 0 {
                    c
                } else {
                    ChorusControls::default()
                })
                .unwrap();
            left.fill(0.25);
            right.fill(-0.5);
            chorus.process_in_place(&mut left, &mut right).unwrap();
            assert!(chorus.phase() >= 0. && chorus.phase() < 1.);
            if n % 1000 == 0 {
                chorus.reset();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert!(left.iter().chain(&right).all(|x| x.is_finite()));
}
