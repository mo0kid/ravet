use ravet_signal::PeakLimiter;
#[test]
fn hard_ceiling_extremes_polarities_and_memoryless_reference() {
    let mut limiter = PeakLimiter::new(48000., -6., 0.).unwrap();
    let ceiling = 10f64.powf(-6. / 20.);
    for input in [
        0.,
        0.1,
        -0.1,
        ceiling,
        -ceiling,
        1.,
        -1.,
        f64::MAX,
        -f64::MAX,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
    ] {
        let output = limiter.process_sample(input);
        assert!(output.is_finite() && output.abs() <= ceiling);
        let expected = input.clamp(-ceiling, ceiling);
        assert!((output - expected).abs() <= ceiling * 1e-15);
    }
    assert_eq!(limiter.ceiling(), ceiling);
    let mut unity = PeakLimiter::new(48000., 0., 0.).unwrap();
    for i in -1000..=1000 {
        let input = i as f64 / 1000.;
        assert_eq!(unity.process_sample(input), input);
    }
}
#[test]
fn instant_attack_exponential_release_partition_reset_and_heap() {
    let mut limiter = PeakLimiter::new(1000., 0., 0.01).unwrap();
    assert_eq!(limiter.process_sample(-4.), -1.);
    assert_eq!(limiter.gain(), 0.25);
    for n in 1..=10 {
        let level = 0.1 + 3.9 * (-(n as f64) / 10.).exp();
        let output = limiter.process_sample(0.1);
        assert!((limiter.detector_level() - level).abs() < 1e-14);
        assert!((output - 0.1 / level).abs() < 1e-14);
    }
    assert_eq!(limiter.process_sample(8.), 1.);
    assert_eq!(limiter.detector_level(), 8.);
    limiter.reset();
    assert_eq!(limiter.gain(), 1.);
    assert_eq!(limiter.detector_level(), 0.);
    let mut a = [0.; 1024];
    for (i, sample) in a.iter_mut().enumerate() {
        *sample = if i % 97 == 0 { 4. } else { -0.5 };
    }
    let mut b = a;
    let mut whole = limiter.clone();
    let mut split = limiter.clone();
    whole.process_in_place(&mut a);
    for part in b.chunks_mut(17) {
        split.process_in_place(part);
    }
    assert_eq!(a, b);
    let counts = allocation_counter::measure(|| {
        for _ in 0..10 {
            limiter.reset();
            for input in [-8., 0., 0.5, 8.].into_iter().cycle().take(10000) {
                std::hint::black_box(limiter.process_sample(input));
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.bytes_total, 0);
}
#[test]
fn invalid_settings_reject_and_reset_recovers_nonfinite_history() {
    for (rate, ceiling, release) in [
        (0., 0., 0.1),
        (f64::NAN, 0., 0.1),
        (48000., -121., 0.1),
        (48000., 25., 0.1),
        (48000., f64::INFINITY, 0.1),
        (48000., 0., -0.1),
        (48000., 0., 61.),
        (48000., 0., f64::NAN),
    ] {
        assert!(PeakLimiter::new(rate, ceiling, release).is_err());
    }
    let mut limiter = PeakLimiter::new(48000., 0., 0.01).unwrap();
    assert!(limiter.process_sample(f64::NAN).is_nan());
    assert!(limiter.detector_level().is_nan());
    limiter.reset();
    assert_eq!(limiter.process_sample(2.), 1.);
}
