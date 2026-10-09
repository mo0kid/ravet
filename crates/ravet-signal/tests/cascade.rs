use ravet_signal::{
    Biquad, BiquadCascade, BiquadCoefficients, ButterworthKind, CascadeCoefficients, FilterKind,
};
fn response<const N: usize>() {
    for kind in [ButterworthKind::LowPass, ButterworthKind::HighPass] {
        for cutoff in [250., 1500., 16000.] {
            let c = CascadeCoefficients::<N>::butterworth(kind, 48000., cutoff).unwrap();
            assert!(
                (c.magnitude_at(cutoff / 48000.).unwrap() - std::f64::consts::FRAC_1_SQRT_2).abs()
                    < 1e-10
            );
            for frequency in [0.0001, 0.002, 0.01, 0.03, 0.1, 0.2, 0.4, 0.499] {
                let ratio = (std::f64::consts::PI * frequency).tan()
                    / (std::f64::consts::PI * cutoff / 48000.).tan();
                let relative = if kind == ButterworthKind::LowPass {
                    ratio
                } else {
                    1. / ratio
                };
                let expected = 1. / (1. + relative.powi((4 * N) as i32)).sqrt();
                assert!(
                    (c.magnitude_at(frequency).unwrap() - expected).abs() < 1e-9,
                    "{N}/{kind:?}/{cutoff}/{frequency}"
                );
            }
            let (dc, nyquist) = if kind == ButterworthKind::LowPass {
                (1., 0.)
            } else {
                (0., 1.)
            };
            assert!((c.magnitude_at(0.).unwrap() - dc).abs() < 1e-9);
            assert!((c.magnitude_at(0.5).unwrap() - nyquist).abs() < 1e-9);
        }
    }
}
#[test]
fn butterworth_matches_independent_warped_response_all_orders() {
    response::<1>();
    response::<2>();
    response::<3>();
    response::<4>();
    response::<5>();
    response::<6>();
    response::<7>();
    response::<8>();
    response::<9>();
    response::<10>();
    response::<11>();
    response::<12>();
    response::<13>();
    response::<14>();
    response::<15>();
    response::<16>();
}
#[test]
fn rendered_cutoff_rms_partition_reset_replacement_and_heap() {
    let c = CascadeCoefficients::<8>::butterworth(ButterworthKind::LowPass, 48000., 1500.).unwrap();
    let mut whole = BiquadCascade::new(c);
    let mut split = BiquadCascade::new(c);
    let mut x = [0.; 8192];
    for (i, sample) in x.iter_mut().enumerate() {
        *sample = (std::f64::consts::TAU * i as f64 / 32.).sin();
    }
    let mut y = x;
    whole.process_in_place(&mut x);
    for part in y.chunks_mut(73) {
        split.process_in_place(part);
    }
    assert_eq!(x, y);
    let power = x[4096..].iter().map(|x| x * x).sum::<f64>() / 4096.;
    assert!((power - 0.25).abs() < 1e-11);
    let counts = allocation_counter::measure(|| {
        for _ in 0..10 {
            whole.reset();
            whole.set_coefficients(c);
            for frame in 0..4096 {
                std::hint::black_box(whole.process_sample(if frame == 0 { 1. } else { 0. }));
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.bytes_total, 0);
    whole.reset();
    let mut fresh = BiquadCascade::new(c);
    for i in 0..128 {
        let sample = i as f64 / 128.;
        assert_eq!(whole.process_sample(sample), fresh.process_sample(sample));
    }
    let coefficients = BiquadCoefficients::design(FilterKind::LowPass, 48000., 1000., 0.7).unwrap();
    let arbitrary = CascadeCoefficients::new([coefficients; 2]).unwrap();
    let mut a = Biquad::new(coefficients);
    let mut b = a.clone();
    let mut cascade = BiquadCascade::new(arbitrary);
    for i in 0..256 {
        let input = i as f64 / 256.;
        assert_eq!(
            cascade.process_sample(input),
            b.process_sample(a.process_sample(input))
        );
    }
    let high = BiquadCoefficients::design(FilterKind::HighPass, 48000., 2000., 0.8).unwrap();
    a.set_coefficients(high);
    cascade.set_coefficients(CascadeCoefficients::new([high, coefficients]).unwrap());
    // A changed first stage preserves both histories exactly as the independent
    // stateful processors do, rather than resetting the whole cascade.
    for i in 0..256 {
        let input = -(i as f64) / 256.;
        assert_eq!(
            cascade.process_sample(input),
            b.process_sample(a.process_sample(input))
        );
    }
}
#[test]
fn invalid_capacity_frequency_and_response_reject() {
    assert!(
        CascadeCoefficients::<0>::butterworth(ButterworthKind::LowPass, 48000., 1000.).is_err()
    );
    assert!(
        CascadeCoefficients::<17>::butterworth(ButterworthKind::HighPass, 48000., 1000.).is_err()
    );
    let identity = BiquadCoefficients::new([1., 0., 0.], [0., 0.]).unwrap();
    assert!(CascadeCoefficients::<0>::new([]).is_err());
    assert!(CascadeCoefficients::<17>::new([identity; 17]).is_err());
    for (rate, frequency) in [
        (0., 1000.),
        (48000., 0.),
        (48000., 24000.),
        (f64::NAN, 1000.),
        (48000., f64::INFINITY),
    ] {
        assert!(
            CascadeCoefficients::<4>::butterworth(ButterworthKind::LowPass, rate, frequency)
                .is_err()
        );
    }
    let c = CascadeCoefficients::<2>::butterworth(ButterworthKind::LowPass, 48000., 1000.).unwrap();
    for frequency in [-0.1, 0.6, f64::NAN] {
        assert!(c.magnitude_at(frequency).is_err());
    }
}
