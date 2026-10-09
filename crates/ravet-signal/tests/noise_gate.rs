use ravet_signal::{NoiseGate, NoiseGateConfig};
fn config(knee: f64) -> NoiseGateConfig {
    NoiseGateConfig::new(48000., -20., 4., knee, 0., 0., -60.).unwrap()
}
#[test]
fn static_expansion_floor_sidechain_and_soft_knee() {
    let c = config(0.);
    // Input -30 dB is ten below threshold: output -60, gain -30 dB.
    let low = 10f64.powf(-30. / 20.);
    assert!((20. * c.gain_for_level(low).unwrap().log10() + 30.).abs() < 1e-12);
    assert_eq!(c.gain_for_level(0.1).unwrap(), 1.);
    assert_eq!(c.gain_for_level(1.).unwrap(), 1.);
    assert_eq!(c.gain_for_level(0.).unwrap(), 0.001);
    assert_eq!(c.gain_for_level(1e-200).unwrap(), 0.001);
    let mut gate = NoiseGate::new(c);
    assert_eq!(gate.process_sample(0.5, 1.), 0.5);
    assert_eq!(gate.process_sample(0.5, 0.), 0.0005);
    let knee = config(6.);
    // At threshold the quadratic joins at -ratioSlope*knee/8 dB.
    assert!((20. * knee.gain_for_level(0.1).unwrap().log10() + 2.25).abs() < 1e-12);
    for (db, expected) in [(-23., -9.), (-17., 0.)] {
        let level = 10f64.powf(db / 20.);
        assert!((20. * knee.gain_for_level(level).unwrap().log10() - expected).abs() < 1e-12);
        assert!(
            (knee.gain_for_level(level * (1. - 1e-8)).unwrap()
                - knee.gain_for_level(level * (1. + 1e-8)).unwrap())
            .abs()
                < 1e-7
        );
    }
    let unity = NoiseGateConfig::new(48000., -20., 1., 6., 0., 0., -60.).unwrap();
    assert_eq!(unity.gain_for_level(0.).unwrap(), 1.);
    for bad in [-1., f64::NAN, f64::INFINITY] {
        assert!(c.gain_for_level(bad).is_err());
    }
}
#[test]
fn detector_timing_reset_and_prepared_heap_contract() {
    let mut gate =
        NoiseGate::new(NoiseGateConfig::new(1000., -20., 4., 6., 0.01, 0.02, -60.).unwrap());
    for n in 1..=40 {
        gate.process_sample(0., -1.);
        assert!((gate.detector_level() - (1. - (-(n as f64) / 10.).exp())).abs() < 1e-14);
    }
    let start = gate.detector_level();
    for n in 1..=40 {
        gate.process_sample(0., 0.);
        assert!((gate.detector_level() - start * (-(n as f64) / 20.).exp()).abs() < 1e-14);
    }
    gate.reset();
    assert_eq!(gate.detector_level(), 0.);
    let counts = allocation_counter::measure(|| {
        for i in 0..10000 {
            std::hint::black_box(gate.process_sample(0.5, if i % 2 == 0 { 0.01 } else { 1. }));
            if i % 100 == 0 {
                gate.reset();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.bytes_total, 0);
}
#[test]
fn configuration_rejects_nonfinite_and_out_of_range_values() {
    for args in [
        (0., -20., 4., 6., 0.01, 0.02, -60.),
        (48000., -121., 4., 6., 0.01, 0.02, -60.),
        (48000., -20., 0.5, 6., 0.01, 0.02, -60.),
        (48000., -20., 4., 49., 0.01, 0.02, -60.),
        (48000., -20., 4., 6., -0.1, 0.02, -60.),
        (48000., -20., 4., 6., 0.01, 61., -60.),
        (48000., -20., 4., 6., 0.01, 0.02, 1.),
        (48000., -20., 4., 6., 0.01, 0.02, f64::NAN),
    ] {
        assert!(
            NoiseGateConfig::new(args.0, args.1, args.2, args.3, args.4, args.5, args.6).is_err()
        );
    }
}
