//! Source-rate impulse-response preparation: analytic resampling references,
//! exact trim/normalize/map arithmetic, length formulas and whole-call refusals.
use ravet_signal::{
    ConvolutionPartitioning, ImpulseResponseError, ImpulseResponseRequest, ResponseChannelMap,
    ResponseDelay, ResponseNormalization, ResponseRateConversion, ResponseRateGain, ResponseTrim,
    TrimThreshold, prepare_impulse_response,
};
use std::f64::consts::PI;

fn request(
    target_rate: u32,
    conversion: ResponseRateConversion,
) -> ImpulseResponseRequest<'static> {
    ImpulseResponseRequest {
        target_rate,
        conversion,
        channels: ResponseChannelMap::Identity,
        trim: ResponseTrim::default(),
        normalization: ResponseNormalization::None,
        maximum_taps: 262_144,
        maximum_working_bytes: 64 << 20,
    }
}

fn sinc(taps: usize, gain: ResponseRateGain, delay: ResponseDelay) -> ResponseRateConversion {
    ResponseRateConversion::Sinc { taps, gain, delay }
}

// Analytic continuous response: Gaussian-windowed cosine burst.
const T0: f64 = 0.003;
const SIGMA: f64 = 0.0005;
const F0: f64 = 2000.;
fn analytic(t: f64) -> f64 {
    let u = (t - T0) / SIGMA;
    (-u * u).exp() * (2. * PI * F0 * (t - T0)).cos()
}
// Continuous Fourier transform magnitude of `analytic` at f (Hz).
fn analytic_spectrum(f: f64) -> f64 {
    let g = |d: f64| (-(PI * SIGMA * d).powi(2)).exp();
    SIGMA * PI.sqrt() / 2. * (g(f - F0) + g(f + F0))
}
fn sampled(rate: u32, seconds: f64) -> Vec<f64> {
    let n = (seconds * f64::from(rate)).ceil() as usize;
    (0..n)
        .map(|i| analytic(i as f64 / f64::from(rate)))
        .collect()
}
fn dtft_magnitude(h: &[f64], f: f64, rate: f64) -> f64 {
    let w = 2. * PI * f / rate;
    let (mut re, mut im) = (0., 0.);
    for (n, x) in h.iter().enumerate() {
        re += x * (w * n as f64).cos();
        im -= x * (w * n as f64).sin();
    }
    re.hypot(im)
}

#[test]
fn resampled_responses_match_the_analytic_continuous_response() {
    let mut observations = Vec::new();
    for (source, target) in [
        (44_100, 48_000),
        (48_000, 44_100),
        (96_000, 48_000),
        (48_000, 192_000),
        (192_000, 48_000),
        (48_000, 48_000),
    ] {
        for taps in [64, 256] {
            let h = sampled(source, 0.008);
            let prepared = prepare_impulse_response(
                source,
                &[&h],
                &request(
                    target,
                    sinc(
                        taps,
                        ResponseRateGain::PreserveResponseGain,
                        ResponseDelay::RemoveWholeTargetFrames,
                    ),
                ),
            )
            .unwrap();
            let report = *prepared.report();
            let ratio = f64::from(source) / f64::from(target);
            assert_eq!(report.rate_gain, ratio);
            let r = report.retained_delay_target_frames;
            let exact = ((taps as u64 - 1) * u64::from(target) % (2 * u64::from(source))) as f64
                / (2 * u64::from(source)) as f64;
            assert_eq!(r, exact);
            assert!((0. ..1.).contains(&r));
            let z = prepared.channel(0).unwrap();
            // Time-domain: z[j] ~= ratio * h_c((j - r) / target).
            let mut error = 0f64;
            for (j, value) in z.iter().enumerate() {
                let expected = ratio * analytic((j as f64 - r) / f64::from(target));
                error = error.max((value - expected).abs());
            }
            let relative = error / ratio;
            // Frequency response preserved: |Z(f)| ~= source * |H_c(f)|, error
            // relative to the response magnitude at the burst centre F0.
            let mut spectral = 0f64;
            let reference = f64::from(source) * analytic_spectrum(F0);
            for f in [0., 500., 1000., 1500., 2000., 2500., 3000., 5000.] {
                let measured = dtft_magnitude(z, f, f64::from(target));
                let expected = f64::from(source) * analytic_spectrum(f);
                spectral = spectral.max((measured - expected).abs() / reference);
            }
            observations.push((source, target, taps, relative, spectral));
            let bound = if taps == 64 { 5e-5 } else { 5e-6 };
            assert!(
                relative < bound,
                "{source}->{target} taps {taps}: {relative:e}"
            );
            assert!(
                spectral < bound,
                "{source}->{target} taps {taps}: {spectral:e}"
            );
        }
    }
    for (s, t, taps, rel, spec) in observations {
        println!("W259 analytic {s}->{t} taps {taps}: time {rel:e} spectral {spec:e}");
    }
}

#[test]
fn rate_gain_and_delay_policies_relate_bitwise_and_lengths_follow_formulas() {
    for (source, target, frames, taps) in [
        (44_100u32, 48_000u32, 353usize, 64usize),
        (48_000, 44_100, 1, 16),
        (96_000, 48_000, 1000, 256),
        (32_000, 48_000, 17, 16),
        (8_000, 128_000, 5, 32),
        (128_000, 8_000, 300, 32),
    ] {
        let h: Vec<f64> = (0..frames)
            .map(|i| ((i * 7919) % 101) as f64 / 101. - 0.5)
            .collect();
        let run = |gain, delay| {
            prepare_impulse_response(source, &[&h], &request(target, sinc(taps, gain, delay)))
                .unwrap()
        };
        let retain = run(
            ResponseRateGain::PreserveSampleAmplitude,
            ResponseDelay::Retain,
        );
        let removed = run(
            ResponseRateGain::PreserveSampleAmplitude,
            ResponseDelay::RemoveWholeTargetFrames,
        );
        let gained = run(
            ResponseRateGain::PreserveResponseGain,
            ResponseDelay::RemoveWholeTargetFrames,
        );
        let (n, t, i, o) = (
            frames as u128,
            taps as u128,
            u128::from(source),
            u128::from(target),
        );
        let staged = ((n + t - 2) * o / i + 1) as usize;
        let k = ((t - 1) * o / (2 * i)) as usize;
        assert_eq!(retain.report().staged_frames, staged);
        assert_eq!(retain.frames(), staged);
        assert_eq!(removed.report().removed_delay_frames, k);
        assert_eq!(removed.frames(), staged - k);
        assert_eq!(retain.report().rate_gain, 1.);
        assert_eq!(
            retain.report().retained_delay_target_frames,
            ((t - 1) * o) as f64 / (2 * i) as f64
        );
        assert_eq!(
            removed.channel(0).unwrap(),
            &retain.channel(0).unwrap()[k..]
        );
        let ratio = f64::from(source) / f64::from(target);
        for (a, b) in gained
            .channel(0)
            .unwrap()
            .iter()
            .zip(removed.channel(0).unwrap())
        {
            assert_eq!(a.to_bits(), (b * ratio).to_bits());
        }
        let plan = request(
            target,
            sinc(
                taps,
                ResponseRateGain::PreserveSampleAmplitude,
                ResponseDelay::Retain,
            ),
        )
        .plan(source, 1, frames)
        .unwrap();
        assert_eq!(
            plan.declared_working_bytes,
            retain.report().declared_working_bytes
        );
        assert_eq!(
            plan.declared_working_bytes,
            staged * 8 + staged * 8 + 1026 * taps * 8
        );
    }
}

#[test]
fn channel_mapping_is_exact_and_per_channel_conversion_is_reproducible() {
    let a = [0.25, -0.5, 0.125, 0.0];
    let b = [1.0, 0.0, -1.0, 0.5];
    let c = [0.0, 0.75, 0.0, -0.25];
    let map = [2usize, 0, 0, 1];
    let mut copy = request(48_000, ResponseRateConversion::RequireEqualRates);
    copy.channels = ResponseChannelMap::Select(&map);
    let prepared = prepare_impulse_response(48_000, &[&a, &b, &c], &copy).unwrap();
    assert_eq!(prepared.channels(), 4);
    assert_eq!(prepared.channel(0).unwrap(), &c);
    assert_eq!(prepared.channel(1).unwrap(), &a);
    assert_eq!(prepared.channel(2).unwrap(), &a);
    assert_eq!(prepared.channel(3).unwrap(), &b);
    assert_eq!(prepared.channel(4), None);
    assert_eq!(prepared.report().source_channels, 3);
    let mut converted = request(
        44_100,
        sinc(
            32,
            ResponseRateGain::PreserveResponseGain,
            ResponseDelay::Retain,
        ),
    );
    converted.channels = ResponseChannelMap::Select(&map);
    let multi = prepare_impulse_response(48_000, &[&a, &b, &c], &converted).unwrap();
    converted.channels = ResponseChannelMap::Identity;
    let single = prepare_impulse_response(48_000, &[&c], &converted).unwrap();
    assert_eq!(multi.channel(0).unwrap(), single.channel(0).unwrap());
    assert_eq!(multi.channel(1).unwrap(), multi.channel(2).unwrap());
}

#[test]
fn trimming_records_linked_offsets_exactly() {
    let left = [0.0, 0.0, 0.0, 0.001, 0.5, -0.25, 0.002, 0.0005, 0.0, 0.0];
    let right = [0.0, 0.0, -0.01, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0008, 0.0];
    let mut r = request(48_000, ResponseRateConversion::RequireEqualRates);
    r.trim = ResponseTrim {
        leading: Some(TrimThreshold::Absolute(0.001)),
        trailing: Some(TrimThreshold::Absolute(0.001)),
    };
    let p = prepare_impulse_response(48_000, &[&left, &right], &r).unwrap();
    // First frame with any |x| > 0.001 is frame 2 (right -0.01); last is frame 6.
    assert_eq!(p.report().leading_trimmed_frames, 2);
    assert_eq!(p.report().trailing_trimmed_frames, 3);
    assert_eq!(p.report().converted_frames, 10);
    assert_eq!(p.frames(), 5);
    assert_eq!(p.channel(0).unwrap(), &left[2..7]);
    assert_eq!(p.channel(1).unwrap(), &right[2..7]);
    assert_eq!(p.report().leading_threshold, Some(0.001));
    // Relative threshold resolves against the linked peak (1.0).
    r.trim = ResponseTrim {
        leading: Some(TrimThreshold::RelativeToPeak(0.3)),
        trailing: None,
    };
    let p = prepare_impulse_response(48_000, &[&left, &right], &r).unwrap();
    assert_eq!(p.report().leading_threshold, Some(0.3));
    assert_eq!(p.report().leading_trimmed_frames, 4);
    assert_eq!(p.report().trailing_trimmed_frames, 0);
    assert_eq!(p.channel(0).unwrap(), &left[4..]);
    assert_eq!(p.channel(1).unwrap(), &right[4..]);
    // Trimming can admit a converted response longer than maximum_taps.
    let mut long = vec![0.0; 300];
    long[100] = 1.0;
    long[149] = -0.5;
    r.trim = ResponseTrim {
        leading: Some(TrimThreshold::Absolute(0.0)),
        trailing: Some(TrimThreshold::Absolute(0.0)),
    };
    r.maximum_taps = 50;
    let p = prepare_impulse_response(48_000, &[&long], &r).unwrap();
    assert_eq!((p.report().leading_trimmed_frames, p.frames()), (100, 50));
    assert_eq!(p.channel(0).unwrap(), &long[100..150]);
    r.maximum_taps = 49;
    assert_eq!(
        prepare_impulse_response(48_000, &[&long], &r),
        Err(ImpulseResponseError::TooLong)
    );
}

#[test]
fn normalization_gains_are_exact_and_reported() {
    let left = [0.1, -0.7, 0.3, 0.05];
    let right = [0.2, 0.4, -0.35, 0.0];
    let mut r = request(44_100, ResponseRateConversion::RequireEqualRates);
    r.normalization = ResponseNormalization::Peak { target: 0.9 };
    let p = prepare_impulse_response(44_100, &[&left, &right], &r).unwrap();
    let gain = 0.9 / 0.7;
    assert_eq!(p.report().normalization_gain, gain);
    assert_eq!(p.report().peak_before_normalization, 0.7);
    for (channel, source) in [(0, &left), (1, &right)] {
        for (y, x) in p.channel(channel).unwrap().iter().zip(source.iter()) {
            assert_eq!(y.to_bits(), (x * gain).to_bits());
        }
    }
    assert_eq!(p.report().peak, 0.7 * gain);
    assert!((p.report().peak - 0.9).abs() <= f64::EPSILON);
    r.normalization = ResponseNormalization::Energy { target: 2.0 };
    let p = prepare_impulse_response(44_100, &[&left, &right], &r).unwrap();
    let e_left = 0.1f64 * 0.1 + 0.7 * 0.7 + 0.3 * 0.3 + 0.05 * 0.05;
    let gain = (2.0 / e_left).sqrt();
    assert_eq!(p.report().normalization_gain, gain);
    for (y, x) in p.channel(1).unwrap().iter().zip(right.iter()) {
        assert_eq!(y.to_bits(), (x * gain).to_bits());
    }
    assert!((p.report().maximum_channel_energy - 2.0).abs() < 4. * f64::EPSILON);
    r.normalization = ResponseNormalization::None;
    let p = prepare_impulse_response(44_100, &[&left, &right], &r).unwrap();
    assert_eq!(p.report().normalization_gain, 1.);
    assert_eq!(p.channel(0).unwrap(), &left);
}

#[test]
fn prepared_responses_build_matching_engines_and_banks() {
    let h: Vec<f64> = (0..300)
        .map(|i| (-(i as f64) / 40.).exp() * ((i % 7) as f64 - 3.))
        .collect();
    let p = prepare_impulse_response(
        48_000,
        &[&h, &h],
        &request(48_000, ResponseRateConversion::RequireEqualRates),
    )
    .unwrap();
    let schedule = ConvolutionPartitioning::NonUniform { head: 16, tail: 64 };
    let bank = p.bank64(schedule, 64, 1 << 20).unwrap();
    assert_eq!(bank.engine(1).unwrap().info().taps, 300);
    let mut engine = p.engine64(0, schedule, 64, 1 << 20).unwrap();
    let mut e32 = p.engine32(1, schedule, 64, 1 << 20).unwrap();
    let mut impulse = vec![0.0; 320];
    impulse[0] = 1.0;
    let mut out = vec![0.0; 320];
    for (i, o) in impulse.chunks(64).zip(out.chunks_mut(64)) {
        engine.process(i, o, i.len()).unwrap();
    }
    for (a, b) in out.iter().zip(&h) {
        assert!((a - b).abs() < 1e-12);
    }
    let y = e32.process_sample(1.0);
    assert_eq!(y, h[0] as f32);
    assert!(p.bank32(schedule, 64, 1 << 20).is_ok());
    assert_eq!(
        p.engine64(2, schedule, 64, 1 << 20).err(),
        Some(ravet_signal::ConvolutionPreparationError::InvalidChannels)
    );
}

#[test]
fn refusals_are_whole_call_and_precede_allocation() {
    use ImpulseResponseError as E;
    let h = [0.5, 0.25];
    let copy = request(48_000, ResponseRateConversion::RequireEqualRates);
    let up = sinc(
        16,
        ResponseRateGain::PreserveResponseGain,
        ResponseDelay::Retain,
    );
    let cases: Vec<(ImpulseResponseRequest<'static>, u32, E)> = vec![
        (copy, 0, E::InvalidRate),
        (request(768_001, up), 48_000, E::InvalidRate),
        (copy, 44_100, E::InvalidConversion),
        (
            request(
                48_000,
                sinc(
                    15,
                    ResponseRateGain::PreserveResponseGain,
                    ResponseDelay::Retain,
                ),
            ),
            48_000,
            E::InvalidConversion,
        ),
        (
            request(
                48_000,
                sinc(
                    258,
                    ResponseRateGain::PreserveResponseGain,
                    ResponseDelay::Retain,
                ),
            ),
            48_000,
            E::InvalidConversion,
        ),
        (request(48_000, up), 2_999, E::InvalidConversion),
        (request(47_999, up), 768_000, E::InvalidConversion),
        (
            ImpulseResponseRequest {
                channels: ResponseChannelMap::Select(&[]),
                ..copy
            },
            48_000,
            E::InvalidChannelMap,
        ),
        (
            ImpulseResponseRequest {
                channels: ResponseChannelMap::Select(&[1]),
                ..copy
            },
            48_000,
            E::InvalidChannelMap,
        ),
        (
            ImpulseResponseRequest {
                trim: ResponseTrim {
                    leading: Some(TrimThreshold::Absolute(-1.)),
                    trailing: None,
                },
                ..copy
            },
            48_000,
            E::InvalidTrim,
        ),
        (
            ImpulseResponseRequest {
                trim: ResponseTrim {
                    leading: None,
                    trailing: Some(TrimThreshold::RelativeToPeak(1.)),
                },
                ..copy
            },
            48_000,
            E::InvalidTrim,
        ),
        (
            ImpulseResponseRequest {
                trim: ResponseTrim {
                    leading: Some(TrimThreshold::Absolute(f64::NAN)),
                    trailing: None,
                },
                ..copy
            },
            48_000,
            E::InvalidTrim,
        ),
        (
            ImpulseResponseRequest {
                normalization: ResponseNormalization::Peak { target: 0. },
                ..copy
            },
            48_000,
            E::InvalidNormalization,
        ),
        (
            ImpulseResponseRequest {
                normalization: ResponseNormalization::Energy {
                    target: f64::INFINITY,
                },
                ..copy
            },
            48_000,
            E::InvalidNormalization,
        ),
        (
            ImpulseResponseRequest {
                maximum_taps: 0,
                ..copy
            },
            48_000,
            E::Capacity,
        ),
        (
            ImpulseResponseRequest {
                maximum_taps: 262_145,
                ..copy
            },
            48_000,
            E::Capacity,
        ),
        (
            ImpulseResponseRequest {
                maximum_working_bytes: 0,
                ..copy
            },
            48_000,
            E::Capacity,
        ),
        (
            ImpulseResponseRequest {
                maximum_working_bytes: (256 << 20) + 1,
                ..copy
            },
            48_000,
            E::Capacity,
        ),
        (
            ImpulseResponseRequest {
                maximum_working_bytes: 15,
                ..copy
            },
            48_000,
            E::Capacity,
        ),
        (
            ImpulseResponseRequest {
                maximum_taps: 1,
                ..copy
            },
            48_000,
            E::TooLong,
        ),
        (
            ImpulseResponseRequest {
                normalization: ResponseNormalization::Peak { target: f64::MAX },
                ..copy
            },
            48_000,
            E::NonFinite,
        ),
    ];
    for (r, rate, expected) in cases {
        let error = allocation_counter::measure(|| {
            assert_eq!(prepare_impulse_response(rate, &[&h], &r), Err(expected));
        });
        // Policy/bound refusals precede every allocation; NonFinite is data dependent.
        if expected != E::NonFinite {
            assert_eq!(error.count_total, 0, "{expected:?}");
        }
    }
    // Source shape refusals.
    assert_eq!(
        prepare_impulse_response(48_000, &[], &copy),
        Err(E::InvalidSource)
    );
    assert_eq!(
        prepare_impulse_response(48_000, &[&[]], &copy),
        Err(E::InvalidSource)
    );
    assert_eq!(
        prepare_impulse_response(48_000, &[&h, &[0.5]], &copy),
        Err(E::InvalidSource)
    );
    assert_eq!(
        prepare_impulse_response(48_000, &[&[0.5, f64::NAN]], &copy),
        Err(E::InvalidSource)
    );
    let many: Vec<&[f64]> = vec![&h; 65];
    assert_eq!(
        prepare_impulse_response(48_000, &many, &copy),
        Err(E::InvalidSource)
    );
    let huge = vec![0.0; ravet_signal::IMPULSE_RESPONSE_MAX_SOURCE_FRAMES + 1];
    assert_eq!(copy.plan(48_000, 1, huge.len()), Err(E::InvalidSource));
    // Data-dependent refusals.
    let zeros = [0.0; 8];
    let trim = ImpulseResponseRequest {
        trim: ResponseTrim {
            leading: Some(TrimThreshold::Absolute(0.)),
            trailing: None,
        },
        ..copy
    };
    assert_eq!(
        prepare_impulse_response(48_000, &[&zeros], &trim),
        Err(E::BelowThreshold)
    );
    let normalize = ImpulseResponseRequest {
        normalization: ResponseNormalization::Energy { target: 1. },
        ..copy
    };
    assert_eq!(
        prepare_impulse_response(48_000, &[&zeros], &normalize),
        Err(E::Silent)
    );
    // The borrowed source is untouched by every refusal.
    assert_eq!(h, [0.5, 0.25]);
}

// R3: energy normalization must survive extreme finite inputs.
fn energy_request(target: f64) -> ImpulseResponseRequest<'static> {
    ImpulseResponseRequest {
        normalization: ResponseNormalization::Energy { target },
        ..request(48_000, ResponseRateConversion::RequireEqualRates)
    }
}

#[test]
fn energy_normalization_handles_huge_finite_samples() {
    // Direct squares overflow to infinity (1e400); the old path returned gain 0
    // and an all-zero response without refusing.
    let h = [1e200, -5e199, 2e199, 0.0];
    let p = prepare_impulse_response(48_000, &[&h], &energy_request(1.0)).unwrap();
    let expected = 1. / (1e200 * (1.0f64 + 0.25 + 0.04).sqrt());
    let gain = p.report().normalization_gain;
    assert!(
        gain > 0. && ((gain - expected) / expected).abs() < 4. * f64::EPSILON,
        "{gain:e}"
    );
    for (y, x) in p.channel(0).unwrap().iter().zip(&h) {
        assert_eq!(y.to_bits(), (x * gain).to_bits());
    }
    assert!((p.report().maximum_channel_energy - 1.).abs() < 8. * f64::EPSILON);
    assert!(p.channel(0).unwrap().iter().any(|x| *x != 0.));
    // The largest finite magnitude also works.
    let p = prepare_impulse_response(48_000, &[&[f64::MAX, -f64::MAX]], &energy_request(1e300))
        .unwrap();
    assert!((p.report().maximum_channel_energy / 1e300 - 1.).abs() < 8. * f64::EPSILON);
}

#[test]
fn energy_normalization_handles_subnormal_scale_responses() {
    // Squares underflow to zero (1e-620); the old path refused this as Silent.
    let h = [1e-310, 3e-311, 0.0];
    assert!(h[0] < f64::MIN_POSITIVE);
    let p = prepare_impulse_response(48_000, &[&h], &energy_request(1e-300)).unwrap();
    let expected = 1e-150 / (1e-310 * (1.0f64 + 0.09).sqrt());
    let gain = p.report().normalization_gain;
    // Subnormal inputs carry reduced precision (about 1e-5 relative at 1e-310).
    assert!(
        ((gain - expected) / expected).abs() < 1e-4,
        "{gain:e} {expected:e}"
    );
    for (y, x) in p.channel(0).unwrap().iter().zip(&h) {
        assert_eq!(y.to_bits(), (x * gain).to_bits());
    }
    let e = p.report().maximum_channel_energy;
    assert!((e / 1e-300 - 1.).abs() < 1e-4, "{e:e}");
    // The smallest subnormal is still not silent.
    let tiny = [f64::from_bits(1), 0.0];
    let p = prepare_impulse_response(48_000, &[&tiny], &energy_request(1e-300)).unwrap();
    assert!((p.channel(0).unwrap()[0] / 1e-150 - 1.).abs() < 4. * f64::EPSILON);
    // An f32 subnormal-scale response widened to f64 normalizes and builds f32 banks.
    let small32 = [1e-40f32, -5e-41, 2e-41];
    assert!(small32[0] < f32::MIN_POSITIVE);
    let widened: Vec<f64> = small32.iter().map(|&x| f64::from(x)).collect();
    let p = prepare_impulse_response(48_000, &[&widened], &energy_request(0.25)).unwrap();
    assert!((p.report().maximum_channel_energy - 0.25).abs() < 1e-14);
    let mut engine = p
        .engine32(
            0,
            ConvolutionPartitioning::ZeroLatency { head: 16 },
            16,
            1 << 20,
        )
        .unwrap();
    assert_eq!(engine.process_sample(1.0), p.channel(0).unwrap()[0] as f32);
}

#[test]
fn unrepresentable_normalization_gains_are_typed_refusals() {
    // Gain underflow: a huge response normalized to a tiny target.
    let huge = [1e300, 1e300];
    assert_eq!(
        prepare_impulse_response(48_000, &[&huge], &energy_request(1e-300)),
        Err(ImpulseResponseError::UnrepresentableGain)
    );
    let peak = ImpulseResponseRequest {
        normalization: ResponseNormalization::Peak { target: 1e-300 },
        ..request(48_000, ResponseRateConversion::RequireEqualRates)
    };
    assert_eq!(
        prepare_impulse_response(48_000, &[&huge], &peak),
        Err(ImpulseResponseError::UnrepresentableGain)
    );
    // Gain overflow: a subnormal response normalized to unit energy.
    let tiny = [1e-310, 0.0];
    assert_eq!(
        prepare_impulse_response(48_000, &[&tiny], &energy_request(1.0)),
        Err(ImpulseResponseError::NonFinite)
    );
    // Genuinely silent responses are still refused.
    assert_eq!(
        prepare_impulse_response(48_000, &[&[0.0, -0.0]], &energy_request(1.0)),
        Err(ImpulseResponseError::Silent)
    );
}
