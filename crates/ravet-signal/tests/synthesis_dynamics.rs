use ravet_signal::{Compressor, CompressorConfig, EnvelopeFollower, Oscillator, Waveform};
#[test]
fn oscillator_phase_partition_retune_and_edge_correction() {
    let mut oscillator = Oscillator::new(48000., 6000., Waveform::Sine).unwrap();
    let mut samples = [0.; 16];
    oscillator.process(&mut samples[..3]);
    oscillator.process(&mut samples[3..]);
    for (i, sample) in samples.iter().enumerate() {
        assert!((sample - (std::f64::consts::TAU * (i % 8) as f64 / 8.).sin()).abs() < 1e-14);
    }
    oscillator.process_sample();
    let phase = oscillator.phase();
    assert!(oscillator.set_frequency(f64::NAN).is_err());
    assert_eq!(oscillator.phase(), phase);
    oscillator.set_frequency(12000.).unwrap();
    assert_eq!(oscillator.phase(), phase);
    oscillator.process_sample();
    assert_eq!(oscillator.phase(), phase + 0.25);
    oscillator.reset();
    assert_eq!(oscillator.process_sample(), 0.);
    for waveform in [Waveform::Saw, Waveform::Square] {
        let mut oscillator = Oscillator::new(48000., 6000., waveform).unwrap();
        assert_eq!(oscillator.process_sample(), 0.); // corrected wrap discontinuity
        for _ in 0..800 {
            assert!(oscillator.process_sample().abs() <= 1.);
        }
    }
    for frequency in [-1., 24000., f64::INFINITY] {
        assert!(Oscillator::new(48000., frequency, Waveform::Sine).is_err());
    }
}
#[test]
fn waveform_switch_preserves_phase_and_frequency() {
    let mut oscillator = Oscillator::new(48_000., 6_000., Waveform::Sine).unwrap();
    oscillator.process_sample();
    let phase = oscillator.phase();
    oscillator.set_waveform(Waveform::Triangle);
    assert_eq!(oscillator.phase(), phase);
    assert_eq!(oscillator.process_sample(), 1. - 4. * (phase - 0.5).abs());
    assert_eq!(oscillator.phase(), phase + 0.125);
}
#[test]
fn detector_matches_independent_step_and_release_recurrence() {
    let mut follower = EnvelopeFollower::new(1000., 0.01, 0.02).unwrap();
    for n in 1..=40 {
        let level = follower.process_sample(-1.);
        assert!((level - (1. - (-(n as f64) / 10.).exp())).abs() < 1e-14);
    }
    let start = follower.level();
    for n in 1..=40 {
        assert!((follower.process_sample(0.) - start * (-(n as f64) / 20.).exp()).abs() < 1e-14);
    }
    follower.reset();
    assert_eq!(follower.level(), 0.);
}
#[test]
fn compressor_static_ratio_knee_sidechain_and_reset() {
    let config = CompressorConfig::new(48000., -20., 4., 0., 0., 0., 0.).unwrap();
    assert_eq!(config.gain_for_level(0.).unwrap(), 1.);
    assert_eq!(config.gain_for_level(0.05).unwrap(), 1.);
    let expected = 10f64.powf(-15. / 20.); // input 0 dB becomes -15 dB
    assert!((config.gain_for_level(1.).unwrap() - expected).abs() < 1e-14);
    let mut compressor = Compressor::new(config);
    assert!((compressor.process_sample(0.5, 1.) - 0.5 * expected).abs() < 1e-14);
    assert_eq!(compressor.process_sample(0.5, 0.), 0.5); // external sidechain
    compressor.reset();
    assert_eq!(compressor.detector_level(), 0.);
    let knee = CompressorConfig::new(48000., -20., 4., 6., 0., 0., 0.).unwrap();
    assert!((20. * knee.gain_for_level(0.1).unwrap().log10() - (-0.75 * 36. / 48.)).abs() < 1e-13);
    for db in [-23., -17.] {
        let level = 10f64.powf(db / 20.);
        let a = knee.gain_for_level(level * (1. - 1e-8)).unwrap();
        let b = knee.gain_for_level(level * (1. + 1e-8)).unwrap();
        assert!((a - b).abs() < 1e-7);
    }
    assert!(config.gain_for_level(f64::NAN).is_err());
    assert!(CompressorConfig::new(48000., -20., 0.5, 0., 0., 0., 0.).is_err());
}
#[test]
fn synthesis_and_dynamics_prepared_paths_have_no_heap_operations() {
    let mut oscillator = Oscillator::new(48000., 440., Waveform::Saw).unwrap();
    let mut compressor =
        Compressor::new(CompressorConfig::new(48000., -12., 3., 6., 0.003, 0.05, 2.).unwrap());
    let counts = allocation_counter::measure(|| {
        for i in 0..10000 {
            let input = oscillator.process_sample();
            std::hint::black_box(compressor.process_sample(input, input));
            if i % 1000 == 0 {
                oscillator.reset();
                compressor.reset();
                oscillator.set_frequency(220.).unwrap();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
