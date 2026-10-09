use ravet_signal::{ReverbConfig, SchroederReverb};

#[test]
fn impulse_and_noise_match_independent_time_indexed_network() {
    for config in [
        ReverbConfig {
            sample_rate: 8000.,
            room_scale: 0.5,
            decay_seconds: 0.2,
            damping_hz: 3000.,
            wet: 1.,
        },
        ReverbConfig {
            sample_rate: 44100.,
            room_scale: 1.3,
            decay_seconds: 2.,
            damping_hz: 1000.,
            wet: 0.4,
        },
    ] {
        let mut actual = SchroederReverb::new(config).unwrap();
        let lengths = actual.delay_lengths();
        let count = 10000;
        let mut comb_histories: [Vec<f64>; 4] = std::array::from_fn(|_| vec![0.; count]);
        let mut lowpass = [0.; 4];
        let mut inputs: [Vec<f64>; 2] = std::array::from_fn(|_| vec![0.; count]);
        let mut outputs: [Vec<f64>; 2] = std::array::from_fn(|_| vec![0.; count]);
        let pole = (-std::f64::consts::TAU * config.damping_hz / config.sample_rate).exp();
        for n in 0..count {
            let input = if n == 0 {
                1.
            } else if n < 2000 {
                (n as f64 * 1.71).sin() * 0.1
            } else {
                0.
            };
            let mut wet = 0.;
            for c in 0..4 {
                let delayed = if n >= lengths[c] {
                    comb_histories[c][n - lengths[c]]
                } else {
                    0.
                };
                lowpass[c] = pole * lowpass[c] + (1. - pole) * delayed;
                let feedback = 10f64
                    .powf(-3. * lengths[c] as f64 / (config.sample_rate * config.decay_seconds));
                comb_histories[c][n] = input + feedback * lowpass[c];
                wet += delayed * 0.25;
            }
            for a in 0..2 {
                inputs[a][n] = wet;
                wet *= -0.5;
                if n >= lengths[a + 4] {
                    wet += inputs[a][n - lengths[a + 4]] + 0.5 * outputs[a][n - lengths[a + 4]];
                }
                outputs[a][n] = wet;
            }
            let expected = (1. - config.wet) * input + config.wet * wet;
            assert!(
                (actual.process_sample(input) - expected).abs() < 1e-12,
                "{n}"
            );
        }
    }
}

#[test]
fn decay_partition_reset_mix_and_heap_policy() {
    let config = ReverbConfig {
        sample_rate: 8000.,
        decay_seconds: 0.2,
        damping_hz: 3000.,
        wet: 1.,
        ..ReverbConfig::default()
    };
    let mut whole = SchroederReverb::new(config).unwrap();
    assert_eq!(whole.delay_lengths(), [248, 296, 328, 344, 40, 10]);
    let mut split = SchroederReverb::new(config).unwrap();
    let mut a = vec![0.; 16000];
    a[0] = 1.;
    let mut b = a.clone();
    whole.process_in_place(&mut a);
    for part in b.chunks_mut(73) {
        split.process_in_place(part);
    }
    assert_eq!(a, b);
    assert!(a[..248].iter().all(|x| *x == 0.));
    assert_eq!(a[248], 0.0625);
    let early: f64 = a[248..2000].iter().map(|x| x * x).sum();
    let late: f64 = a[8000..].iter().map(|x| x * x).sum();
    assert!(early > 0. && late < early * 1e-12);
    split.reset();
    b.fill(0.);
    b[0] = 1.;
    split.process_in_place(&mut b);
    assert_eq!(a, b);
    split.reset();
    split.set_wet(0.).unwrap();
    for i in 0..1000 {
        let x = (i as f64).sin();
        assert_eq!(split.process_sample(x), x);
    }
    assert!(split.set_wet(f64::NAN).is_err());
    assert_eq!(split.wet(), 0.);
    split.set_wet(1.).unwrap(); // histories continued while dry
    assert!(split.process_sample(0.).abs() > 0.);
    let counts = allocation_counter::measure(|| {
        split.reset();
        split.set_wet(0.5).unwrap();
        for i in 0..10000 {
            std::hint::black_box(split.process_sample((i as f64).sin()));
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.bytes_total, 0);
}

#[test]
fn configuration_bounds_and_nonfinite_recovery() {
    let base = ReverbConfig::default();
    for config in [
        ReverbConfig {
            sample_rate: 7999.,
            ..base
        },
        ReverbConfig {
            sample_rate: 192001.,
            ..base
        },
        ReverbConfig {
            sample_rate: f64::NAN,
            ..base
        },
        ReverbConfig {
            room_scale: 0.49,
            ..base
        },
        ReverbConfig {
            room_scale: 2.01,
            ..base
        },
        ReverbConfig {
            decay_seconds: 0.09,
            ..base
        },
        ReverbConfig {
            decay_seconds: 20.01,
            ..base
        },
        ReverbConfig {
            damping_hz: 19.,
            ..base
        },
        ReverbConfig {
            damping_hz: 24000.,
            ..base
        },
        ReverbConfig { wet: -0.1, ..base },
        ReverbConfig {
            wet: f64::INFINITY,
            ..base
        },
    ] {
        assert!(SchroederReverb::new(config).is_err());
    }
    let maximum = SchroederReverb::new(ReverbConfig {
        sample_rate: 192000.,
        room_scale: 2.,
        decay_seconds: 20.,
        damping_hz: 20.,
        wet: 1.,
    })
    .unwrap();
    assert_eq!(
        maximum.delay_lengths(),
        [11904, 14208, 15744, 16512, 1920, 499]
    );
    let mut r = SchroederReverb::new(base).unwrap();
    assert!(r.process_sample(f64::NAN).is_nan());
    for _ in 0..3000 {
        r.process_sample(0.);
    }
    assert!(r.process_sample(0.).is_nan());
    r.reset();
    assert_eq!(r.process_sample(0.), 0.);
}
