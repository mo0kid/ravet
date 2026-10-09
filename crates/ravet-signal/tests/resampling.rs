use ravet_signal::SincResampler;
fn convert(input: &[f64], rate: f64, input_chunk: usize, output_chunk: usize) -> Vec<f64> {
    let mut converter = SincResampler::new(48000., rate, 64).unwrap();
    let mut result = Vec::new();
    let mut consumed = 0;
    loop {
        let end = (consumed + input_chunk).min(input.len());
        let mut output = vec![99.; output_chunk];
        let progress = converter.process(&input[consumed..end], &mut output);
        consumed += progress.consumed;
        result.extend_from_slice(&output[..progress.produced]);
        assert!(output[progress.produced..].iter().all(|v| *v == 99.));
        if consumed == input.len() && progress.produced == 0 {
            break;
        }
        assert!(progress.consumed != 0 || progress.produced != 0);
    }
    result
}
#[test]
fn arbitrary_ratios_preserve_partitioning_dc_and_counts() {
    let input = vec![1.; 4096];
    for rate in [3000., 32000., 44100., 48000., 96000., 768000.] {
        let a = convert(&input, rate, 4096, 100000);
        let b = convert(&input, rate, 7, 3);
        assert_eq!(a, b);
        let count = (((input.len() - 1) as f64 * rate / 48000.).floor() as usize) + 1;
        assert!((a.len() as isize - count as isize).abs() <= 1);
        let skip = ((100. * rate / 48000.) as usize).max(20);
        for sample in &a[skip..] {
            assert!((sample - 1.).abs() < 1e-12);
        }
    }
}
#[test]
fn resampler_delay_passband_and_downsample_rejection() {
    let converter = SincResampler::new(48000., 24000., 64).unwrap();
    assert_eq!(converter.latency_input_samples(), 31.5);
    let sine = |frequency: f64| {
        (0..4096)
            .map(|i| (std::f64::consts::TAU * frequency * i as f64 / 48000.).sin())
            .collect::<Vec<_>>()
    };
    let low = convert(&sine(1000.), 24000., 17, 13);
    let high = convert(&sine(18000.), 24000., 17, 13);
    let rms = |values: &[f64]| {
        (values[100..].iter().map(|v| v * v).sum::<f64>() / (values.len() - 100) as f64).sqrt()
    };
    assert!((rms(&low) - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.003);
    assert!(rms(&high) < 0.001);
}
#[test]
fn output_backpressure_zero_capacity_reset_and_heap() {
    let mut converter = SincResampler::new(48000., 96000., 32).unwrap();
    let mut empty = [];
    assert_eq!(converter.process(&[1., 2.], &mut empty).consumed, 0);
    let mut one = [99.];
    let p = converter.process(&[1., 2.], &mut one);
    assert_eq!((p.consumed, p.produced), (1, 1));
    let p = converter.process(&[2.], &mut one);
    assert_eq!((p.consumed, p.produced), (1, 1));
    let p = converter.process(&[], &mut one);
    assert_eq!((p.consumed, p.produced), (0, 1));
    converter.reset();
    let mut baseline = converter.clone();
    let mut a = [99.; 16];
    let mut b = [99.; 16];
    assert_eq!(
        converter.process(&[0.5; 8], &mut a),
        baseline.process(&[0.5; 8], &mut b)
    );
    assert_eq!(a, b);
    let counts = allocation_counter::measure(|| {
        let mut output = [0.; 64];
        for _ in 0..1000 {
            std::hint::black_box(converter.process(&[0.5; 32], &mut output));
            converter.reset();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert!(SincResampler::new(48000., 1., 32).is_err());
    assert!(SincResampler::new(48000., 96000., 17).is_err());
}
