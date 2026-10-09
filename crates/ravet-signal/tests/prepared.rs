use ravet_signal::{
    Adsr, AdsrConfig, Biquad, BiquadCoefficients, Complex, FftPlan, FilterKind, FirConvolver,
    Oversampler, OversamplingFactor,
};

#[test]
fn prepared_numerical_kernels_and_resets_do_not_allocate_or_deallocate() {
    let plan = FftPlan::new(64).unwrap();
    let mut spectrum = [Complex::new(0.5, -0.25); 64];
    let mut convolver = FirConvolver::new(&[0.25; 32]).unwrap();
    let mut filter =
        Biquad::new(BiquadCoefficients::design(FilterKind::LowPass, 48000., 1000., 0.7).unwrap());
    let mut oversampler = Oversampler::new(OversamplingFactor::Eight);
    let mut envelope = Adsr::new(AdsrConfig::new(48000., 0.001, 0.002, 0.5, 0.003).unwrap());
    let counts = allocation_counter::measure(|| {
        for block in 0..1000 {
            if block % 10 == 0 {
                envelope.note_on();
            }
            plan.forward(&mut spectrum).unwrap();
            plan.inverse(&mut spectrum).unwrap();
            let mut output = 0.0;
            for index in 0..64 {
                if index == 32 && block % 10 == 5 {
                    envelope.note_off();
                }
                std::hint::black_box(envelope.process_sample());
                output =
                    filter.process_sample(convolver.process_sample((index as f64 * 0.3).sin()));
                output += f64::from(oversampler.process_sample(output as f32, f32::tanh));
            }
            std::hint::black_box(output);
            if block % 10 == 0 {
                filter.reset();
                convolver.reset();
                oversampler.reset();
                envelope.reset();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(counts.bytes_current, 0);
}
