use ravet_signal::{ResamplingError, SincResampler};
fn render(input: &[f64], input_chunk: usize, output_chunk: usize) -> Vec<f64> {
    let mut converter = SincResampler::new_variable(48000., 48000., 64, 10000.).unwrap();
    let mut output = Vec::new();
    let mut consumed = 0;
    for (boundary, ppm) in [
        (257, 0.),
        (511, 10000.),
        (1024, -10000.),
        (2048, 300.),
        (4096, -300.),
    ] {
        converter.set_rate_adjustment_ppm(ppm).unwrap();
        while output.len() < boundary {
            let end = (consumed + input_chunk).min(input.len());
            let capacity = output_chunk.min(boundary - output.len());
            let mut scratch = vec![99.; capacity];
            let progress = converter.process(&input[consumed..end], &mut scratch);
            assert!(progress.consumed > 0 || progress.produced > 0);
            consumed += progress.consumed;
            output.extend_from_slice(&scratch[..progress.produced]);
            assert!(scratch[progress.produced..].iter().all(|v| *v == 99.));
        }
    }
    output
}
#[test]
fn scheduled_changes_keep_partition_identity_dc_and_signal_continuity() {
    let input: Vec<_> = (0..8192)
        .map(|i| (std::f64::consts::TAU * 1000. * i as f64 / 48000.).sin())
        .collect();
    let whole = render(&input, 8192, 512);
    let split = render(&input, 7, 3);
    assert_eq!(whole, split);
    assert!(
        whole[128..]
            .windows(2)
            .all(|pair| (pair[0] - pair[1]).abs() < 0.15)
    );
    let dc = render(&vec![1.; 8192], 5, 1);
    assert!(dc[128..].iter().all(|x| (x - 1.).abs() < 1e-12));
}
#[test]
fn constant_adjustments_match_independent_output_counts_and_live_history_reset() {
    let input = vec![1.; 100000];
    for ppm in [-10000., -500., 0., 500., 10000.] {
        let mut converter = SincResampler::new_variable(48000., 48000., 32, 10000.).unwrap();
        converter.set_rate_adjustment_ppm(ppm).unwrap();
        assert_eq!(converter.rate_step(), 1. + ppm * 1e-6);
        let mut consumed = 0;
        let mut produced = 0;
        let mut output = [99.; 123];
        loop {
            let p = converter.process(&input[consumed..], &mut output);
            consumed += p.consumed;
            produced += p.produced;
            if consumed == input.len() && p.produced == 0 {
                break;
            }
        }
        let expected = 1. + ((input.len() - 1) as f64 / (1. + ppm * 1e-6)).floor();
        assert!(
            (produced as f64 - expected).abs() <= 1.,
            "{ppm}: {produced}/{expected}"
        );
        converter.reset();
        assert_eq!(converter.rate_step(), 1. + ppm * 1e-6);
        let mut fresh = SincResampler::new_variable(48000., 48000., 32, 10000.).unwrap();
        fresh.set_rate_adjustment_ppm(ppm).unwrap();
        let mut a = [99.; 16];
        let mut b = a;
        assert_eq!(
            converter.process(&[0.5; 16], &mut a),
            fresh.process(&[0.5; 16], &mut b)
        );
        assert_eq!(a, b);
    }
}
#[test]
fn prepared_bounds_atomic_rejection_pending_phase_and_heap_policy() {
    for ppm in [0., -1., 10001., f64::NAN, f64::INFINITY] {
        assert!(SincResampler::new_variable(48000., 48000., 32, ppm).is_err());
    }
    assert!(SincResampler::new_variable(16., 1., 32, 1.).is_err());
    assert!(SincResampler::new_variable(1., 16., 32, 1.).is_err());
    let mut fixed = SincResampler::new(48000., 48000., 32).unwrap();
    assert_eq!(fixed.maximum_adjustment_ppm(), 0.);
    assert_eq!(fixed.set_rate_adjustment_ppm(1.), Err(ResamplingError));
    fixed.set_rate_adjustment_ppm(0.).unwrap();
    let mut converter = SincResampler::new_variable(48000., 96000., 32, 10000.).unwrap();
    let mut one = [99.];
    let p = converter.process(&[1.], &mut one);
    assert_eq!((p.consumed, p.produced), (1, 1));
    let mut previous = converter.clone();
    converter.set_rate_adjustment_ppm(10000.).unwrap();
    // Both retain the same already scheduled output position; subsequent
    // output spacing uses the new step, without replacing filter history.
    let mut a = [99.; 1];
    let mut b = a;
    assert_eq!(
        converter.process(&[2.], &mut a),
        previous.process(&[2.], &mut b)
    );
    assert_eq!(a, b);
    let baseline = converter.clone();
    for ppm in [-10001., 10001., f64::NAN, f64::INFINITY] {
        assert_eq!(converter.set_rate_adjustment_ppm(ppm), Err(ResamplingError));
    }
    let mut reference = baseline;
    let mut a = [99.; 64];
    let mut b = a;
    assert_eq!(
        converter.process(&[0.5; 32], &mut a),
        reference.process(&[0.5; 32], &mut b)
    );
    assert_eq!(a, b);
    let counts = allocation_counter::measure(|| {
        for i in 0..1000 {
            converter
                .set_rate_adjustment_ppm(if i % 2 == 0 { 500. } else { -500. })
                .unwrap();
            converter.process(&[0.5; 32], &mut a);
            if i % 100 == 0 {
                converter.reset();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.bytes_total, 0);
}

#[test]
fn variable_preflight_matches_constructor_bounds_and_allocates_nothing() {
    let cases = [
        (48000., 44100., 32, 2000.),
        (1., 1., 16, 10000.),
        (48000., 44100., 31, 2000.),
        (0., 1., 32, 100.),
        (f64::NAN, 1., 32, 100.),
        (1., f64::INFINITY, 32, 100.),
        (16., 1., 32, 1.),
        (1., 16., 32, 1.),
        (1., 1., 32, 0.),
        (1., 1., 32, 10001.),
        (1., 1., 32, f64::NAN),
        (1e-300, 1e300, 32, 100.),
    ];
    for (input, output, taps, ppm) in cases {
        let expected = SincResampler::new_variable(input, output, taps, ppm).is_ok();
        let allocation = allocation_counter::measure(|| {
            for _ in 0..1000 {
                assert_eq!(
                    SincResampler::validate_variable(input, output, taps, ppm).is_ok(),
                    expected
                );
            }
        });
        assert_eq!(allocation.count_total, 0);
    }
}
