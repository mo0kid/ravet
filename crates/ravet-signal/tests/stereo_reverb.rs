use ravet_signal::{ReverbConfig, SchroederReverb, StereoReverbConfig, StereoSchroederReverb};
fn config() -> StereoReverbConfig {
    StereoReverbConfig {
        reverb: ReverbConfig {
            sample_rate: 8000.,
            room_scale: 0.5,
            decay_seconds: 0.2,
            damping_hz: 3000.,
            wet: 1.,
        },
        spread_seconds: 0.001,
        width: 1.,
    }
}
#[test]
fn impulse_spread_width_dry_channels_and_cancellation() {
    let mut full = StereoSchroederReverb::new(config()).unwrap();
    assert_eq!(
        full.delay_lengths(),
        [[124, 148, 164, 172, 20, 5], [132, 156, 172, 180, 28, 13]]
    );
    let mut narrow = StereoSchroederReverb::new(StereoReverbConfig {
        width: 0.,
        ..config()
    })
    .unwrap();
    let mut partial = StereoSchroederReverb::new(StereoReverbConfig {
        width: 0.3,
        ..config()
    })
    .unwrap();
    let mut energy = [0.; 2];
    for frame in 0..16000 {
        let input = if frame == 0 { [1., 0.] } else { [0.; 2] };
        let separated = full.process_sample(input);
        let averaged = narrow.process_sample(input);
        let interpolated = partial.process_sample(input);
        if frame < 124 {
            assert_eq!(separated, [0.; 2]);
        }
        if frame == 124 {
            assert_eq!(separated, [0.03125, 0.]);
        }
        if frame == 132 {
            assert_eq!(separated[1], 0.03125);
        }
        let middle = separated[0] * 0.5 + separated[1] * 0.5;
        assert_eq!(averaged, [middle; 2]);
        for channel in 0..2 {
            assert_eq!(
                interpolated[channel],
                middle * 0.7 + separated[channel] * 0.3
            );
            assert!(separated[channel].is_finite());
            if frame < 2000 {
                energy[0] += separated[channel].powi(2);
            }
            if frame > 8000 {
                energy[1] += separated[channel].powi(2);
            }
        }
    }
    assert!(energy[0] > 0. && energy[1] < energy[0] * 1e-12);
    full.reset();
    for frame in 0..1000 {
        let x = (frame as f64).sin();
        assert_eq!(full.process_sample([x, -x]), [0.; 2]);
    }
    full.set_mix(0., 1.).unwrap();
    for frame in 0..1000 {
        let input = [(frame as f64).sin(), (frame as f64).cos()];
        assert_eq!(full.process_sample(input), input);
    }
    full.set_mix(1., 1.).unwrap();
    assert_ne!(full.process_sample([0.; 2]), [0.; 2]);
}
#[test]
fn zero_spread_matches_existing_mono_partition_reset_and_heap_contract() {
    let c = StereoReverbConfig {
        spread_seconds: 0.,
        ..config()
    };
    let mut stereo = StereoSchroederReverb::new(c).unwrap();
    let mut mono = SchroederReverb::new(c.reverb).unwrap();
    for n in 0..10000 {
        let input = [(n as f64 * 1.1).sin(), (n as f64 * 0.7).cos()];
        let expected = mono.process_sample(input[0] * 0.5 + input[1] * 0.5);
        assert_eq!(stereo.process_sample(input), [expected; 2]);
    }
    let mut a = vec![[0.; 2]; 20000];
    a[0] = [1., 0.5];
    let mut b = a.clone();
    let mut whole = StereoSchroederReverb::new(config()).unwrap();
    let mut split = StereoSchroederReverb::new(config()).unwrap();
    whole.process_in_place(&mut a);
    for block in b.chunks_mut(73) {
        split.process_in_place(block);
    }
    assert_eq!(a, b);
    split.reset();
    b.fill([0.; 2]);
    b[0] = [1., 0.5];
    split.process_in_place(&mut b);
    assert_eq!(a, b);
    let counts = allocation_counter::measure(|| {
        stereo.reset();
        stereo.set_mix(0.5, 0.3).unwrap();
        for n in 0..10000 {
            std::hint::black_box(stereo.process_sample([(n as f64).sin(), 0.2]));
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
#[test]
fn complete_configuration_and_mix_rejection_preserve_history_and_bound_storage() {
    let base = config();
    for spread in [-0.1, 0.01001, f64::NAN, f64::INFINITY] {
        assert!(
            StereoSchroederReverb::new(StereoReverbConfig {
                spread_seconds: spread,
                ..base
            })
            .is_err()
        );
    }
    for width in [-0.1, 1.01, f64::NAN, f64::INFINITY] {
        assert!(StereoSchroederReverb::new(StereoReverbConfig { width, ..base }).is_err());
    }
    let maximum = StereoSchroederReverb::new(StereoReverbConfig {
        reverb: ReverbConfig {
            sample_rate: 192000.,
            room_scale: 2.,
            ..ReverbConfig::default()
        },
        spread_seconds: 0.010,
        width: 1.,
    })
    .unwrap();
    assert_eq!(
        maximum.delay_lengths().iter().flatten().sum::<usize>(),
        133094
    );
    let mut a = StereoSchroederReverb::new(base).unwrap();
    let mut b = StereoSchroederReverb::new(base).unwrap();
    assert_eq!(a.process_sample([1., 0.]), b.process_sample([1., 0.]));
    for (wet, width) in [(f64::NAN, 0.5), (0.5, f64::INFINITY), (-1., 0.), (0., 2.)] {
        assert!(a.set_mix(wet, width).is_err());
        assert_eq!((a.wet(), a.width()), (1., 1.));
    }
    for _ in 0..1000 {
        assert_eq!(a.process_sample([0.; 2]), b.process_sample([0.; 2]));
    }
    a.process_sample([f64::NAN, 0.]);
    for _ in 0..1000 {
        a.process_sample([0.; 2]);
    }
    assert!(a.process_sample([0.; 2]).iter().all(|v| v.is_nan()));
    a.reset();
    assert_eq!(a.process_sample([0.; 2]), [0.; 2]);
}
