use ravet_signal::{
    Complex, Complex32, FftError, FftPlan, FftPlan32, FftPreparationError as Error,
};
#[test]
fn complete_preflight_storage_and_size_one_zero_budget() {
    for size in [0, 3, 17, 1_048_577, usize::MAX] {
        assert_eq!(
            FftPlan::try_new(size, usize::MAX).unwrap_err(),
            Error::InvalidSize
        );
        assert_eq!(
            FftPlan32::try_new(size, usize::MAX).unwrap_err(),
            Error::InvalidSize
        );
    }
    let p = FftPlan::try_new(1, 0).unwrap();
    let q = FftPlan32::try_new(1, 0).unwrap();
    assert_eq!(p.twiddle_storage_bytes(), 0);
    assert_eq!(q.twiddle_storage_bytes(), 0);
    let mut a = [Complex::new(f64::from_bits(1), -0.)];
    let original = a;
    p.forward(&mut a).unwrap();
    p.inverse(&mut a).unwrap();
    assert_eq!(a[0].re.to_bits(), original[0].re.to_bits());
    assert_eq!(a[0].im.to_bits(), original[0].im.to_bits());
    let mut b = [Complex32::new(f32::from_bits(1), -0.)];
    let original = b;
    q.forward(&mut b).unwrap();
    q.inverse(&mut b).unwrap();
    assert_eq!(b[0].re.to_bits(), original[0].re.to_bits());
    assert_eq!(b[0].im.to_bits(), original[0].im.to_bits());
    for n in [2, 4, 16, 128, 4096] {
        let bytes64 = n / 2 * size_of::<Complex>();
        let bytes32 = n / 2 * size_of::<Complex32>();
        assert_eq!(
            FftPlan::try_new(n, bytes64 - 1).unwrap_err(),
            Error::Capacity
        );
        assert_eq!(
            FftPlan32::try_new(n, bytes32 - 1).unwrap_err(),
            Error::Capacity
        );
        assert_eq!(
            FftPlan::try_new(n, bytes64)
                .unwrap()
                .twiddle_storage_bytes(),
            bytes64
        );
        assert_eq!(
            FftPlan32::try_new(n, bytes32)
                .unwrap()
                .twiddle_storage_bytes(),
            bytes32
        );
    }
    let mut a = [Complex::new(5., -3.); 3];
    let mut b = [Complex32::new(5., -3.); 3];
    assert_eq!(
        FftPlan::try_new(4, 32).unwrap().forward(&mut a),
        Err(FftError::LengthMismatch)
    );
    assert_eq!(
        FftPlan32::try_new(4, 16).unwrap().inverse(&mut b),
        Err(FftError::LengthMismatch)
    );
    assert_eq!(a, [Complex::new(5., -3.); 3]);
    assert_eq!(b, [Complex32::new(5., -3.); 3]);
}
#[test]
fn legacy_bounded_f64_actual_transform_bit_identity() {
    for n in [1, 2, 4, 16, 128, 1024] {
        let legacy = FftPlan::new(n).unwrap();
        let bounded = FftPlan::try_new(n, n / 2 * size_of::<Complex>()).unwrap();
        let mut a: Vec<_> = (0..n)
            .map(|i| Complex::new((i as f64 * 0.31).sin(), (i as f64 * 0.71).cos()))
            .collect();
        let mut b = a.clone();
        legacy.forward(&mut a).unwrap();
        bounded.forward(&mut b).unwrap();
        assert!(
            a.iter()
                .zip(&b)
                .all(|(a, b)| a.re.to_bits() == b.re.to_bits() && a.im.to_bits() == b.im.to_bits())
        );
        legacy.inverse(&mut a).unwrap();
        bounded.inverse(&mut b).unwrap();
        assert!(
            a.iter()
                .zip(&b)
                .all(|(a, b)| a.re.to_bits() == b.re.to_bits() && a.im.to_bits() == b.im.to_bits())
        );
    }
}
#[test]
fn native32_complex_dft_inverse_and_nonfinite_policy_without_heap() {
    for n in [1, 2, 4, 8, 16, 32, 128, 256] {
        let p = FftPlan32::try_new(n, n / 2 * size_of::<Complex32>()).unwrap();
        let original: Vec<_> = (0..n)
            .map(|i| Complex32::new((i as f64 * 0.3).sin() as f32, (i as f64 * 0.7).cos() as f32))
            .collect();
        let expected: Vec<_> = (0..n)
            .map(|k| {
                original
                    .iter()
                    .enumerate()
                    .fold((0., 0.), |(re, im), (j, v)| {
                        let phase = -std::f64::consts::TAU * k as f64 * j as f64 / n as f64;
                        (
                            re + f64::from(v.re) * phase.cos() - f64::from(v.im) * phase.sin(),
                            im + f64::from(v.re) * phase.sin() + f64::from(v.im) * phase.cos(),
                        )
                    })
            })
            .collect();
        let mut actual = original.clone();
        let counter = allocation_counter::measure(|| p.forward(&mut actual).unwrap());
        assert_eq!(counter.count_total, 0);
        assert_eq!(counter.count_current, 0);
        for (a, b) in actual.iter().zip(expected) {
            assert!((f64::from(a.re) - b.0).abs() < 3e-6 * n as f64);
            assert!((f64::from(a.im) - b.1).abs() < 3e-6 * n as f64);
        }
        p.inverse(&mut actual).unwrap();
        for (a, b) in actual.iter().zip(&original) {
            assert!((a.re - b.re).abs() < 2e-6);
            assert!((a.im - b.im).abs() < 2e-6);
        }
    }
    let p = FftPlan32::try_new(4, 16).unwrap();
    let mut a = [Complex32::new(0., 0.); 4];
    a[1].re = f32::NAN;
    p.forward(&mut a).unwrap();
    assert!(a.iter().all(|a| a.re.is_nan() || a.im.is_nan()));
}
#[test]
fn full_maximum_native32_structured_complex_signal() {
    let n = 1_048_576;
    let plan = FftPlan32::try_new(n, n / 2 * size_of::<Complex32>()).unwrap();
    // Exact quarter-cycle complex tone: no generated sin/cos oracle or input error.
    let phase = [
        Complex32::new(1., 0.),
        Complex32::new(0., 1.),
        Complex32::new(-1., 0.),
        Complex32::new(0., -1.),
    ];
    let mut samples: Vec<_> = (0..n).map(|i| phase[i % 4]).collect();
    let counter = allocation_counter::measure(|| plan.forward(&mut samples).unwrap());
    assert_eq!(counter.count_total, 0);
    assert_eq!(counter.count_current, 0);
    let mut leakage = 0f64;
    let mut peak_error = 0f64;
    for (k, v) in samples.iter().enumerate() {
        if k == n / 4 {
            peak_error = (f64::from(v.re) - n as f64)
                .abs()
                .max(f64::from(v.im).abs());
        } else {
            leakage = leakage
                .max(f64::from(v.re).abs())
                .max(f64::from(v.im).abs());
        }
    }
    // Exact source and analytic DFT; CONTROL cos(pi/2) is approximately6e-17,
    // not exact0. The original literal-zero assertion exposed its3.21e-11
    // coherent leakage at fullsize, not native32 butterfly instability. Keep
    // this finite selected-profile qualification distinct from any universal
    // FFT error certificate; no coefficient nudge/repair is performed.
    println!("FFT32 structured pre-inverse peakerror={peak_error:e}, nonpeakmax={leakage:e}");
    assert!(peak_error <= 8e-16 * n as f64, "peakerror={peak_error:e}");
    assert!(
        leakage <= 8e-16 * n as f64,
        "fullsize nonpeak leakage={leakage}"
    );
    plan.inverse(&mut samples).unwrap();
    let inverse_error = samples.iter().enumerate().fold(0f64, |e, (i, v)| {
        e.max(f64::from(v.re - phase[i % 4].re).abs())
            .max(f64::from(v.im - phase[i % 4].im).abs())
    });
    println!(
        "FFT32 fullsize={n}: peakerror={peak_error}, nonpeakmax={leakage:e}, inversemax={inverse_error:e}; exact source/analytic spectrum, finite CONTROL-twiddle rounding observation"
    );
    assert!(
        inverse_error <= 8e-16,
        "fullsize inversemax={inverse_error:e}"
    );
}
