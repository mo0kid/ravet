use ravet_signal::{MultiplicativeSmoother, SmoothingError};
#[test]
fn geometric_landmarks_exact_endpoint_and_mid_ramp_retarget() {
    let mut smoother = MultiplicativeSmoother::new(1., 4).unwrap();
    smoother.set_target(16.).unwrap();
    assert_eq!(smoother.current(), 1.);
    for expected in [2., 4., 8., 16.] {
        assert_eq!(smoother.next_value(), expected);
    }
    assert!(!smoother.is_smoothing());
    assert_eq!(smoother.next_value(), 16.);
    smoother.set_target(1.).unwrap();
    assert_eq!(smoother.next_value(), 8.);
    smoother.set_target(128.).unwrap();
    assert_eq!(smoother.current(), 8.);
    assert_eq!(smoother.next_value(), 16.);
    smoother.set_target(128.).unwrap();
    for expected in [32., 64., 128.] {
        assert_eq!(smoother.next_value(), expected);
    }
    smoother.set_target(2.).unwrap();
    smoother.next_value();
    smoother.reset();
    assert_eq!(smoother.current(), 2.);
    assert_eq!(smoother.target(), 2.);
    assert!(!smoother.is_smoothing());
    for duration in [0, 1] {
        let mut smoother = MultiplicativeSmoother::new(1., duration).unwrap();
        smoother.set_target(7.).unwrap();
        if duration == 0 {
            assert_eq!(smoother.current(), 7.);
        }
        assert_eq!(smoother.next_value(), 7.);
        assert!(!smoother.is_smoothing());
    }
}
#[test]
fn extreme_positive_values_monotonicity_partition_and_rounded_unity_ratio() {
    for (start, target) in [
        (f32::from_bits(1), f32::MAX),
        (f32::MAX, f32::from_bits(1)),
        (20., 20000.),
        (20000., 20.),
    ] {
        let mut smoother = MultiplicativeSmoother::new(start, 1000).unwrap();
        smoother.set_target(target).unwrap();
        let mut previous = start;
        for sample in 1..=1000 {
            let actual = smoother.next_value();
            assert!(actual.is_finite() && actual > 0.);
            assert!(if start < target {
                actual >= previous && actual <= target
            } else {
                actual <= previous && actual >= target
            });
            let expected = ((f64::from(start).ln() * (1. - sample as f64 / 1000.))
                + f64::from(target).ln() * (sample as f64 / 1000.))
                .exp() as f32;
            assert!((actual - expected).abs() <= expected.abs() * 3e-6 + f32::from_bits(1));
            previous = actual;
        }
        assert_eq!(smoother.current(), target);
    }
    for (start, target) in [
        (1., f32::from_bits(1_f32.to_bits() + 1)),
        (f32::from_bits(1_f32.to_bits() + 1), 1.),
    ] {
        let mut smoother = MultiplicativeSmoother::new(start, u32::MAX).unwrap();
        smoother.set_target(target).unwrap();
        for _ in 0..4 {
            assert_eq!(smoother.next_value(), start);
        }
        assert!(smoother.is_smoothing());
        smoother.reset();
        assert_eq!(smoother.current(), target);
    }
    let mut whole = MultiplicativeSmoother::new(0.1, 1024).unwrap();
    whole.set_target(10.).unwrap();
    let mut split = whole;
    let a: [f32; 1024] = std::array::from_fn(|_| whole.next_value());
    let mut b = [0.; 1024];
    for block in b.chunks_mut(73) {
        for value in block {
            *value = split.next_value();
        }
    }
    assert_eq!(a, b);
}
#[test]
fn invalid_updates_preserve_progress_and_processing_retuning_reset_have_no_heap_action() {
    let mut a = MultiplicativeSmoother::new(1., 1000).unwrap();
    a.set_target(100.).unwrap();
    let mut b = a;
    for value in [0., -0., -1.] {
        assert_eq!(a.set_target(value), Err(SmoothingError::NonPositiveValue));
        assert!(MultiplicativeSmoother::new(value, 1).is_err());
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(a.set_target(value), Err(SmoothingError::NonFiniteValue));
        assert!(MultiplicativeSmoother::new(value, 1).is_err());
    }
    for _ in 0..1000 {
        assert_eq!(a.next_value(), b.next_value());
    }
    let counts = allocation_counter::measure(|| {
        for target in [0.1, 10., 0.25, 2.] {
            a.set_target(target).unwrap();
            for _ in 0..10000 {
                std::hint::black_box(a.next_value());
            }
            a.reset();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
