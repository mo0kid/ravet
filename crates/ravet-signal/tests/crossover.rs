use ravet_signal::{
    Biquad, BiquadCoefficients, CrossoverLengthError, CrossoverOutputs, FilterKind,
    LinkwitzRileyCoefficients, LinkwitzRileyCrossover,
};

#[test]
fn branch_amplitudes_match_independent_bilinear_response() {
    for rate in [44100., 48000., 96000.] {
        for cutoff in [80., 1000., rate * 0.4] {
            let c = LinkwitzRileyCoefficients::design(rate, cutoff).unwrap();
            let center = c.magnitudes_at(cutoff / rate).unwrap();
            assert!((center.low - 0.5).abs() < 1e-9);
            assert!((center.high - 0.5).abs() < 1e-9);
            for frequency in [0., 0.0001, 0.001, 0.01, 0.1, 0.3, 0.49, 0.5] {
                let ratio = (std::f64::consts::PI * frequency).tan()
                    / (std::f64::consts::PI * cutoff / rate).tan();
                let low = 1. / (1. + ratio.powi(4));
                let response = c.magnitudes_at(frequency).unwrap();
                assert!((response.low - low).abs() < 1e-9);
                assert!((response.high - (1. - low)).abs() < 1e-9);
                assert!((response.low + response.high - 1.).abs() < 1e-9);
            }
        }
    }
}

#[test]
fn recombination_matches_independent_second_order_allpass() {
    for cutoff in [80., 1000., 19000.] {
        let mut crossover =
            LinkwitzRileyCrossover::new(LinkwitzRileyCoefficients::design(48000., cutoff).unwrap());
        let mut allpass = Biquad::new(
            BiquadCoefficients::design(
                FilterKind::AllPass,
                48000.,
                cutoff,
                std::f64::consts::FRAC_1_SQRT_2,
            )
            .unwrap(),
        );
        let mut energy = 0.;
        for i in 0..32768 {
            let input = if i == 0 { 1. } else { 0. };
            let output = crossover.process_sample(input);
            let sum = output.low + output.high;
            assert!((sum - allpass.process_sample(input)).abs() < 1e-11);
            energy += sum * sum;
        }
        assert!((energy - 1.).abs() < 1e-10);
    }
}

#[test]
fn rendered_cutoff_partition_and_reset_match() {
    let c = LinkwitzRileyCoefficients::design(48000., 1500.).unwrap();
    let input: [f64; 8192] =
        std::array::from_fn(|i| (std::f64::consts::TAU * i as f64 / 32.).sin());
    let mut whole = LinkwitzRileyCrossover::new(c);
    let mut split = whole.clone();
    let mut low = [0.; 8192];
    let mut high = low;
    whole.process_block(&input, &mut low, &mut high).unwrap();
    let mut split_low = [0.; 8192];
    let mut split_high = split_low;
    for ((input, low), high) in input
        .chunks(73)
        .zip(split_low.chunks_mut(73))
        .zip(split_high.chunks_mut(73))
    {
        split.process_block(input, low, high).unwrap();
    }
    assert_eq!(low, split_low);
    assert_eq!(high, split_high);
    let mut summed_power = 0.;
    for branch in [&low, &high] {
        let power = branch[4096..].iter().map(|x| x * x).sum::<f64>() / 4096.;
        assert!((power - 0.125).abs() < 1e-12);
    }
    for i in 4096..8192 {
        summed_power += (low[i] + high[i]).powi(2) / 4096.;
    }
    assert!((summed_power - 0.5).abs() < 1e-12);
    whole.reset();
    let mut fresh = LinkwitzRileyCrossover::new(c);
    for x in input {
        assert_eq!(whole.process_sample(x), fresh.process_sample(x));
    }
}

#[test]
fn replacements_retain_histories_and_prepared_work_is_heap_free() {
    let a = LinkwitzRileyCoefficients::design(48000., 1000.).unwrap();
    let b = LinkwitzRileyCoefficients::design(48000., 3000.).unwrap();
    let mut crossover = LinkwitzRileyCrossover::new(a);
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let mut low = std::array::from_fn::<_, 2, _>(|_| {
        Biquad::new(BiquadCoefficients::design(FilterKind::LowPass, 48000., 1000., q).unwrap())
    });
    let mut high = std::array::from_fn::<_, 2, _>(|_| {
        Biquad::new(BiquadCoefficients::design(FilterKind::HighPass, 48000., 1000., q).unwrap())
    });
    for i in 0..512 {
        if i == 128 {
            crossover.set_coefficients(b);
            for stage in &mut low {
                stage.set_coefficients(
                    BiquadCoefficients::design(FilterKind::LowPass, 48000., 3000., q).unwrap(),
                );
            }
            for stage in &mut high {
                stage.set_coefficients(
                    BiquadCoefficients::design(FilterKind::HighPass, 48000., 3000., q).unwrap(),
                );
            }
        }
        let x = (i as f64 * 0.037).sin();
        let l = low[0].process_sample(x);
        let h = high[0].process_sample(x);
        assert_eq!(
            crossover.process_sample(x),
            CrossoverOutputs {
                low: low[1].process_sample(l),
                high: high[1].process_sample(h),
            }
        );
    }
    let input = [0.25; 32];
    let mut out_low = [0.; 32];
    let mut out_high = out_low;
    let counts = allocation_counter::measure(|| {
        for i in 0..10000 {
            crossover.set_coefficients(if i % 2 == 0 { a } else { b });
            crossover.reset();
            crossover
                .process_block(&input, &mut out_low, &mut out_high)
                .unwrap();
            std::hint::black_box(crossover.process_sample(1.));
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.bytes_total, 0);
}

#[test]
fn invalid_design_response_and_buffer_shapes_reject_atomically() {
    for (rate, cutoff) in [
        (0., 1.),
        (48000., 0.),
        (48000., -1.),
        (48000., 24000.),
        (f64::NAN, 1.),
        (48000., f64::NAN),
        (f64::INFINITY, 1.),
        (48000., f64::INFINITY),
        (48000., f64::MIN_POSITIVE),
    ] {
        assert!(LinkwitzRileyCoefficients::design(rate, cutoff).is_err());
    }
    let c = LinkwitzRileyCoefficients::design(48000., 1000.).unwrap();
    for frequency in [-0.1, 0.6, f64::NAN, f64::INFINITY] {
        assert!(c.magnitudes_at(frequency).is_err());
    }
    let mut crossover = LinkwitzRileyCrossover::new(c);
    crossover.process_sample(1.);
    let mut unchanged = crossover.clone();
    let mut low = [91.; 4];
    let mut high = [92.; 4];
    for (input_len, low_len, high_len) in [(4, 3, 4), (4, 4, 3), (0, 4, 4), (4, 0, 0)] {
        assert_eq!(
            crossover.process_block(
                &[1.; 4][..input_len],
                &mut low[..low_len],
                &mut high[..high_len]
            ),
            Err(CrossoverLengthError)
        );
        assert_eq!(low, [91.; 4]);
        assert_eq!(high, [92.; 4]);
    }
    crossover.process_block(&[], &mut [], &mut []).unwrap();
    for _ in 0..256 {
        assert_eq!(crossover.process_sample(0.), unchanged.process_sample(0.));
    }
}
