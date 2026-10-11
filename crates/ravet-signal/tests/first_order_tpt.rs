use ravet_signal::{
    FirstOrderTptBank as B, FirstOrderTptBank32 as B32, FirstOrderTptDesign as D,
    FirstOrderTptFilter as F, FirstOrderTptFilter32 as F32, FirstOrderTptResponse as R,
};
const RESPONSES: [R; 3] = [R::LowPass, R::HighPass, R::AllPass];
fn close(a: f64, b: f64, e: f64) {
    assert!(
        (a - b).abs() <= e,
        "{a} != {b}, error{} > {e}",
        (a - b).abs()
    );
}
fn analog(gain: f64, f: f64, response: R) -> f64 {
    if f == 0. {
        return if response == R::HighPass { 0. } else { 1. };
    }
    if f == 0.5 {
        return if response == R::LowPass { 0. } else { 1. };
    }
    let prewarp = gain / (1. - gain);
    let omega = (std::f64::consts::PI * f).tan() / prewarp;
    let denominator = 1_f64.hypot(omega);
    match response {
        R::LowPass => 1. / denominator,
        R::HighPass => omega / denominator,
        R::AllPass => 1.,
    }
}
fn circuit(state: &mut f64, input: f64, gain: f64) -> [f64; 3] {
    let g = gain / (1. - gain);
    let old = *state;
    let low = (g * input + old) / (1. + g);
    *state = ((1. - g) * old + 2. * g * input) / (1. + g);
    [low, input - low, 2. * low - input]
}
#[test]
fn actual_precision_bilinear_analog_spectrum_and_requested_landmarks_for_all_responses() {
    for rate in [1., 8000., 44100., 48000., 96000., 192000., 768000.] {
        for ratio in [1e-6, 0.001, 0.01, 0.1, 0.25, 0.49, 0.499999] {
            let d = D::new(rate, rate * ratio).unwrap();
            let q = d.quantize_f32().unwrap();
            assert_eq!(d.sample_rate(), rate);
            assert_eq!(d.cutoff_hz(), rate * ratio);
            assert_eq!(q.sample_rate(), rate);
            assert_eq!(q.cutoff_hz(), rate * ratio);
            assert_eq!(q.gain(), d.gain() as f32);
            for r in RESPONSES {
                for i in 0..=1024 {
                    let f = i as f64 / 2048.;
                    close(d.magnitude_at(f, r).unwrap(), analog(d.gain(), f, r), 2e-11);
                    close(
                        q.magnitude_at(f, r).unwrap(),
                        analog(f64::from(q.gain()), f, r),
                        2e-11,
                    );
                }
                for invalid in [-1., 0.5000001, f64::NAN, f64::INFINITY] {
                    assert!(d.magnitude_at(invalid, r).is_err());
                    assert!(q.magnitude_at(invalid, r).is_err());
                }
            }
            close(
                d.magnitude_at(ratio, R::LowPass).unwrap(),
                std::f64::consts::FRAC_1_SQRT_2,
                2e-11,
            );
            close(
                d.magnitude_at(ratio, R::HighPass).unwrap(),
                std::f64::consts::FRAC_1_SQRT_2,
                2e-11,
            );
            // Rounded32 gain sets the actual spectrum independently of requested metadata.
            let actual32 =
                (f64::from(q.gain()) / (1. - f64::from(q.gain()))).atan() / std::f64::consts::PI;
            close(
                q.magnitude_at(actual32, R::LowPass).unwrap(),
                std::f64::consts::FRAC_1_SQRT_2,
                2e-11,
            );
        }
    }
    let d = D::new(48000., 1000.).unwrap();
    let q = d.quantize_f32().unwrap();
    assert_ne!(
        q.magnitude_at(1000. / 48000., R::LowPass).unwrap(),
        d.magnitude_at(1000. / 48000., R::LowPass).unwrap()
    );
}
#[test]
fn independent_polynomial_impulse_composite_transfer_and_actual_quarter_rate_profiles() {
    for ratio in [0.001, 0.01, 0.1, 0.25, 0.49] {
        let d = D::new(48000., 48000. * ratio).unwrap();
        let q = d.quantize_f32().unwrap();
        for gain32 in [false, true] {
            let gain = if gain32 {
                f64::from(q.gain())
            } else {
                d.gain()
            };
            let pole = 1. - 2. * gain;
            for impulse in [false, true] {
                let mut f = F::new(d);
                let mut f32 = F32::new(q);
                let mut previous_input = 0.;
                let mut previous = [0.; 3];
                for i in 0..4096 {
                    let x = if impulse {
                        if i == 0 { 1. } else { 0. }
                    } else {
                        (i as f64 * 0.173).sin() * 0.31 + ((i * 17 % 23) as f64 - 11.) / 71.
                    };
                    let x = if gain32 { x as f32 as f64 } else { x };
                    // Separate direct polynomial difference equations, no capacitor update.
                    let expected = [
                        pole * previous[0] + gain * (x + previous_input),
                        pole * previous[1] + (1. - gain) * (x - previous_input),
                        pole * previous[2] - pole * x + previous_input,
                    ];
                    let actual = if gain32 {
                        let y = f32.process_sample(x as f32);
                        [
                            f64::from(y.low_pass),
                            f64::from(y.high_pass),
                            f64::from(y.all_pass),
                        ]
                    } else {
                        let y = f.process_sample(x);
                        [y.low_pass, y.high_pass, y.all_pass]
                    };
                    for k in 0..3 {
                        close(actual[k], expected[k], if gain32 { 2e-5 } else { 2e-13 });
                    }
                    close(actual[0] + actual[1], x, if gain32 { 1e-7 } else { 2e-16 });
                    previous = expected;
                    previous_input = x;
                }
                if impulse {
                    // Explicit ideal infinite tail bound; no finite EOF assertion.
                    let residual = previous.iter().fold(0_f64, |m, x| m.max(x.abs()));
                    let ideal_bound = 3. * pole.abs().powi(4094);
                    // The independent f64 recurrence can stick at subnormal
                    // rounding. Bound accumulated absolute product rounding by
                    // min_subnormal/(1-|p|), separately from the ideal tail.
                    let rounding_floor = f64::from_bits(1) / (1. - pole.abs());
                    if pole == 0. {
                        assert_eq!(residual, 0.);
                    } else {
                        assert!(
                            residual <= ideal_bound + rounding_floor,
                            "ratio{ratio} precision32{gain32} residual{residual:e} ideal{ideal_bound:e} floor{rounding_floor:e}"
                        );
                    }
                }
            }
        }
    }
    let d = D::new(48000., 12000.).unwrap();
    close(d.gain(), 0.5, f64::EPSILON);
    // Native tan/prewarp may round away from the ideal half gain; retain it.
    let q = d.quantize_f32().unwrap();
    assert_eq!(q.gain(), 0.5);
    let mut native32 = F32::new(q);
    let mut f = F::new(d);
    let mut previous = 0.;
    for x in [1., 0., -0.5, 0.25, 0.] {
        let y = f.process_sample(x);
        close(y.low_pass, 0.5 * (x + previous), 1e-15);
        close(y.high_pass, 0.5 * (x - previous), 1e-15);
        close(y.all_pass, previous, 1e-15);
        let y32 = native32.process_sample(x as f32);
        assert_eq!(y32.low_pass, 0.5 * (x as f32 + previous as f32));
        assert_eq!(y32.high_pass, 0.5 * (x as f32 - previous as f32));
        assert_eq!(y32.all_pass, previous as f32);
        previous = x;
    }
}
#[test]
fn signed_initial_capacitor_and_sample_varying_profiles_match_independent_feedback_solution() {
    let designs: Vec<_> = (0..3072)
        .map(|i| D::new(48000., [7., 1700., 12000., 23111.][i % 4]).unwrap())
        .collect();
    let rounded: Vec<_> = designs.iter().map(|d| d.quantize_f32().unwrap()).collect();
    for initial in [-0.71, 0., 0.39] {
        let mut f = F::new(designs[0]);
        let mut f32 = F32::new(rounded[0]);
        f.reset_to(initial).unwrap();
        f32.reset_to(initial as f32).unwrap();
        let mut reference64 = initial;
        let mut reference32 = initial as f32 as f64;
        for i in 0_usize..3072 {
            let x = (i as f64 * 0.053).sin() * 0.5 + if i.is_multiple_of(117) { 0.7 } else { 0. };
            let old = f.state();
            let old32 = f32.state();
            f.set_design(designs[i]);
            f32.set_design(rounded[i]);
            assert_eq!(f.state(), old);
            assert_eq!(f32.state(), old32);
            let y = f.process_sample(x);
            let y32 = f32.process_sample(x as f32);
            let expected = circuit(&mut reference64, x, designs[i].gain());
            let expected32 = circuit(
                &mut reference32,
                x as f32 as f64,
                f64::from(rounded[i].gain()),
            );
            for (r, k) in RESPONSES.into_iter().zip(0..3) {
                close(y.select(r), expected[k], 2e-13);
                close(f64::from(y32.select(r)), expected32[k], 3e-6);
            }
            close(f.state(), reference64, 3e-13);
            close(f64::from(f32.state()), reference32, 3e-6);
        }
        f.replace_design(designs[0]);
        f32.replace_design(rounded[0]);
        assert_eq!(f.state(), 0.);
        assert_eq!(f32.state(), 0.);
    }
}
#[test]
fn input_domain_rounding_and_precision_failures_preserve_existing_owners_without_heap() {
    let d = D::new(48000., 2000.).unwrap();
    let mut f = F::new(d);
    f.process_sample(0.4);
    let mut preserved = f.clone();
    let measured = allocation_counter::measure(|| {
        for rate in [
            0.,
            -1.,
            f64::from_bits(1),
            f64::MIN_POSITIVE,
            0.999,
            768001.,
            f64::NAN,
            f64::INFINITY,
        ] {
            assert!(D::new(rate, 0.1).is_err());
        }
        for rate in [1., 8000., 44100., 48000., 768000.] {
            for fc in [
                0.,
                -1.,
                f64::from_bits(1),
                f64::MIN_POSITIVE,
                rate * 0.5,
                rate,
                f64::MAX,
                f64::NAN,
                f64::INFINITY,
            ] {
                assert!(D::new(rate, fc).is_err());
            }
            let low = D::new(rate, rate * 1e-12).unwrap();
            assert!(low.quantize_f32().is_err());
            let high = D::new(rate, rate * 0.499999999999).unwrap();
            assert!(high.quantize_f32().is_err());
            for cutoff in [rate * 1e-6, rate * 0.01, rate * 0.25, rate * 0.499999] {
                let d = D::new(rate, cutoff).unwrap();
                assert!(d.quantize_f32().is_ok());
                let _ = F::new(d);
            }
        }
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
    for x in [0.2, -0.3, 0., 0.1] {
        assert_eq!(f.process_sample(x), preserved.process_sample(x));
    }
    assert_eq!(*f.design(), d);
    // Original Nyquist-adjacent endpoints: unavailable ratio or coefficient rejects,
    // otherwise the physical/normalized relation and actual pole remain strict.
    for bits in [
        1_f64.to_bits(),
        1.1_f64.to_bits(),
        44100_f64.to_bits(),
        768000_f64.to_bits(),
    ] {
        let rate = f64::from_bits(bits);
        let nyquist = rate * 0.5;
        let cutoff = f64::from_bits(nyquist.to_bits() - 1);
        match D::new(rate, cutoff) {
            Ok(d) => {
                assert!(cutoff < nyquist && cutoff / rate < 0.5);
                assert!(d.gain() < 1. && (1. - 2. * d.gain()).abs() < 1.);
            }
            Err(_) => assert!(
                cutoff / rate >= 0.5
                    || (std::f64::consts::PI * (cutoff / rate)).tan()
                        / (1. + (std::f64::consts::PI * (cutoff / rate)).tan())
                        == 1.
            ),
        }
    }
}
macro_rules! lifecycle {
    ($name:ident,$sample:ty,$filter:ty,$bank:ty,$design:expr) => {
        #[test]
        fn $name() {
            let d = $design;
            for response in RESPONSES {
                let input: Vec<$sample> = (0..513)
                    .map(|i| ((i as f64 * 0.071).sin() * 0.73) as $sample)
                    .collect();
                let mut scalar = <$filter>::new(d);
                scalar.reset_to(-0.2).unwrap();
                let expected: Vec<_> = input
                    .iter()
                    .map(|&x| scalar.process_sample(x).select(response))
                    .collect();
                let mut whole = <$filter>::new(d);
                whole.reset_to(-0.2).unwrap();
                let mut split = whole.clone();
                let mut inplace = whole.clone();
                let mut output = vec![0.; input.len()];
                whole.process_block(&input, &mut output, response).unwrap();
                assert_eq!(output, expected);
                let mut output = vec![0.; input.len()];
                let mut at = 0;
                for n in [0, 1, 17, 0, 3, 127, 2, 0, 363] {
                    split
                        .process_block(&input[at..at + n], &mut output[at..at + n], response)
                        .unwrap();
                    at += n;
                }
                assert_eq!(at, input.len());
                assert_eq!(output, expected);
                let mut copied = input.clone();
                inplace.process_in_place(&mut copied, response);
                assert_eq!(copied, expected);
                let before = whole.state();
                let mut sentinel = [99.; 3];
                assert!(
                    whole
                        .process_block(&[1.; 2], &mut sentinel, response)
                        .is_err()
                );
                assert_eq!(sentinel, [99.; 3]);
                assert_eq!(whole.state(), before);
                for value in [<$sample>::NAN, <$sample>::INFINITY, <$sample>::NEG_INFINITY] {
                    assert!(whole.reset_to(value).is_err());
                    assert_eq!(whole.state(), before);
                }
                whole.reset_to(-<$sample>::from_bits(1)).unwrap();
                whole.process_block(&[], &mut [], response).unwrap();
                assert_eq!(whole.state(), -<$sample>::from_bits(1));
                whole.snap_to_zero();
                assert_eq!(whole.state(), 0.);
                whole.reset_to(-<$sample>::MIN_POSITIVE).unwrap();
                whole.snap_to_zero();
                assert_eq!(whole.state(), -<$sample>::MIN_POSITIVE);
                whole.process_sample(<$sample>::NAN);
                assert!(whole.state().is_nan());
                whole.snap_to_zero();
                assert!(whole.state().is_nan());
                whole.replace_design(d);
                assert_eq!(whole.state(), 0.);
                whole.process_sample(<$sample>::INFINITY);
                assert!(!whole.state().is_finite());
                whole.reset();
                assert!(whole.process_sample(0.).select(response).is_finite());
                whole.reset_to(<$sample>::MAX).unwrap();
                whole.process_sample(-<$sample>::MAX);
                assert!(!whole.state().is_finite());
                whole.reset();
                assert_eq!(whole.state(), 0.);
                for channels in 1..=64 {
                    let mut bank = <$bank>::new(d, channels, 137).unwrap();
                    bank.reset_to(-0.2).unwrap();
                    let mut reference = bank.clone();
                    let mut inplace = bank.clone();
                    let mut split = bank.clone();
                    assert_eq!(bank.channels(), channels);
                    assert_eq!(bank.maximum_block_frames(), 137);
                    assert_eq!(*bank.design(), d);
                    let data: Vec<Vec<$sample>> = (0..channels)
                        .map(|ch| {
                            (0..140)
                                .map(|i| {
                                    ((i as f64 * 0.017 + ch as f64 * 0.13).sin() * (ch + 1) as f64
                                        / 64.) as $sample
                                })
                                .collect()
                        })
                        .collect();
                    let expected: Vec<Vec<_>> = data
                        .iter()
                        .enumerate()
                        .map(|(ch, x)| {
                            x[..137]
                                .iter()
                                .map(|&x| reference.process_sample(ch, x).unwrap().select(response))
                                .collect()
                        })
                        .collect();
                    let inputs: Vec<&[$sample]> = data.iter().map(Vec::as_slice).collect();
                    let mut output = vec![vec![99.; 140]; channels];
                    let mut refs: Vec<&mut [$sample]> =
                        output.iter_mut().map(Vec::as_mut_slice).collect();
                    let measured = allocation_counter::measure(|| {
                        bank.process_planar(&inputs, &mut refs, 137, response)
                            .unwrap()
                    });
                    assert_eq!(measured.count_total, 0);
                    assert_eq!(measured.count_current, 0);
                    for (ch, x) in output.iter().enumerate() {
                        assert_eq!(&x[..137], &expected[ch]);
                        assert_eq!(&x[137..], &[99.; 3]);
                        assert_eq!(bank.state(ch).unwrap(), reference.state(ch).unwrap());
                    }
                    let mut data_inplace = data.clone();
                    let mut refs: Vec<&mut [$sample]> =
                        data_inplace.iter_mut().map(Vec::as_mut_slice).collect();
                    inplace
                        .process_planar_in_place(&mut refs, 137, response)
                        .unwrap();
                    for (ch, x) in data_inplace.iter().enumerate() {
                        assert_eq!(&x[..137], &expected[ch]);
                        assert_eq!(&x[137..], &data[ch][137..]);
                    }
                    let mut output = vec![vec![99.; 140]; channels];
                    let mut at = 0;
                    for n in [0, 1, 13, 0, 64, 59] {
                        let input: Vec<&[$sample]> = data.iter().map(|x| &x[at..]).collect();
                        let mut refs: Vec<&mut [$sample]> =
                            output.iter_mut().map(|x| &mut x[at..]).collect();
                        split
                            .process_planar(&input, &mut refs, n, response)
                            .unwrap();
                        at += n;
                    }
                    assert_eq!(at, 137);
                    for (ch, x) in output.iter().enumerate() {
                        assert_eq!(&x[..137], &expected[ch]);
                        assert_eq!(bank.state(ch).unwrap(), split.state(ch).unwrap());
                    }
                    let previous = bank.clone();
                    let last = channels - 1;
                    let mut bad = vec![vec![99.; 140]; channels];
                    let mut refs: Vec<&mut [$sample]> =
                        bad.iter_mut().map(Vec::as_mut_slice).collect();
                    refs[last] = &mut [];
                    assert!(
                        bank.process_planar(&inputs, &mut refs, 1, response)
                            .is_err()
                    );
                    assert!(bad.iter().all(|x| x.iter().all(|&x| x == 99.)));
                    let mut refs: Vec<&mut [$sample]> =
                        bad.iter_mut().map(Vec::as_mut_slice).collect();
                    assert!(
                        bank.process_planar(&inputs, &mut refs, 138, response)
                            .is_err()
                    );
                    assert!(
                        bank.process_planar(&inputs[..last], &mut refs, 0, response)
                            .is_err()
                    );
                    let mut short = inputs.clone();
                    short[last] = &[];
                    assert!(bank.process_planar(&short, &mut refs, 1, response).is_err());
                    let mut refs: Vec<&mut [$sample]> =
                        bad.iter_mut().map(Vec::as_mut_slice).collect();
                    refs[last] = &mut [];
                    assert!(
                        bank.process_planar_in_place(&mut refs, 1, response)
                            .is_err()
                    );
                    assert!(bad.iter().all(|x| x.iter().all(|&x| x == 99.)));
                    assert!(bank.process_sample(channels, 0.).is_err());
                    assert!(bank.state(channels).is_err());
                    assert!(bank.reset_to(<$sample>::NAN).is_err());
                    for ch in 0..channels {
                        assert_eq!(bank.state(ch).unwrap(), previous.state(ch).unwrap());
                    }
                    bank.set_design(d);
                    assert_eq!(bank.state(last).unwrap(), previous.state(last).unwrap());
                    bank.replace_design(d);
                    assert_eq!(bank.state(last).unwrap(), 0.);
                    bank.reset_to(-<$sample>::from_bits(1)).unwrap();
                    let mut refs: Vec<&mut [$sample]> =
                        bad.iter_mut().map(Vec::as_mut_slice).collect();
                    bank.process_planar(&inputs, &mut refs, 0, response)
                        .unwrap();
                    assert_eq!(bank.state(last).unwrap(), -<$sample>::from_bits(1));
                    bank.snap_to_zero();
                    assert_eq!(bank.state(last).unwrap(), 0.);
                    bank.process_sample(last, <$sample>::NAN).unwrap();
                    bank.snap_to_zero();
                    assert!(bank.state(last).unwrap().is_nan());
                    if channels > 1 {
                        assert!(bank.process_sample(0, 0.1).unwrap().low_pass.is_finite());
                        assert!(bank.state(last).unwrap().is_nan());
                    }
                    bank.reset();
                    assert_eq!(bank.state(last).unwrap(), 0.);
                }
                let measured = allocation_counter::measure(|| {
                    let mut f = <$filter>::new(d);
                    let mut b = <$bank>::new(d, 64, 65536).unwrap();
                    for _ in 0..10000 {
                        f.process_sample(0.3);
                        b.process_sample(63, 0.1).unwrap();
                    }
                    f.reset();
                    f.reset_to(-0.3).unwrap();
                    f.snap_to_zero();
                    f.set_design(d);
                    f.replace_design(d);
                    b.reset();
                    b.reset_to(-0.7).unwrap();
                    b.snap_to_zero();
                    b.set_design(d);
                    b.replace_design(d);
                    for (ch, max) in [
                        (0, 1),
                        (65, 1),
                        (1, 0),
                        (1, 65537),
                        (usize::MAX, usize::MAX),
                    ] {
                        assert!(<$bank>::new(d, ch, max).is_err());
                    }
                });
                assert_eq!(measured.count_total, 0);
                assert_eq!(measured.count_current, 0);
            }
        }
    };
}
lifecycle!(
    native64_all_responses_and_independent_channel_lifecycles,
    f64,
    F,
    B,
    D::new(48000., 1700.).unwrap()
);
lifecycle!(
    native32_all_responses_and_independent_channel_lifecycles,
    f32,
    F32,
    B32,
    D::new(48000., 1700.).unwrap().quantize_f32().unwrap()
);
macro_rules! graph_fir {
    ($name:ident,$sample:ty,$design:ty,$filter:ty,$convert:expr,$tolerance:expr) => {
        #[test]
        fn $name() {
            use ravet_foundation::{
                AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
                Processor, Transport,
            };
            use ravet_graph::{
                Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source,
            };
            use ravet_signal::FirConvolver;
            use std::sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            };
            struct Stereo {
                profiles: [Vec<$design>; 2],
                filters: [$filter; 2],
                fir: [FirConvolver; 2],
                responses: [R; 2],
                position: usize,
                config: Option<ProcessConfig>,
                frames: Arc<AtomicUsize>,
                deactivations: Arc<AtomicUsize>,
            }
            impl Processor<$sample> for Stereo {
                fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
                    self.config = None;
                    if c.layout() != AudioLayout::STEREO {
                        return Err(PrepareError::UnsupportedLayout);
                    }
                    if c.sample_rate() != 48000. {
                        return Err(PrepareError::InvalidSampleRate);
                    }
                    self.config = Some(c);
                    self.reset();
                    Ok(())
                }
                fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                    if let Err(e) = self.config.unwrap().validate_block(b) {
                        b.clear_outputs();
                        return ProcessStatus::Fault(e);
                    }
                    for ch in 0..2 {
                        for i in 0..b.audio.frames() {
                            let index = (self.position + i) % self.profiles[ch].len();
                            self.filters[ch].set_design(self.profiles[ch][index]);
                            let source = b.audio.input(ch).unwrap()[i];
                            let selected = self.filters[ch]
                                .process_sample(source)
                                .select(self.responses[ch]);
                            // Caller-explicit precision bridge to the existing f64 FIR;
                            // the Foundation buffer and TPT core stay native $sample.
                            b.audio.output_mut(ch).unwrap()[i] =
                                self.fir[ch].process_sample(selected as f64) as $sample;
                        }
                    }
                    self.position += b.audio.frames();
                    self.frames.fetch_add(b.audio.frames(), Ordering::Relaxed);
                    ProcessStatus::Continue
                }
                fn reset(&mut self) {
                    for ch in 0..2 {
                        self.filters[ch].replace_design(self.profiles[ch][0]);
                        self.fir[ch].reset();
                    }
                    self.position = 0;
                    self.frames.store(0, Ordering::Relaxed);
                }
                fn deactivate(&mut self) {
                    if self.config.take().is_some() {
                        self.deactivations.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            for response in 0..3 {
                let responses = [RESPONSES[response], RESPONSES[(response + 1) % 3]];
                let profiles: [Vec<$design>; 2] = std::array::from_fn(|ch| {
                    (0..1501)
                        .map(|i| {
                            ($convert)(
                                D::new(48000., [370., 1700., 12000., 22900.][(i + ch) % 4])
                                    .unwrap(),
                            )
                        })
                        .collect()
                });
                let input: [Vec<$sample>; 2] = std::array::from_fn(|ch| {
                    (0..1501)
                        .map(|i| {
                            ((i as f64 * (0.17 + ch as f64 * 0.037)).sin()
                                * (if ch == 0 { 0.8 } else { 0.07 })
                                + ((i * 13 % 23) as f64 - 11.) / 73.) as $sample
                        })
                        .collect()
                });
                let impulses: [&[f64]; 2] = [&[0.5, -0.25, 0.125, 0.2], &[-0.1, 0.6, 0.2]];
                let expected: [Vec<f64>; 2] = std::array::from_fn(|ch| {
                    let mut state = 0.;
                    let filtered: Vec<_> = input[ch]
                        .iter()
                        .enumerate()
                        .map(|(i, &x)| {
                            let y = circuit(&mut state, x as f64, profiles[ch][i].gain() as f64);
                            y[match responses[ch] {
                                R::LowPass => 0,
                                R::HighPass => 1,
                                R::AllPass => 2,
                            }]
                        })
                        .collect();
                    (0..filtered.len())
                        .map(|i| {
                            impulses[ch]
                                .iter()
                                .enumerate()
                                .filter(|(k, _)| *k <= i)
                                .map(|(k, &h)| h * filtered[i - k])
                                .sum()
                        })
                        .collect()
                });
                let filters = [
                    <$filter>::new(profiles[0][0]),
                    <$filter>::new(profiles[1][0]),
                ];
                let fir = [
                    FirConvolver::new(impulses[0]).unwrap(),
                    FirConvolver::new(impulses[1]).unwrap(),
                ];
                let frames = Arc::new(AtomicUsize::new(0));
                let deactivations = Arc::new(AtomicUsize::new(0));
                let node = GraphNode {
                    id: NodeId(19),
                    layout: AudioLayout::STEREO,
                    processor: Box::new(Stereo {
                        profiles,
                        filters,
                        fir,
                        responses,
                        position: 0,
                        config: None,
                        frames: frames.clone(),
                        deactivations: deactivations.clone(),
                    }),
                };
                let routes: Vec<_> = (0..2)
                    .flat_map(|channel| {
                        [
                            Connection {
                                source: Source::Input(channel),
                                destination: Destination::Node {
                                    id: NodeId(19),
                                    channel,
                                },
                                gain: 1.,
                            },
                            Connection {
                                source: Source::Node {
                                    id: NodeId(19),
                                    channel,
                                },
                                destination: Destination::Output(channel),
                                gain: 1.,
                            },
                        ]
                    })
                    .collect();
                let mut graph = Graph::new(AudioLayout::STEREO, vec![node], &routes).unwrap();
                for cycle in 0..2 {
                    graph
                        .prepare(ProcessConfig::new(48000., 67, 0, AudioLayout::STEREO).unwrap())
                        .unwrap();
                    for repeat in 0..2 {
                        let mut bad = [[99.; 69]; 2];
                        let [l, r] = &mut bad;
                        let mut output = [l.as_mut_slice(), r.as_mut_slice()];
                        let inputs = [&input[0][..68], &input[1][..68]];
                        let audio = AudioBuffer::separate(&inputs, &mut output, 68).unwrap();
                        assert!(matches!(
                            graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                            GraphStatus::Fault { node: None, .. }
                        ));
                        assert_eq!(frames.load(Ordering::Relaxed), 0);
                        assert!(
                            bad.iter()
                                .all(|c| c[..68].iter().all(|&x| x == 0.) && c[68] == 99.)
                        );
                        let mut start = 0;
                        let mut turn = 0;
                        while start < input[0].len() {
                            let n = [1, 67, 2, 0, 31, 5][turn % 6].min(input[0].len() - start);
                            let mut output = [[99.; 69]; 2];
                            let measured = allocation_counter::measure(|| {
                                let inputs =
                                    [&input[0][start..start + n], &input[1][start..start + n]];
                                let [l, r] = &mut output;
                                let mut outputs = [l.as_mut_slice(), r.as_mut_slice()];
                                let audio =
                                    AudioBuffer::separate(&inputs, &mut outputs, n).unwrap();
                                assert_eq!(
                                    graph.process(&mut ProcessBlock::new(
                                        audio,
                                        &[],
                                        Transport::default()
                                    )),
                                    GraphStatus::Continue
                                );
                            });
                            assert_eq!(measured.count_total, 0);
                            assert_eq!(measured.count_current, 0);
                            for ch in 0..2 {
                                for i in 0..n {
                                    close(
                                        output[ch][i] as f64,
                                        expected[ch][start + i],
                                        $tolerance,
                                    );
                                }
                                assert!(output[ch][n..].iter().all(|&x| x == 99.));
                            }
                            start += n;
                            turn += 1;
                        }
                        assert_eq!(frames.load(Ordering::Relaxed), input[0].len());
                        if repeat == 0 {
                            graph.reset();
                        }
                    }
                    graph.deactivate();
                    assert_eq!(deactivations.load(Ordering::Relaxed), cycle + 1);
                }
            }
        }
    };
}
graph_fir!(
    actual_native64_modulated_stereo_graph_fir_composition,
    f64,
    D,
    F,
    |d: D| d,
    3e-13
);
graph_fir!(
    actual_native32_modulated_stereo_graph_with_explicit_f64_fir_bridge,
    f32,
    ravet_signal::FirstOrderTptDesign32,
    F32,
    |d: D| d.quantize_f32().unwrap(),
    3e-6
);

#[test]
fn near_endpoint_actual_coefficient_spectrum_avoids_expanded_pole_cancellation() {
    for ratio in [1e-12, 1e-10, 1e-8, 1e-6, 0.499999999999, 0.49999999] {
        let d = D::new(48000., 48000. * ratio).unwrap();
        let actual = (d.gain() / (1. - d.gain())).atan() / std::f64::consts::PI;
        for f in [ratio, actual, ratio * 0.3, 0.5 - (0.5 - ratio) * 0.5] {
            for response in RESPONSES {
                close(
                    d.magnitude_at(f, response).unwrap(),
                    analog(d.gain(), f, response),
                    3e-14,
                );
            }
        }
    }
}
