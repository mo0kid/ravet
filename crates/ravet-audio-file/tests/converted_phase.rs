#![cfg(feature = "resampled-playback")]
#[cfg(test)]
mod tests {
    const CEILING: u64 = 16_777_216;
    // Primary backend scheduling, with an independent rational coordinate oracle.
    fn recurrence(n: u64, taps: u64, input: u64, output: u64) -> (u64, f64, f64) {
        let target =
            ((u128::from(n + taps - 2) * u128::from(output)) / u128::from(input) + 1) as u64;
        let step = input as f64 / output as f64;
        let mut distance = 1.;
        let mut consumed = 0u64;
        let mut produced = 0u64;
        let mut max_error: f64 = 0.;
        while produced < target {
            if consumed == 0 || distance > 0. {
                consumed += 1;
                distance -= 1.;
            } else {
                if produced.is_multiple_of(1_048_576) || produced + 1 == target {
                    let coordinate = u128::from(produced) * u128::from(input);
                    let integer = coordinate / u128::from(output);
                    let fraction = (coordinate % u128::from(output)) as f64 / output as f64;
                    let ideal = (integer as i128 - (consumed - 1) as i128) as f64 + fraction;
                    max_error = max_error.max((distance - ideal).abs());
                }
                produced += 1;
                distance += step;
            }
        }
        let c = (n + taps) as f64;
        let m = (16 * (n + taps) + 2) as f64;
        let bound = c * 2f64.powi(-54) + m * 32. * 2f64.powi(-53);
        assert!(
            max_error < bound,
            "{input}/{output} error{max_error:e} bound{bound:e}"
        );
        assert!(bound < 1. / 768000.);
        assert!(
            consumed <= n + taps,
            "{input}/{output} consumed{consumed} target{target}"
        );
        (consumed, max_error, bound)
    }
    #[test]
    fn ceiling_float_schedule_and_support_match_independent_integer_coordinates() {
        for (input, output) in [
            (48001, 768000),
            (768000, 48001),
            (44100, 48000),
            (48000, 44100),
            (47999, 48000),
            (48000, 47999),
            (48000, 48000),
        ] {
            let (consumed, error, bound) = recurrence(CEILING, 256, input, output);
            eprintln!(
                "phase {input}/{output} N{CEILING} padding{} error{error:e} global_bound{bound:e}",
                consumed.saturating_sub(CEILING)
            );
        }
    }
    #[test]
    fn actual_converter_ceiling_terminal_impulse_retains_tail_with_bounded_storage() {
        use ravet_signal::SincResampler;
        let n = CEILING as usize;
        let taps = 16;
        let input = 48000u64;
        let output = 44100u64;
        let target = ((n + taps - 2) as u128 * output as u128 / input as u128 + 1) as usize;
        let mut converter = SincResampler::new(input as f64, output as f64, taps).unwrap();
        let mut source = [0.; 4096];
        let mut buffer = [0.; 4096];
        let mut consumed = 0;
        let mut produced = 0;
        let mut tail = Vec::new();
        while consumed < n {
            let count = (n - consumed).min(source.len());
            source.fill(0.);
            if consumed + count == n {
                source[count - 1] = 1.;
            }
            let mut offset = 0;
            loop {
                let p = converter.process(&source[offset..count], &mut buffer);
                offset += p.consumed;
                for (frame, sample) in buffer[..p.produced].iter().copied().enumerate() {
                    if produced + frame >= target - 64 {
                        tail.push(sample);
                    }
                }
                produced += p.produced;
                if p.consumed == 0 && p.produced == 0 {
                    break;
                }
            }
            assert_eq!(offset, count);
            consumed += count;
        }
        let mut padding = 0;
        source.fill(0.);
        while produced < target {
            let p = converter.process(
                &source[..taps - padding],
                &mut buffer[..(target - produced).min(4096)],
            );
            padding += p.consumed;
            for (frame, sample) in buffer[..p.produced].iter().copied().enumerate() {
                if produced + frame >= target - 64 {
                    tail.push(sample);
                }
            }
            produced += p.produced;
            assert!(p.consumed != 0 || p.produced != 0);
            assert!(padding <= taps);
        }
        assert_eq!(produced, target);
        assert_eq!(tail.len(), 64);
        assert!(tail.iter().all(|v| v.is_finite()));
        assert!(tail.iter().any(|v| v.abs() > 0.1));
        let (model_consumed, phase_error, _) = recurrence(n as u64, taps as u64, input, output);
        assert_eq!(padding, model_consumed as usize - n);
        // Independent phase reference: the expected delay is7.5 original samples.
        let peak = tail
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0;
        let peak_coordinate = (target - 64 + peak) as f64 * input as f64 / output as f64;
        assert!((peak_coordinate - (n - 1) as f64 - 7.5).abs() < 1.);
        eprintln!(
            "actual N{n} target{target} padding{padding} peak_coordinate{peak_coordinate} phase_error{phase_error:e}"
        );
    }
}
