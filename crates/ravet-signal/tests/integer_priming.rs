use ravet_signal::SincResampler;
fn drain(resampler: &mut SincResampler, input: &[f64]) -> Vec<f64> {
    let mut consumed = 0;
    let mut result = Vec::new();
    let mut output = [0.0; 17];
    loop {
        let end = (consumed + 13).min(input.len());
        let p = resampler.process(&input[consumed..end], &mut output);
        consumed += p.consumed;
        result.extend_from_slice(&output[..p.produced]);
        if p.consumed == 0 && p.produced == 0 {
            assert_eq!(consumed, input.len());
            return result;
        }
    }
}
#[test]
fn finite_history_matches_streamed_suffix_at_arbitrary_rational_phases() {
    let input: Vec<_> = (0..8193).map(|n| (n as f64 * 0.113).sin()).collect();
    for (i, o) in [
        (44100, 48000),
        (48000, 44100),
        (1, 16),
        (16, 1),
        (u32::MAX, u32::MAX / 2),
        (u32::MAX, u32::MAX),
    ] {
        for taps in [16, 64, 256] {
            let profile = SincResampler::new_integer_rates(i, o, taps).unwrap();
            let full = drain(&mut profile.clone(), &input);
            for j in [0, 1, 7, 17, full.len() / 2, full.len() - 1] {
                let mut primed = profile.clone();
                let plan = primed.integer_position(j as u64).unwrap();
                let c = plan.input_frames_consumed() as usize;
                let history: Vec<_> = (0..taps)
                    .map(|k| {
                        if c + k < taps {
                            0.0
                        } else {
                            input[c + k - taps]
                        }
                    })
                    .collect();
                assert_eq!(primed.prime_integer_at(j as u64, &history).unwrap(), plan);
                assert_eq!(
                    drain(&mut primed, &input[c..]),
                    full[j..],
                    "{i}/{o}, taps={taps}, j={j}"
                );
            }
        }
    }
}
#[test]
fn full_coordinate_plan_is_exact_and_does_not_allocate() {
    for (i, o) in [
        (44100, 48000),
        (16, 1),
        (1, 16),
        (u32::MAX, u32::MAX / 16 + 1),
    ] {
        let mut p = SincResampler::new_integer_rates(i, o, 16).unwrap();
        let allocation = allocation_counter::measure(|| {
            for j in [0, 1, u64::MAX - 65536, u64::MAX] {
                let plan = p.integer_position(j).unwrap();
                let product = u128::from(j) * u128::from(i);
                let expected = 1 + product.div_ceil(u128::from(o));
                assert_eq!(plan.input_frames_consumed(), expected);
                assert_eq!(plan.output_frame(), j);
                assert_eq!(p.prime_integer_at(j, &[0.0; 16]).unwrap(), plan);
            }
        });
        assert_eq!(allocation.count_total, 0);
        assert_eq!(allocation.count_current, 0);
    }
}
#[test]
fn invalid_history_and_floating_modes_preserve_original_stream_and_reset_is_explicit() {
    let input: Vec<_> = (0..257).map(|n| n as f64 / 512.0).collect();
    for mut p in [
        SincResampler::new_integer_rates(44100, 48000, 16).unwrap(),
        SincResampler::new(44100.0, 48000.0, 16).unwrap(),
        SincResampler::new_variable(44100.0, 48000.0, 16, 100.0).unwrap(),
    ] {
        let mut expected = p.clone();
        let mut prefix = [0.0; 7];
        p.process(&input[..13], &mut prefix);
        expected.process(&input[..13], &mut prefix);
        assert!(p.prime_integer_at(17, &[0.0; 15]).is_err());
        assert!(p.prime_integer_at(17, &[f64::NAN; 16]).is_err());
        assert_eq!(
            drain(&mut p, &input[13..]),
            drain(&mut expected, &input[13..])
        );
    }
    let mut p = SincResampler::new_integer_rates(44100, 48000, 16).unwrap();
    p.prime_integer_at(123, &[1.0; 16]).unwrap();
    p.reset();
    assert_eq!(
        drain(&mut p, &input),
        drain(
            &mut SincResampler::new_integer_rates(44100, 48000, 16).unwrap(),
            &input
        )
    );
    assert!(
        SincResampler::new(48000.0, 48000.0, 16)
            .unwrap()
            .integer_position(0)
            .is_err()
    );
}
