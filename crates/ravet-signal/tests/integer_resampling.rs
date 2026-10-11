use ravet_signal::SincResampler;

fn rendered(input: &[f64], input_rate: u32, output_rate: u32, batches: (usize, usize)) -> Vec<f64> {
    let mut resampler = SincResampler::new_integer_rates(input_rate, output_rate, 16).unwrap();
    let mut consumed = 0;
    let mut result = Vec::new();
    let mut output = vec![123.0; batches.1];
    loop {
        let end = (consumed + batches.0).min(input.len());
        let progress = resampler.process(&input[consumed..end], &mut output);
        consumed += progress.consumed;
        result.extend_from_slice(&output[..progress.produced]);
        if progress.consumed == 0 && progress.produced == 0 {
            assert_eq!(consumed, input.len());
            break;
        }
    }
    result
}

#[test]
fn exact_output_schedule_matches_independent_absolute_integer_oracle() {
    for (input_rate, output_rate) in [
        (44100, 48000),
        (48000, 44100),
        (1, 16),
        (16, 1),
        (u32::MAX, u32::MAX - 1),
        (u32::MAX / 16, u32::MAX - 15),
    ] {
        let mut resampler = SincResampler::new_integer_rates(input_rate, output_rate, 16).unwrap();
        let input = vec![0.0; 8193];
        let mut consumed = 0;
        let mut produced = 0u128;
        let mut output = [11.0];
        loop {
            let end = (consumed + 1).min(input.len());
            let p = resampler.process(&input[consumed..end], &mut output);
            consumed += p.consumed;
            if p.produced != 0 {
                let needed =
                    1 + (produced * u128::from(input_rate)).div_ceil(u128::from(output_rate));
                assert_eq!(
                    consumed as u128, needed,
                    "{input_rate}/{output_rate} output{produced}"
                );
                assert_eq!(output, [0.0]);
                produced += 1;
            }
            if p.consumed == 0 && p.produced == 0 {
                break;
            }
        }
        assert_eq!(consumed, input.len());
        let expected =
            ((input.len() as u128 - 1) * u128::from(output_rate)) / u128::from(input_rate) + 1;
        assert_eq!(produced, expected);
    }
}

#[test]
fn arbitrary_partitions_reset_zero_adjustment_pending_drain_and_heap_contract() {
    let input: Vec<_> = (0..4097).map(|i| (i as f64 * 0.11).sin() * 0.4).collect();
    for (i, o) in [
        (44100, 48000),
        (48000, 44100),
        (8000, 128000),
        (128000, 8000),
        (96000, 96000),
    ] {
        let reference = rendered(&input, i, o, (4097, 65536));
        for batches in [(1, 1), (13, 7), (17, 31), (4096, 17)] {
            assert_eq!(rendered(&input, i, o, batches), reference);
        }
        let mut r = SincResampler::new_integer_rates(i, o, 16).unwrap();
        assert!(r.set_rate_adjustment_ppm(1.0).is_err());
        assert!(r.set_rate_adjustment_ppm(f64::NAN).is_err());
        r.set_rate_adjustment_ppm(0.0).unwrap();
        assert_eq!(r.rate_step(), f64::from(i) / f64::from(o));
        assert_eq!(r.latency_input_samples(), 7.5);
        assert_eq!(r.process(&input, &mut []).consumed, 0);
        let mut output = vec![0.0; reference.len() + 1];
        let measured = allocation_counter::measure(|| {
            let p = r.process(&input, &mut output);
            assert_eq!(p.consumed, input.len());
            assert_eq!(p.produced, reference.len());
            r.reset();
            let p = r.process(&input, &mut output);
            assert_eq!(p.produced, reference.len());
        });
        assert_eq!(measured.count_total, 0);
        assert_eq!(measured.count_current, 0);
        assert_eq!(&output[..reference.len()], reference);
    }
}

#[test]
fn invalid_integer_profiles_reject_and_equal_rates_explicitly_filter() {
    for (i, o, taps) in [
        (0, 48000, 16),
        (48000, 0, 16),
        (1, 17, 16),
        (17, 1, 16),
        (44100, 48000, 15),
        (44100, 48000, 257),
    ] {
        assert!(SincResampler::new_integer_rates(i, o, taps).is_err());
    }
    let mut input = vec![0.0; 33];
    input[0] = 1.0;
    let output = rendered(&input, 48000, 48000, (3, 7));
    assert_ne!(output, input);
    assert_eq!(output.len(), input.len());
}
