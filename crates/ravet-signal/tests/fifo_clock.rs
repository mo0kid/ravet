use ravet_signal::{FifoClockConfig, FifoClockController, FifoClockError, SincResampler};
#[test]
fn independent_clock_queue_model_settles_both_directions_with_jitter() {
    for drift in [-1500., -500., 500., 1500.] {
        for jitter in [false, true] {
            let config = FifoClockConfig::default();
            let mut clock = FifoClockController::new(config).unwrap();
            let mut occupancy = config.target_frames as f64;
            let mut minimum = occupancy;
            let mut maximum = occupancy;
            let rate = 48000.;
            for i in 0..30000 {
                // Vary output callback size; the input clock is independently
                // faster/slower. Bounded arrival jitter affects observations,
                // not the long-term production clock.
                let frames = if i % 3 == 0 { 64. } else { 128. };
                let duration = frames / rate;
                let perturb = if jitter {
                    (i as f64 * 0.13).sin() * 64.
                } else {
                    0.
                };
                let observed = (occupancy + perturb).round() as usize;
                let correction = clock.observe(observed, duration).unwrap();
                occupancy += frames * (1. + drift * 1e-6) - frames * (1. + correction * 1e-6);
                minimum = minimum.min(occupancy);
                maximum = maximum.max(occupancy);
                assert!(occupancy > 0. && occupancy < config.capacity_frames as f64);
                assert!(correction.abs() <= config.maximum_ppm);
            }
            assert!(
                (clock.correction_ppm() - drift).abs() < 30.,
                "{drift}/{jitter}: {}",
                clock.correction_ppm()
            );
            let equilibrium = config.target_frames as f64 + drift / config.ppm_per_frame;
            assert!(
                (occupancy - equilibrium).abs() < 8.,
                "{drift}/{jitter}: {occupancy}/{equilibrium}"
            );
            assert!(maximum - minimum < 800.);
        }
    }
}
#[test]
fn smoothing_and_slew_match_independent_landmarks_and_drive_prepared_resampler_without_heap() {
    let config = FifoClockConfig {
        capacity_frames: 1000,
        target_frames: 500,
        maximum_ppm: 1000.,
        ppm_per_frame: 10.,
        smoothing_seconds: 1.,
        slew_ppm_per_second: 100.,
    };
    let mut c = FifoClockController::new(config).unwrap();
    let correction = c.observe(600, 0.1).unwrap();
    assert_eq!(correction, 10.);
    assert!((c.filtered_frames() - (600. - 100. * (-0.1f64).exp())).abs() < 1e-12);
    for _ in 0..20 {
        c.observe(1000, 1.).unwrap();
    }
    assert_eq!(c.correction_ppm(), 1000.);
    c.reset();
    assert_eq!(c.correction_ppm(), 0.);
    assert_eq!(c.filtered_frames(), 500.);
    let mut r = SincResampler::new_variable(48000., 48000., 32, 1000.).unwrap();
    let mut out = [0.; 64];
    let counts = allocation_counter::measure(|| {
        for i in 0..1000 {
            let ppm = c
                .observe(if i % 2 == 0 { 510 } else { 490 }, 0.001)
                .unwrap();
            r.set_rate_adjustment_ppm(ppm).unwrap();
            r.process(&[0.5; 64], &mut out);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.bytes_total, 0);
    let instant = FifoClockConfig {
        smoothing_seconds: 0.,
        slew_ppm_per_second: 1000000.,
        ..config
    };
    let mut c = FifoClockController::new(instant).unwrap();
    assert_eq!(c.observe(600, 1.).unwrap(), 1000.);
    assert_eq!(c.observe(400, 1.).unwrap(), -1000.);
}
#[test]
fn rejected_observations_are_atomic_and_configuration_bounds_are_explicit() {
    let config = FifoClockConfig::default();
    for invalid in [
        FifoClockConfig {
            capacity_frames: 1,
            ..config
        },
        FifoClockConfig {
            target_frames: 0,
            ..config
        },
        FifoClockConfig {
            target_frames: 4096,
            ..config
        },
        FifoClockConfig {
            maximum_ppm: 0.,
            ..config
        },
        FifoClockConfig {
            maximum_ppm: 10001.,
            ..config
        },
        FifoClockConfig {
            ppm_per_frame: f64::NAN,
            ..config
        },
        FifoClockConfig {
            smoothing_seconds: -1.,
            ..config
        },
        FifoClockConfig {
            slew_ppm_per_second: 0.,
            ..config
        },
    ] {
        assert!(FifoClockController::new(invalid).is_err());
    }
    let mut c = FifoClockController::new(config).unwrap();
    c.observe(2100, 0.1).unwrap();
    let before = (c.correction_ppm(), c.filtered_frames());
    for (queued, time) in [
        (4097, 0.1),
        (2000, 0.),
        (2000, -1.),
        (2000, 11.),
        (2000, f64::NAN),
        (2000, f64::INFINITY),
    ] {
        assert_eq!(c.observe(queued, time), Err(FifoClockError));
        assert_eq!((c.correction_ppm(), c.filtered_frames()), before);
    }
    for maximum in [0.001, 0.3, 123.456789, 9999.99, 10000.] {
        let mut bounded = FifoClockController::new(FifoClockConfig {
            maximum_ppm: maximum,
            smoothing_seconds: 0.,
            slew_ppm_per_second: 999999.,
            ..config
        })
        .unwrap();
        for queued in [0, 4096, 1, 4095, 2047, 2049, 0, 4096]
            .into_iter()
            .cycle()
            .take(1000)
        {
            let value = bounded.observe(queued, 0.00171).unwrap();
            assert!(value.abs() <= maximum);
        }
    }
    assert_eq!(c.config(), config);
}
