use ravet_signal::{OscillatorError, WavetableOscillator};
#[test]
fn exact_period_linear_segments_and_wrapped_last_interval() {
    let mut oscillator = WavetableOscillator::new(16., 1., &[0., 1., 0., -1.]).unwrap();
    let expected = [
        0., 0.25, 0.5, 0.75, 1., 0.75, 0.5, 0.25, 0., -0.25, -0.5, -0.75, -1., -0.75, -0.5, -0.25,
    ];
    for _ in 0..100 {
        for value in expected {
            assert_eq!(oscillator.process_sample(), value);
        }
    }
    assert_eq!(oscillator.phase(), 0.);
    oscillator.set_frequency(0.).unwrap();
    for _ in 0..100 {
        assert_eq!(oscillator.process_sample(), 0.);
    }
    // Non-power-of-two tables also wrap, without a duplicated endpoint.
    let mut oscillator = WavetableOscillator::new(6., 1., &[0., 1., -1.]).unwrap();
    for value in [0., 0.5, 1., 0., -1., -0.5] {
        assert!((oscillator.process_sample() - value).abs() < 1e-14);
    }
}
#[test]
fn sine_accuracy_phase_continuity_partition_reset_and_heap_policy() {
    let table: Vec<_> = (0..4096)
        .map(|i| (std::f64::consts::TAU * i as f64 / 4096.).sin())
        .collect();
    let mut oscillator = WavetableOscillator::new(48000., 997., &table).unwrap();
    for _ in 0..10000 {
        let expected = (std::f64::consts::TAU * oscillator.phase()).sin();
        assert!((oscillator.process_sample() - expected).abs() < 3e-7);
    }
    let phase = oscillator.phase();
    oscillator.set_frequency(1319.).unwrap();
    assert_eq!(oscillator.phase(), phase);
    oscillator.reset();
    assert_eq!(oscillator.phase(), 0.);
    assert_eq!(oscillator.process_sample(), 0.);
    assert_eq!(oscillator.phase(), 1319. / 48000.);
    let mut whole = WavetableOscillator::new(48000., 997., &table).unwrap();
    let mut split = whole.clone();
    let mut a = [0.; 1024];
    let mut b = a;
    whole.process(&mut a);
    for block in b.chunks_mut(17) {
        split.process(block);
    }
    assert_eq!(a, b);
    let counts = allocation_counter::measure(|| {
        split.reset();
        split.set_frequency(1200.).unwrap();
        split.process(&mut b);
        for _ in 0..10000 {
            std::hint::black_box(split.process_sample());
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
#[test]
fn invalid_input_rejects_without_allocation_and_frequency_edit_preserves_history() {
    let oversized = vec![0.; 65537];
    let counts = allocation_counter::measure(|| {
        for table in [
            &[][..],
            &[0.][..],
            &[0., f64::NAN][..],
            &[f64::INFINITY, 0.][..],
            oversized.as_slice(),
        ] {
            assert!(WavetableOscillator::new(48000., 440., table).is_err());
        }
        for rate in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(WavetableOscillator::new(rate, 0., &[0., 1.]).is_err());
        }
        for frequency in [-1., 24000., f64::NAN, f64::INFINITY] {
            assert!(WavetableOscillator::new(48000., frequency, &[0., 1.]).is_err());
        }
    });
    assert_eq!(counts.count_total, 0);
    let maximum = WavetableOscillator::new(48000., 0., &vec![0.; 65536]).unwrap();
    assert_eq!(maximum.table().len(), 65536);
    let mut a = WavetableOscillator::new(48000., 4000., &[0., 1., 0., -1.]).unwrap();
    let mut b = a.clone();
    for _ in 0..7 {
        assert_eq!(a.process_sample(), b.process_sample());
    }
    for frequency in [-1., 24000., f64::NAN, f64::INFINITY] {
        assert_eq!(a.set_frequency(frequency), Err(OscillatorError));
    }
    for _ in 0..1000 {
        assert_eq!(a.process_sample(), b.process_sample());
    }
}
