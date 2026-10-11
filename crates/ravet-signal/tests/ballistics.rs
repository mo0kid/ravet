use ravet_signal::{
    BallisticsBank as Bank, BallisticsBank32 as Bank32, BallisticsConfig as C,
    BallisticsFilter as F, BallisticsFilter32 as F32, BallisticsMode as Mode,
};
fn close(a: f64, b: f64, error: f64) {
    assert!(
        (a - b).abs() <= error,
        "{a} != {b}: {} > {error}",
        (a - b).abs()
    );
}
fn reference(level: f64, input: f64, attack: f64, release: f64, mode: Mode) -> f64 {
    let target = input.abs();
    let a = if target > level { attack } else { release };
    match mode {
        Mode::Peak => (1. - a) * level + a * target,
        Mode::Rms => ((1. - a) * level * level + a * target * target).sqrt(),
    }
}
// Error bound for an n-step normal f32 product: coefficient rounding and initial
// error are separate from gamma_n multiplication rounding (u <= EPSILON).
fn tail_bound(initial: f32, ideal_initial: f64, factor: f32, ideal_factor: f64, n: usize) -> f64 {
    let observed_power = f64::from(factor).powi(n as i32);
    let ideal_power = ideal_factor.powi(n as i32);
    let nu = n as f64 * f64::from(f32::EPSILON);
    (f64::from(initial) - ideal_initial).abs() * ideal_power
        + f64::from(initial).abs() * (observed_power - ideal_power).abs()
        + f64::from(initial).abs() * observed_power * nu / (1. - nu)
}
#[test]
fn independent_amplitude_energy_steps_rate_scaling_zero_and_actual_precision_coefficients() {
    for mode in [Mode::Peak, Mode::Rms] {
        for rate in [8000., 44100., 48000., 96000., 192000., 768000.] {
            for time in [0., 0.0001, 0.01, 0.1, 60.] {
                let c = C::new(rate, time, time, mode).unwrap();
                assert_eq!(c.sample_rate(), rate);
                assert_eq!(c.attack_seconds(), time);
                assert_eq!(c.release_seconds(), time);
                assert_eq!(c.mode(), mode);
                let ideal = if time == 0. {
                    1.
                } else {
                    -(-1. / (rate * time)).exp_m1()
                };
                assert_eq!(c.attack_alpha(), ideal);
                assert_eq!(c.release_alpha(), ideal);
                assert_eq!(c.attack_complement(), 1. - ideal);
                let a = ideal as f32;
                let comp = 1. - a;
                let admissible = time == 0.
                    || (0. < a
                        && a < 1.
                        && 0. < comp
                        && comp < 1.
                        && (mode != Mode::Rms || comp.sqrt() != 1.));
                assert_eq!(c.quantize_f32().is_ok(), admissible);
                if let Ok(q) = c.quantize_f32() {
                    assert_eq!(q.attack_alpha(), a);
                    assert_eq!(q.release_complement(), comp);
                    assert_eq!(q.sample_rate(), rate);
                    assert_eq!(q.attack_seconds(), time);
                    assert_eq!(q.release_seconds(), time);
                    assert_eq!(q.mode(), mode);
                    let mut f = F32::new(q);
                    let expected = if mode == Mode::Peak { a } else { a.sqrt() };
                    assert_eq!(f.process_sample(-1.), expected);
                }
            }
        }
        for rate in [8000., 48000., 192000.] {
            let time = 0.01;
            let n = (rate * time) as usize;
            let c = C::new(rate, time, time, mode).unwrap();
            let mut f = F::new(c);
            let mut f32 = F32::new(c.quantize_f32().unwrap());
            for i in 1..=n {
                let energy = -(-(i as f64) / (rate * time)).exp_m1();
                let expected = if mode == Mode::Peak {
                    energy
                } else {
                    energy.sqrt()
                };
                close(f.process_sample(1.), expected, 5e-13);
                close(f64::from(f32.process_sample(1.)), expected, 2e-5);
            }
            f.reset_to(1.).unwrap();
            f32.reset_to(1.).unwrap();
            for i in 1..=n {
                let exponent = -(i as f64) / (rate * time);
                let expected = (exponent / if mode == Mode::Rms { 2. } else { 1. }).exp();
                close(f.process_sample(0.), expected, 5e-13);
                close(f64::from(f32.process_sample(0.)), expected, 4e-5);
            }
        }
        let mut instant = F::new(C::new(48000., 0., 0., mode).unwrap());
        for x in [1., -0.4, 0., 17., 0.] {
            assert_eq!(instant.process_sample(x), x.abs());
        }
    }
}
#[test]
fn independent_weighted_energy_convolution_and_switched_recursion_distinguish_peak_from_rms() {
    for mode in [Mode::Peak, Mode::Rms] {
        for (attack, release) in [(0.002, 0.002), (0.001, 0.03), (0., 0.01), (0.01, 0.)] {
            let config = C::new(48000., attack, release, mode).unwrap();
            let q = config.quantize_f32().unwrap();
            let mut f = F::new(config);
            let mut f32 = F32::new(q);
            let mut expected = 0.;
            let mut expected32 = 0.;
            let mut tail_initial = (0_f32, 0_f64);
            for i in 0..4096 {
                let x = if i < 2048 {
                    (i as f64 * 0.037).sin() * 0.4 + ((i * 17 % 29) as f64 - 14.) / 43.
                } else {
                    0.
                };
                expected = reference(
                    expected,
                    x,
                    config.attack_alpha(),
                    config.release_alpha(),
                    mode,
                );
                close(f.process_sample(x), expected, 5e-13);
                let x = x as f32;
                expected32 = reference(
                    expected32,
                    f64::from(x),
                    f64::from(q.attack_alpha()),
                    f64::from(q.release_alpha()),
                    mode,
                );
                let previous = f32.level();
                let actual = f32.process_sample(x);
                if i < 2048 || mode == Mode::Peak {
                    close(f64::from(actual), expected32, 3e-6);
                } else {
                    let factor = q.release_complement().sqrt();
                    assert_eq!(actual, previous * factor);
                    let bound = tail_bound(
                        tail_initial.0,
                        tail_initial.1,
                        factor,
                        (1. - f64::from(q.release_alpha())).sqrt(),
                        i - 2047,
                    );
                    close(f64::from(actual), expected32, bound + 1e-15);
                }
                if i == 2047 {
                    tail_initial = (actual, expected32);
                }
            }
        }
    }
    let c = C::new(48000., 0.003, 0.003, Mode::Rms).unwrap();
    let mut f = F::new(c);
    let mut weighted = 0.;
    let mut squares = [0.; 200];
    for i in 0..200 {
        let x = ((i * 13 % 17) as f64 - 8.) / 9.;
        squares[i] = x * x;
        // Direct finite convolution, independently of a recursive state update.
        weighted = squares[..=i]
            .iter()
            .enumerate()
            .map(|(j, &p)| c.attack_alpha() * c.attack_complement().powi((i - j) as i32) * p)
            .sum::<f64>();
        close(f.process_sample(x), weighted.sqrt(), 3e-14);
    }
    assert!(weighted > 0.);
    let mut peak = F::new(C::new(48000., 0.01, 0.01, Mode::Peak).unwrap());
    let mut rms = F::new(C::new(48000., 0.01, 0.01, Mode::Rms).unwrap());
    for _ in 0..480 {
        peak.process_sample(1.);
        rms.process_sample(1.);
    }
    close(peak.level(), 1. - (-1_f64).exp(), 3e-14);
    close(rms.level(), (1. - (-1_f64).exp()).sqrt(), 3e-14);
    assert!(rms.level() - peak.level() > 0.15);
    // Equal-time sinusoid has an exact energy recurrence, not a window-RMS reset.
    let c = C::new(48000., 0.02, 0.02, Mode::Rms).unwrap();
    let mut f = F::new(c);
    let mut energy = 0.;
    let mut extrema = [f64::INFINITY, 0.];
    for i in 0..32768 {
        let x = (std::f64::consts::TAU * 193. / 4096. * i as f64).sin();
        energy = (1. - c.attack_alpha()) * energy + c.attack_alpha() * x * x;
        let actual = f.process_sample(x);
        close(actual * actual, energy, 2e-13);
        if i >= 16384 {
            extrema[0] = extrema[0].min(actual);
            extrema[1] = extrema[1].max(actual);
        }
    }
    assert!((0.706..0.708).contains(&extrema[0]) && (0.706..0.708).contains(&extrema[1]));
    assert!(extrema[1] > extrema[0]);
}
#[test]
fn wide_magnitude_scaled_rms_matches_independent_normalized_energy_and_exact_equal_levels() {
    let c = C::new(48000., 0.003, 0.04, Mode::Rms).unwrap();
    let q = c.quantize_f32().unwrap();
    for scale in [2_f64.powi(1000), 2_f64.powi(-1000), 1.] {
        let mut f = F::new(c);
        let mut expected = 0.;
        for i in 0..4096 {
            let normalized = if i < 2048 {
                ((i * 31 % 127) as f64 - 63.) / 127.
            } else {
                0.
            };
            expected = reference(
                expected,
                normalized,
                c.attack_alpha(),
                c.release_alpha(),
                Mode::Rms,
            );
            close(
                f.process_sample(normalized * scale) / scale,
                expected,
                5e-13,
            );
        }
    }
    for scale in [2_f32.powi(120), 2_f32.powi(-120), 1.] {
        let mut f = F32::new(q);
        let mut expected = 0.;
        let mut tail_initial = (0_f32, 0_f64);
        for i in 0..4096 {
            let normalized = if i < 2048 {
                ((i * 31 % 127) as f32 - 63.) / 127.
            } else {
                0.
            };
            expected = reference(
                expected,
                f64::from(normalized),
                f64::from(q.attack_alpha()),
                f64::from(q.release_alpha()),
                Mode::Rms,
            );
            let previous = f.level();
            let actual = f.process_sample(normalized * scale);
            if i < 2048 {
                close(f64::from(actual / scale), expected, 5e-6);
            } else {
                let factor = q.release_complement().sqrt();
                assert_eq!(actual, previous * factor);
                let bound = tail_bound(
                    tail_initial.0,
                    tail_initial.1,
                    factor,
                    (1. - f64::from(q.release_alpha())).sqrt(),
                    i - 2047,
                );
                close(f64::from(actual / scale), expected, bound + 1e-15);
            }
            if i == 2047 {
                tail_initial = (actual / scale, expected);
            }
        }
    }
    for amplitude in [f64::MAX, f64::MIN_POSITIVE, f64::from_bits(1), 1.] {
        let mut f = F::new(c);
        f.reset_to(amplitude).unwrap();
        for _ in 0..128 {
            assert_eq!(f.process_sample(-amplitude), amplitude);
        }
        let mut attack = F::new(c);
        let actual = attack.process_sample(amplitude);
        assert!(actual.is_finite());
        assert_eq!(actual, amplitude * c.attack_alpha().sqrt());
        let release = f.process_sample(0.);
        assert!(release.is_finite());
        assert_eq!(release, amplitude * c.release_complement().sqrt());
    }
    for amplitude in [f32::MAX, f32::MIN_POSITIVE, f32::from_bits(1), 1.] {
        let mut f = F32::new(q);
        f.reset_to(amplitude).unwrap();
        for _ in 0..128 {
            assert_eq!(f.process_sample(-amplitude), amplitude);
        }
        let mut attack = F32::new(q);
        let actual = attack.process_sample(amplitude);
        assert!(actual.is_finite());
        assert_eq!(actual, amplitude * q.attack_alpha().sqrt());
        let release = f.process_sample(0.);
        assert!(release.is_finite());
        assert_eq!(release, amplitude * q.release_complement().sqrt());
    }
}
#[test]
fn native_slow_release_is_qualified_separately_from_ideal_timing_and_endpoint_rejections() {
    for mode in [Mode::Peak, Mode::Rms] {
        assert!(
            C::new(768000., 60., 60., mode)
                .unwrap()
                .quantize_f32()
                .is_err()
        );
        let c = C::new(384000., 60., 60., mode).unwrap();
        let q = c.quantize_f32().unwrap();
        assert!(q.release_alpha() > 0. && q.release_complement() < 1.);
        let mut f = F32::new(q);
        f.reset_to(1.).unwrap();
        let mut expected = 1_f32;
        for _ in 0..200000 {
            expected = if mode == Mode::Peak {
                expected + q.release_alpha() * (0. - expected)
            } else {
                expected * q.release_complement().sqrt()
            };
            assert_eq!(f.process_sample(0.), expected);
        }
        let ideal = (-200000_f64 / (384000. * 60.) / if mode == Mode::Rms { 2. } else { 1. }).exp();
        assert!(f.level() < 1. && f.level() > 0.97);
        // Native rounding at this admitted precision extreme is visibly different.
        assert!((f64::from(f.level()) - ideal).abs() > 0.002);
        for time in [f64::from_bits(1), 1e-300, 1e-12, 1e-8] {
            assert!(C::new(48000., time, 0., mode).is_err());
        }
    }
}

macro_rules! native_lifecycle {
    ($name:ident,$sample:ty,$filter:ty,$bank:ty,$config:expr) => {
        #[test]
        fn $name() {
            for mode in [Mode::Peak, Mode::Rms] {
                let c = ($config)(mode);
                let input: Vec<$sample> = (0..513)
                    .map(|i| ((i as f64 * 0.071).sin() * 0.73) as $sample)
                    .collect();
                let mut scalar = <$filter>::new(c);
                let expected: Vec<_> = input.iter().map(|&x| scalar.process_sample(x)).collect();
                let mut whole = <$filter>::new(c);
                let mut split = whole.clone();
                let mut inplace = whole.clone();
                let mut output = vec![0.; input.len()];
                whole.process_block(&input, &mut output).unwrap();
                assert_eq!(output, expected);
                let mut output = vec![0.; input.len()];
                let mut at = 0;
                for n in [0, 1, 17, 0, 3, 127, 2, 0, 363] {
                    split
                        .process_block(&input[at..at + n], &mut output[at..at + n])
                        .unwrap();
                    at += n;
                }
                assert_eq!(at, input.len());
                assert_eq!(output, expected);
                let mut copied = input.clone();
                inplace.process_in_place(&mut copied);
                assert_eq!(copied, expected);
                let before = whole.level();
                let mut sentinel = [99.; 3];
                assert!(whole.process_block(&[1.; 2], &mut sentinel).is_err());
                assert_eq!(sentinel, [99.; 3]);
                assert_eq!(whole.level(), before);
                for level in [-1., <$sample>::NAN, <$sample>::INFINITY] {
                    assert!(whole.reset_to(level).is_err());
                    assert_eq!(whole.level(), before);
                }
                for initial in [0., 0.6, <$sample>::MAX] {
                    whole.reset_to(initial).unwrap();
                    assert_eq!(whole.level(), initial);
                    assert_eq!(whole.process_sample(-initial), initial);
                }
                whole.reset_to(<$sample>::from_bits(1)).unwrap();
                whole.process_block(&[], &mut []).unwrap();
                assert_eq!(whole.level(), <$sample>::from_bits(1));
                whole.snap_to_zero();
                assert_eq!(whole.level(), 0.);
                whole.reset_to(<$sample>::MIN_POSITIVE).unwrap();
                whole.snap_to_zero();
                assert_eq!(whole.level(), <$sample>::MIN_POSITIVE);
                whole.process_sample(<$sample>::INFINITY);
                assert!(whole.process_sample(0.).is_infinite());
                whole.snap_to_zero();
                assert!(whole.level().is_infinite());
                whole.process_sample(<$sample>::NAN);
                assert!(whole.process_sample(<$sample>::INFINITY).is_nan());
                whole.snap_to_zero();
                assert!(whole.level().is_nan());
                whole.replace_config(c);
                assert_eq!(whole.level(), 0.);
                for channels in 1..=64 {
                    let mut bank = <$bank>::new(c, channels, 137).unwrap();
                    let mut reference = bank.clone();
                    let mut inplace = bank.clone();
                    let mut split = bank.clone();
                    assert_eq!(bank.channels(), channels);
                    assert_eq!(bank.maximum_block_frames(), 137);
                    assert_eq!(*bank.config(), c);
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
                    let mut expected: Vec<Vec<$sample>> = data
                        .iter()
                        .enumerate()
                        .map(|(ch, x)| {
                            x[..137]
                                .iter()
                                .map(|&x| reference.process_sample(ch, x).unwrap())
                                .collect()
                        })
                        .collect();
                    let inputs: Vec<&[$sample]> = data.iter().map(Vec::as_slice).collect();
                    let mut outputs = vec![vec![99.; 140]; channels];
                    let mut refs: Vec<&mut [$sample]> =
                        outputs.iter_mut().map(Vec::as_mut_slice).collect();
                    let measured = allocation_counter::measure(|| {
                        bank.process_planar(&inputs, &mut refs, 137).unwrap()
                    });
                    assert_eq!(measured.count_total, 0);
                    assert_eq!(measured.count_current, 0);
                    for (ch, x) in outputs.iter().enumerate() {
                        assert_eq!(&x[..137], &expected[ch]);
                        assert_eq!(&x[137..], &[99.; 3]);
                        assert_eq!(bank.level(ch).unwrap(), reference.level(ch).unwrap());
                    }
                    let mut data_inplace = data.clone();
                    let mut refs: Vec<&mut [$sample]> =
                        data_inplace.iter_mut().map(Vec::as_mut_slice).collect();
                    inplace.process_planar_in_place(&mut refs, 137).unwrap();
                    for (ch, x) in data_inplace.iter().enumerate() {
                        assert_eq!(&x[..137], &expected[ch]);
                        assert_eq!(&x[137..], &data[ch][137..]);
                    }
                    let mut outputs = vec![vec![99.; 140]; channels];
                    let mut at = 0;
                    for n in [0, 1, 13, 0, 64, 59] {
                        let input: Vec<&[$sample]> = data.iter().map(|x| &x[at..]).collect();
                        let mut output: Vec<&mut [$sample]> =
                            outputs.iter_mut().map(|x| &mut x[at..]).collect();
                        split.process_planar(&input, &mut output, n).unwrap();
                        at += n;
                    }
                    assert_eq!(at, 137);
                    for (ch, x) in outputs.iter().enumerate() {
                        assert_eq!(&x[..137], &expected[ch]);
                        assert_eq!(split.level(ch).unwrap(), bank.level(ch).unwrap());
                    }
                    let previous = bank.clone();
                    let mut bad = vec![vec![99.; 140]; channels];
                    let mut output: Vec<&mut [$sample]> =
                        bad.iter_mut().map(Vec::as_mut_slice).collect();
                    let last = channels - 1;
                    output[last] = &mut [];
                    assert!(bank.process_planar(&inputs, &mut output, 1).is_err());
                    assert!(bad.iter().all(|x| x.iter().all(|&x| x == 99.)));
                    let mut output: Vec<&mut [$sample]> =
                        bad.iter_mut().map(Vec::as_mut_slice).collect();
                    assert!(bank.process_planar(&inputs, &mut output, 138).is_err());
                    assert!(
                        bank.process_planar(&inputs[..last], &mut output, 0)
                            .is_err()
                    );
                    let mut short_input = inputs.clone();
                    short_input[last] = &[];
                    assert!(bank.process_planar(&short_input, &mut output, 1).is_err());
                    let mut short_inplace: Vec<&mut [$sample]> =
                        bad.iter_mut().map(Vec::as_mut_slice).collect();
                    short_inplace[last] = &mut [];
                    assert!(bank.process_planar_in_place(&mut short_inplace, 1).is_err());
                    assert!(bad.iter().all(|x| x.iter().all(|&x| x == 99.)));
                    let mut output: Vec<&mut [$sample]> =
                        bad.iter_mut().map(Vec::as_mut_slice).collect();
                    assert!(bank.process_sample(channels, 1.).is_err());
                    assert!(bank.level(channels).is_err());
                    assert!(bank.reset_to(-1.).is_err());
                    for ch in 0..channels {
                        assert_eq!(bank.level(ch).unwrap(), previous.level(ch).unwrap());
                    }
                    bank.reset_to(<$sample>::from_bits(1)).unwrap();
                    bank.process_planar(&inputs, &mut output, 0).unwrap();
                    assert_eq!(bank.level(last).unwrap(), <$sample>::from_bits(1));
                    bank.snap_to_zero();
                    assert_eq!(bank.level(last).unwrap(), 0.);
                    bank.reset_to(0.8).unwrap();
                    bank.replace_config(c);
                    assert_eq!(bank.level(last).unwrap(), 0.);
                    bank.process_sample(last, <$sample>::NAN).unwrap();
                    assert!(bank.process_sample(last, 0.).unwrap().is_nan());
                    bank.snap_to_zero();
                    assert!(bank.level(last).unwrap().is_nan());
                    if channels > 1 {
                        assert!(bank.process_sample(0, 0.1).unwrap().is_finite());
                        assert!(bank.level(last).unwrap().is_nan());
                    }
                    bank.reset();
                    bank.process_sample(last, <$sample>::NEG_INFINITY).unwrap();
                    assert_eq!(bank.process_sample(last, 0.).unwrap(), <$sample>::INFINITY);
                    bank.replace_config(c);
                    assert_eq!(bank.level(last).unwrap(), 0.);
                    expected.clear();
                }
                let measured = allocation_counter::measure(|| {
                    let mut f = <$filter>::new(c);
                    let mut b = <$bank>::new(c, 64, 65536).unwrap();
                    for _ in 0..10000 {
                        f.process_sample(0.3);
                        b.process_sample(63, 0.1).unwrap();
                    }
                    f.reset();
                    f.snap_to_zero();
                    f.replace_config(c);
                    b.reset();
                    b.reset_to(0.7).unwrap();
                    b.snap_to_zero();
                    b.replace_config(c);
                    for (ch, max) in [
                        (0, 1),
                        (65, 1),
                        (1, 0),
                        (1, 65537),
                        (usize::MAX, usize::MAX),
                    ] {
                        assert!(<$bank>::new(c, ch, max).is_err());
                    }
                });
                assert_eq!(measured.count_total, 0);
                assert_eq!(measured.count_current, 0);
            }
        }
    };
}
native_lifecycle!(
    native64_scalar_planar_preflight_partition_poison_reset_and_zero_heap,
    f64,
    F,
    Bank,
    |mode| C::new(48000., 0.003, 0.07, mode).unwrap()
);
native_lifecycle!(
    native32_scalar_planar_preflight_partition_poison_reset_and_zero_heap,
    f32,
    F32,
    Bank32,
    |mode| C::new(48000., 0.003, 0.07, mode)
        .unwrap()
        .quantize_f32()
        .unwrap()
);
#[test]
fn configuration_failure_preserves_existing_detectors_and_every_control_path_is_heap_free() {
    for mode in [Mode::Peak, Mode::Rms] {
        let c = C::new(48000., 0.01, 0.2, mode).unwrap();
        let mut f = F::new(c);
        f.process_sample(0.2);
        let mut expected = f.clone();
        let measured = allocation_counter::measure(|| {
            for rate in [0., -1., 0.5, 768001., f64::NAN, f64::INFINITY] {
                assert!(C::new(rate, 0., 0., mode).is_err());
            }
            for time in [-1., 60.1, f64::NAN, f64::INFINITY, 1e-300] {
                assert!(C::new(48000., time, 0., mode).is_err());
                assert!(C::new(48000., 0., time, mode).is_err());
            }
            for rate in [8000., 44100., 48000., 96000., 192000., 768000.] {
                for time in [0., 0.001, 0.01, 0.1, 60.] {
                    let c = C::new(rate, time, time, mode).unwrap();
                    let _ = c.quantize_f32();
                    let _ = F::new(c);
                }
            }
        });
        assert_eq!(measured.count_total, 0);
        assert_eq!(measured.count_current, 0);
        for x in [0.1, 0.7, -0.2, 0.] {
            assert_eq!(f.process_sample(x), expected.process_sample(x));
        }
    }
}

macro_rules! graph_fixture {
    ($name:ident,$sample:ty,$bank:ty,$convert:expr,$tolerance:expr) => {
        #[test]
        fn $name() {
            use ravet_foundation::{
                AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
                Processor, Transport,
            };
            use ravet_graph::{
                Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source,
            };
            use std::sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            };
            struct Stereo {
                mode: Mode,
                bank: Option<$bank>,
                config: Option<ProcessConfig>,
                frames: Arc<AtomicUsize>,
                deactivations: Arc<AtomicUsize>,
            }
            impl Processor<$sample> for Stereo {
                fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
                    self.config = None;
                    self.bank = None;
                    if c.layout() != AudioLayout::STEREO {
                        return Err(PrepareError::UnsupportedLayout);
                    }
                    let profile =
                        ($convert)(C::new(c.sample_rate(), 0.003, 0.07, self.mode).unwrap());
                    self.bank = Some(<$bank>::new(profile, 2, c.maximum_frames()).unwrap());
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
                            let source = b.audio.input(ch).unwrap()[i];
                            let envelope = self
                                .bank
                                .as_mut()
                                .unwrap()
                                .process_sample(ch, source)
                                .unwrap();
                            b.audio.output_mut(ch).unwrap()[i] = source / (1. + envelope);
                        }
                    }
                    self.frames.fetch_add(b.audio.frames(), Ordering::Relaxed);
                    ProcessStatus::Continue
                }
                fn reset(&mut self) {
                    if let Some(b) = self.bank.as_mut() {
                        b.reset();
                    }
                    self.frames.store(0, Ordering::Relaxed);
                }
                fn deactivate(&mut self) {
                    if self.config.take().is_some() {
                        self.bank = None;
                        self.deactivations.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            for mode in [Mode::Peak, Mode::Rms] {
                let profile = ($convert)(C::new(48000., 0.003, 0.07, mode).unwrap());
                let input: [Vec<$sample>; 2] = std::array::from_fn(|ch| {
                    (0..1501)
                        .map(|i| {
                            ((i as f64 * (0.17 + ch as f64 * 0.037)).sin()
                                * (if ch == 0 { 0.8 } else { 0.07 })
                                + ((i * 13 % 23) as f64 - 11.) / 73.) as $sample
                        })
                        .collect()
                });
                let expected: [Vec<f64>; 2] = std::array::from_fn(|ch| {
                    let mut level = 0.;
                    input[ch]
                        .iter()
                        .map(|&x| {
                            level = reference(
                                level,
                                x as f64,
                                profile.attack_alpha() as f64,
                                profile.release_alpha() as f64,
                                mode,
                            );
                            x as f64 / (1. + level)
                        })
                        .collect()
                });
                let frames = Arc::new(AtomicUsize::new(0));
                let deactivations = Arc::new(AtomicUsize::new(0));
                let node = GraphNode {
                    id: NodeId(19),
                    layout: AudioLayout::STEREO,
                    processor: Box::new(Stereo {
                        mode,
                        bank: None,
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
graph_fixture!(
    actual_native64_stereo_ballistics_gain_graph_and_lifecycle,
    f64,
    Bank,
    |c: C| c,
    3e-13
);
graph_fixture!(
    actual_native32_stereo_ballistics_gain_graph_and_lifecycle,
    f32,
    Bank32,
    |c: C| c.quantize_f32().unwrap(),
    3e-6
);
