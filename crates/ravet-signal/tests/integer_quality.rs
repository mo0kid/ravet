use ravet_signal::SincResampler;
fn convert(input: &[f64], input_rate: u32, output_rate: u32, exact: bool) -> Vec<f64> {
    let mut converter = if exact {
        SincResampler::new_integer_rates(input_rate, output_rate, 64)
    } else {
        SincResampler::new(f64::from(input_rate), f64::from(output_rate), 64)
    }
    .unwrap();
    let mut consumed = 0;
    let mut result = Vec::new();
    let mut output = [99.0; 113];
    loop {
        let end = (consumed + 17).min(input.len());
        let p = converter.process(&input[consumed..end], &mut output);
        consumed += p.consumed;
        result.extend_from_slice(&output[..p.produced]);
        if p.consumed == 0 && p.produced == 0 {
            assert_eq!(consumed, input.len());
            break;
        }
    }
    result
}
#[test]
fn integer_profile_retains_delay_passband_rejection_and_documents_legacy_pcm_difference() {
    let sine = |rate: u32, freq: f64| {
        (0..16385)
            .map(|i| (std::f64::consts::TAU * freq * i as f64 / f64::from(rate)).sin() * 0.4)
            .collect::<Vec<_>>()
    };
    for (input_rate, output_rate) in [
        (44100, 48000),
        (48000, 44100),
        (48000, 24000),
        (96000, 96000),
    ] {
        let input = sine(input_rate, 977.0);
        let exact = convert(&input, input_rate, output_rate, true);
        let legacy = convert(&input, input_rate, output_rate, false);
        let n = exact.len().min(legacy.len());
        let signal = exact[100..n].iter().map(|v| v * v).sum::<f64>();
        let error = exact[100..n]
            .iter()
            .zip(&legacy[100..n])
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>();
        let snr = 10.0 * (signal / error).log10();
        let max = exact[..n]
            .iter()
            .zip(&legacy[..n])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        println!(
            "integer_vs_legacy {input_rate}/{output_rate} frames={}/{} whole_max_abs={max:e} interior_snr_db={snr}",
            exact.len(),
            legacy.len()
        );
        assert!(
            snr > 45.0,
            "unexpected scheduling-profile PCM difference {snr}dB"
        );
        let rms = (signal / (n - 100) as f64).sqrt();
        assert!((rms - 0.4 * std::f64::consts::FRAC_1_SQRT_2).abs() < 0.003);
        let expected =
            ((input.len() as u128 - 1) * u128::from(output_rate)) / u128::from(input_rate) + 1;
        assert_eq!(exact.len() as u128, expected);
    }
    let high = convert(&sine(48000, 18000.0), 48000, 24000, true);
    let rms = (high[100..].iter().map(|v| v * v).sum::<f64>() / (high.len() - 100) as f64).sqrt();
    assert!(rms < 0.001);
    assert_eq!(
        SincResampler::new_integer_rates(48000, 24000, 64)
            .unwrap()
            .latency_input_samples(),
        31.5
    );
}
