use ravet_signal::{
    Biquad, BiquadCoefficients, FilterKind, StateVariableCoefficients, StateVariableFilter,
};

#[test]
fn three_outputs_match_independent_biquad_impulses() {
    for normalized in [0.001, 0.03, 0.2, 0.45] {
        for q in [0.1, 0.707, 1., 10., 100.] {
            let cutoff = normalized * 48000.;
            let mut svf = StateVariableFilter::new(
                StateVariableCoefficients::new(48000., cutoff, q).unwrap(),
            );
            let mut low = Biquad::new(
                BiquadCoefficients::design(FilterKind::LowPass, 48000., cutoff, q).unwrap(),
            );
            let mut band = Biquad::new(
                BiquadCoefficients::design(FilterKind::BandPass, 48000., cutoff, q).unwrap(),
            );
            let mut high = Biquad::new(
                BiquadCoefficients::design(FilterKind::HighPass, 48000., cutoff, q).unwrap(),
            );
            for i in 0..4096 {
                let input = if i == 0 { 1. } else { 0. };
                let out = svf.process_sample(input);
                for (actual, expected) in [
                    (out.low_pass, low.process_sample(input)),
                    (out.band_pass, q * band.process_sample(input)),
                    (out.high_pass, high.process_sample(input)),
                ] {
                    assert!(
                        (actual - expected).abs() < 2e-10,
                        "{normalized}/{q}/{i}: {actual}/{expected}"
                    );
                }
                assert!((out.high_pass + out.band_pass / q + out.low_pass - input).abs() < 1e-12);
            }
        }
    }
}

#[test]
fn changing_coefficients_matches_nodal_solve_preserves_history_and_allocates_nothing() {
    let mut actual =
        StateVariableFilter::new(StateVariableCoefficients::new(48000., 1000., 0.7).unwrap());
    let mut state = [0.; 2];
    // Independent simultaneous nodal solution, deliberately without the
    // production's grouped a1/a2/a3 update. Reconfigure with live histories.
    let counts = allocation_counter::measure(|| {
        for i in 0..10000 {
            let cutoff = if i < 1000 {
                1000.
            } else {
                2000. + (i % 31) as f64 * 100.
            };
            let q = if i < 1000 { 0.7 } else { 1.2 };
            actual.set_coefficients(StateVariableCoefficients::new(48000., cutoff, q).unwrap());
            let input = (i as f64 * 0.17).sin();
            let g = (std::f64::consts::PI * cutoff / 48000.).tan();
            let band = (state[0] + g * (input - state[1])) / (1. + g / q + g * g);
            let low = state[1] + g * band;
            let high = input - band / q - low;
            state = [2. * band - state[0], 2. * low - state[1]];
            let output = actual.process_sample(input);
            assert!((output.low_pass - low).abs() < 1e-12);
            assert!((output.band_pass - band).abs() < 1e-12);
            assert!((output.high_pass - high).abs() < 1e-12);
        }
        actual.reset();
        let output = actual.process_sample(0.);
        assert_eq!(
            (output.low_pass, output.band_pass, output.high_pass),
            (0., 0., 0.)
        );
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.bytes_total, 0);
}

#[test]
fn design_bounds_extremes_and_nonfinite_reset_policy() {
    for (rate, cutoff, q) in [
        (0., 1000., 1.),
        (f64::NAN, 1000., 1.),
        (48000., 0., 1.),
        (48000., 24000., 1.),
        (48000., f64::INFINITY, 1.),
        (48000., 1000., 0.09),
        (48000., 1000., 101.),
        (48000., 1000., f64::NAN),
    ] {
        assert!(StateVariableCoefficients::new(rate, cutoff, q).is_err());
    }
    for normalized in [1e-6, 0.499999] {
        for q in [0.1, 100.] {
            let mut filter = StateVariableFilter::new(
                StateVariableCoefficients::new(1., normalized, q).unwrap(),
            );
            for i in 0..10000 {
                let out = filter.process_sample(if i == 0 { 1. } else { 0. });
                assert!(
                    out.low_pass.is_finite()
                        && out.band_pass.is_finite()
                        && out.high_pass.is_finite()
                );
            }
        }
    }
    let mut filter =
        StateVariableFilter::new(StateVariableCoefficients::new(48000., 1000., 1.).unwrap());
    assert!(filter.process_sample(f64::NAN).low_pass.is_nan());
    assert!(filter.process_sample(0.).low_pass.is_nan());
    filter.reset();
    assert!(filter.process_sample(1.).low_pass.is_finite());
}
