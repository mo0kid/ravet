use ravet_signal::{
    Complex, FftError, RealFftError, RealFftPlan, WindowConvention as C, WindowError,
    WindowKind as K, WindowPlan,
};

#[test]
fn real_transform_direct_dft_roundtrip_parseval_and_endpoint_reconstruction() {
    for size in [1, 2, 4, 8, 32, 128] {
        let input: Vec<f64> = (0..size).map(|i| (i as f64 * 0.37).sin() + 0.2).collect();
        let mut plan = RealFftPlan::new(size).unwrap();
        let mut bins = vec![Complex::default(); plan.bins()];
        plan.forward(&input, &mut bins).unwrap();
        for (k, actual) in bins.iter().enumerate() {
            let mut expected = Complex::default();
            for (i, x) in input.iter().enumerate() {
                let phase = -std::f64::consts::TAU * (i * k) as f64 / size as f64;
                expected.re += x * phase.cos();
                expected.im += x * phase.sin();
            }
            assert!((actual.re - expected.re).abs() < 1e-10);
            assert!((actual.im - expected.im).abs() < 1e-10);
        }
        let time_energy: f64 = input.iter().map(|x| x * x).sum();
        let frequency_energy: f64 = bins
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let weight = if i == 0 || i * 2 == size { 1. } else { 2. };
                weight * (b.re * b.re + b.im * b.im) / size as f64
            })
            .sum();
        assert!((time_energy - frequency_energy).abs() < 1e-10);
        let mut output = vec![0.; size];
        plan.inverse(&bins, &mut output).unwrap();
        for (a, b) in input.iter().zip(&output) {
            assert!((a - b).abs() < 1e-12);
        }
    }
    let mut plan = RealFftPlan::new(16).unwrap();
    let mut bins = [Complex::default(); 9];
    bins[0].re = 16.;
    bins[3].re = 8.;
    bins[8].re = 4.;
    let mut out = [0.; 16];
    plan.inverse(&bins, &mut out).unwrap();
    for (i, x) in out.iter().enumerate() {
        let expected = 1.
            + (std::f64::consts::TAU * 3. * i as f64 / 16.).cos()
            + if i % 2 == 0 { 0.25 } else { -0.25 };
        assert!((x - expected).abs() < 1e-12);
    }
}
#[test]
fn prepared_windows_landmarks_conventions_gain_and_noise_bandwidth() {
    for (kind, expected, mean, harmonics) in [
        (K::Rectangular, [1., 1., 1., 1., 1.], 1., [0., 0., 0.]),
        (K::Hann, [0., 0.5, 1., 0.5, 0.], 0.5, [-0.5, 0., 0.]),
        (
            K::Hamming,
            [0.08, 0.54, 1., 0.54, 0.08],
            0.54,
            [-0.46, 0., 0.],
        ),
        (
            K::Blackman,
            [0., 0.34, 1., 0.34, 0.],
            0.42,
            [-0.5, 0.08, 0.],
        ),
        (
            K::BlackmanHarris,
            [0.00006, 0.21747, 1., 0.21747, 0.00006],
            0.35875,
            [-0.48829, 0.14128, -0.01168],
        ),
    ] {
        let symmetric = WindowPlan::new(kind, C::Symmetric, 5).unwrap();
        for (a, b) in symmetric.coefficients().iter().zip(expected) {
            assert!((a - b).abs() < 1e-14);
        }
        let periodic = WindowPlan::new(kind, C::Periodic, 4).unwrap();
        assert_eq!(periodic.coefficients(), &symmetric.coefficients()[..4]);
        let long = WindowPlan::new(kind, C::Periodic, 1024).unwrap();
        assert!((long.coherent_gain() - mean).abs() < 1e-14);
        let noise = 1. + harmonics.iter().map(|x| x * x).sum::<f64>() / (2. * mean * mean);
        assert!((long.equivalent_noise_bandwidth().unwrap() - noise).abs() < 1e-13);
        assert!((long.energy() - noise * mean * mean * 1024.).abs() < 1e-10);
        for convention in [C::Symmetric, C::Periodic] {
            assert_eq!(
                WindowPlan::new(kind, convention, 1).unwrap().coefficients(),
                &[1.]
            );
        }
    }
    let window = WindowPlan::new(K::Hann, C::Periodic, 1024).unwrap();
    let mut input: Vec<f64> = (0..1024)
        .map(|i| (std::f64::consts::TAU * 31. * i as f64 / 1024.).cos())
        .collect();
    window.apply(&mut input).unwrap();
    let mut plan = RealFftPlan::new(1024).unwrap();
    let mut bins = vec![Complex::default(); plan.bins()];
    plan.forward(&input, &mut bins).unwrap();
    let corrected = 2. * bins[31].re.hypot(bins[31].im) / (1024. * window.coherent_gain());
    assert!((corrected - 1.).abs() < 1e-12);
    assert!(
        WindowPlan::new(K::Hann, C::Symmetric, 2)
            .unwrap()
            .equivalent_noise_bandwidth()
            .is_none()
    );
}
#[test]
fn atomic_errors_endpoints_nonfinite_and_prepared_heap_policy() {
    assert_eq!(
        RealFftPlan::new(3).unwrap_err(),
        RealFftError::Transform(FftError::InvalidSize)
    );
    let mut plan = RealFftPlan::new(8).unwrap();
    let mut bins = [Complex::new(99., 99.); 5];
    assert_eq!(
        plan.forward(&[1.; 7], &mut bins),
        Err(RealFftError::Transform(FftError::LengthMismatch))
    );
    assert_eq!(bins, [Complex::new(99., 99.); 5]);
    let mut out = [99.; 8];
    for endpoint in [0, 4] {
        for imaginary in [1., f64::NAN, f64::INFINITY] {
            bins.fill(Complex::default());
            bins[endpoint].im = imaginary;
            assert_eq!(
                plan.inverse(&bins, &mut out),
                Err(RealFftError::InvalidEndpoints)
            );
            assert_eq!(out, [99.; 8]);
        }
    }
    let window = WindowPlan::new(K::Hann, C::Periodic, 8).unwrap();
    let mut short = [99.; 7];
    assert_eq!(window.apply(&mut short), Err(WindowError::LengthMismatch));
    assert_eq!(short, [99.; 7]);
    for size in [0, 1048577] {
        assert_eq!(
            WindowPlan::new(K::Hann, C::Periodic, size).unwrap_err(),
            WindowError::InvalidSize
        );
    }
    let counts = allocation_counter::measure(|| {
        for _ in 0..100 {
            let mut input = [1.; 8];
            window.apply(&mut input).unwrap();
            plan.forward(&input, &mut bins).unwrap();
            plan.inverse(&bins, &mut out).unwrap();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.bytes_total, 0);
    plan.forward(&[f64::NAN; 8], &mut bins).unwrap();
    assert_eq!(bins[0].im, 0.);
    assert_eq!(bins[4].im, 0.);
    assert!(bins[0].re.is_nan());
}
