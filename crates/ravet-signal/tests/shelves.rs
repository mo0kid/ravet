use ravet_signal::{Biquad, BiquadCoefficients, ShelfKind};
#[test]
fn shelf_endpoints_midpoints_monotonicity_and_reciprocal_responses() {
    for kind in [ShelfKind::Low, ShelfKind::High] {
        for frequency in [125., 1000., 20000.] {
            for gain in [-24., -6., 0., 6., 24.] {
                for slope in [0.25, 0.5, 1.] {
                    let c = BiquadCoefficients::design_shelf(kind, 48000., frequency, gain, slope)
                        .unwrap();
                    let inverse =
                        BiquadCoefficients::design_shelf(kind, 48000., frequency, -gain, slope)
                            .unwrap();
                    let target = 10f64.powf(gain / 20.);
                    let (dc, nyquist) = if kind == ShelfKind::Low {
                        (target, 1.)
                    } else {
                        (1., target)
                    };
                    assert!((c.magnitude_at(0.).unwrap() - dc).abs() < 1e-8);
                    assert!((c.magnitude_at(0.5).unwrap() - nyquist).abs() < 1e-8);
                    assert!(
                        (c.magnitude_at(frequency / 48000.).unwrap() - target.sqrt()).abs() < 1e-8
                    );
                    let increasing = (kind == ShelfKind::High && gain > 0.)
                        || (kind == ShelfKind::Low && gain < 0.);
                    let mut previous = c.magnitude_at(0.).unwrap();
                    for bin in 1..=512 {
                        let f = bin as f64 / 1024.;
                        let current = c.magnitude_at(f).unwrap();
                        if gain != 0. {
                            assert!(if increasing {
                                current >= previous - 1e-10
                            } else {
                                current <= previous + 1e-10
                            });
                        }
                        assert!((current * inverse.magnitude_at(f).unwrap() - 1.).abs() < 1e-8);
                        previous = current;
                    }
                }
            }
        }
    }
}
#[test]
fn cascaded_boost_cut_impulse_partition_reset_and_heap() {
    for kind in [ShelfKind::Low, ShelfKind::High] {
        let boost = BiquadCoefficients::design_shelf(kind, 48000., 1000., 18., 0.75).unwrap();
        let cut = BiquadCoefficients::design_shelf(kind, 48000., 1000., -18., 0.75).unwrap();
        let mut a = Biquad::new(boost);
        let mut b = Biquad::new(cut);
        let counts = allocation_counter::measure(|| {
            for iteration in 0..3 {
                a.reset();
                b.reset();
                for frame in 0..4096 {
                    let input = if frame == 0 { 1. } else { 0. };
                    let output = b.process_sample(a.process_sample(input));
                    assert!((output - input).abs() < 1e-11, "{iteration}/{frame}");
                }
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.bytes_total, 0);
        let mut whole = Biquad::new(boost);
        let mut split = Biquad::new(boost);
        let mut x = [0.; 256];
        x[0] = 1.;
        let mut y = x;
        whole.process_in_place(&mut x);
        split.process_in_place(&mut y[..13]);
        split.process_in_place(&mut y[13..]);
        assert_eq!(x, y);
    }
}
#[test]
fn invalid_shelf_designs_reject() {
    for args in [
        (0., 1000., 6., 1.),
        (48000., 0., 6., 1.),
        (48000., 24000., 6., 1.),
        (48000., 1000., 121., 1.),
        (48000., 1000., 6., 0.),
        (48000., 1000., 6., 1.1),
        (f64::INFINITY, 1000., 6., 1.),
        (48000., f64::NAN, 6., 1.),
        (48000., 1000., f64::INFINITY, 1.),
        (48000., 1000., 6., f64::NAN),
    ] {
        for kind in [ShelfKind::Low, ShelfKind::High] {
            assert!(
                BiquadCoefficients::design_shelf(kind, args.0, args.1, args.2, args.3).is_err()
            );
        }
    }
}
