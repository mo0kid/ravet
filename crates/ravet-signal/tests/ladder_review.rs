use ravet_signal::{
    LadderBank as B, LadderBank32 as B32, LadderDesign as D, LadderError as E, LadderFilter as F,
    LadderFilter32 as F32, LadderMode as M,
};
const MODES: [M; 6] = [
    M::LowPass12,
    M::LowPass24,
    M::HighPass12,
    M::HighPass24,
    M::BandPass12,
    M::BandPass24,
];
#[test]
fn physical_and_divided_endpoint_policy_standard_rates_and_neighbours() {
    for rate in [
        8000., 11025., 16000., 22050., 44100., 48000., 96000., 192000., 384000., 768000.,
    ] {
        for ratio in [1e-5, 0.45] {
            let endpoint: f64 = rate * ratio;
            for cutoff in [endpoint.next_down(), endpoint, endpoint.next_up()] {
                let admitted = cutoff >= rate * 1e-5
                    && cutoff <= rate * 0.45
                    && (1e-5..=0.45).contains(&(cutoff / rate));
                let design = D::new(rate, cutoff, 0.5, 1.);
                assert_eq!(design.is_ok(), admitted);
                if let Ok(d) = design {
                    assert_eq!(d.cutoff_hz(), cutoff);
                    d.quantize_f32().unwrap();
                } else {
                    assert_eq!(design, Err(E::Cutoff));
                }
                println!(
                    "endpoint rate={rate} requested_ratio={ratio} actual_ratio={:.17e} cutoff={cutoff:.17e} admitted={admitted}",
                    cutoff / rate
                );
            }
        }
    }
}
macro_rules! underflow {
    ($name:ident,$t:ty,$f:ty,$convert:expr,$r:expr) => {
        #[test]
        fn $name() {
            let design = $convert(D::new(48000., 1000., $r, 1.).unwrap());
            let g = design.gain();
            let k = design.feedback();
            let lambda = k * (((g * g) * g) * g);
            assert!(k > 0.);
            assert_eq!(lambda, 0.);
            let mut filter = <$f>::new(design);
            filter.process_sample(<$t>::NAN);
            let outputs = filter.process_sample(0.);
            assert!(outputs.low_pass24.is_nan());
            assert!(filter.last_solve().saturated_input.is_nan());
            filter.reset();
            let x = 0.17 as $t;
            filter.process_sample(x);
            assert_eq!(filter.last_solve().iterations, 0);
            assert_eq!(filter.last_solve().saturated_input, x.tanh());
            assert_eq!(filter.last_solve().scalar_residual, 0.);
            let state = filter.capacitor_state().map(|v| (1. - g) * v);
            let b = ((state[0] * g + state[1]) * g + state[2]) * g + state[3];
            let x = -0.21 as $t;
            let c = design.input_drive() * x - k * b;
            filter.process_sample(x);
            assert_eq!(filter.last_solve().saturated_input, c.tanh());
            assert_eq!(filter.last_solve().iterations, 0);
            println!(
                "positive feedback rounded-zero lambda native{} k={k:e} G={g:e} lambda={lambda:e}",
                std::mem::size_of::<$t>() * 8
            );
        }
    };
}
underflow!(
    positive_k_rounded_zero_lambda_history_and_finite_native64,
    f64,
    F,
    |d: D| d,
    1e-320
);
underflow!(
    positive_k_rounded_zero_lambda_history_and_finite_native32,
    f32,
    F32,
    |d: D| d.quantize_f32().unwrap(),
    5e-45
);
#[test]
fn native32_low_cutoff_million_frame_dc_precision_floor_is_observable() {
    for ratio in [1e-5, 1e-4, 1e-3, 1e-2] {
        let d = D::new(48000., 48000. * ratio, 0., 1.).unwrap();
        let mut f = F::new(d);
        let mut q = F32::new(d.quantize_f32().unwrap());
        let mut y = f.process_sample(0.5);
        let mut z = q.process_sample(0.5);
        for _ in 1..1_000_000 {
            y = f.process_sample(0.5);
            z = q.process_sample(0.5);
        }
        let target = 0.5f64.tanh();
        let errors = [
            (z.low_pass24 as f64 - target).abs(),
            z.high_pass12.abs() as f64,
            z.high_pass24.abs() as f64,
            z.band_pass12.abs() as f64,
            z.band_pass24.abs() as f64,
        ];
        let next_state = q.capacitor_state();
        q.process_sample(0.5);
        assert_eq!(q.capacitor_state(), next_state);
        assert!(
            MODES
                .iter()
                .all(|m| y.select(*m).is_finite() && z.select(*m).is_finite())
        );
        assert!((y.low_pass24 - target).abs() < 1e-10);
        assert!(errors.iter().all(|e| *e < 0.02));
        if ratio == 1e-5 {
            assert!(errors[0] > 1e-4);
        }
        println!(
            "LOW-CUTOFF DC million frames ratio={ratio:e} actualG={:e} native64 LP24error={:e} native32 [LP24,HP12,HP24,BP12,BP24]errors={errors:?}",
            q.design().gain(),
            (y.low_pass24 - target).abs()
        );
    }
}
macro_rules! bank {
    ($name:ident,$t:ty,$b:ty,$f:ty,$convert:expr) => {
        #[test]
        fn $name() {
            let design = $convert(D::new(48000., 2300., 0.7, 2.).unwrap());
            let changed = $convert(D::new(48000., 4700., 0.3, 7.).unwrap());
            let mut bank = <$b>::new(design, 64, 8).unwrap();
            let mut independent: Vec<$f> = (0..64).map(|_| <$f>::new(design)).collect();
            for frame in 0..123 {
                let input: Vec<$t> = (0..64)
                    .map(|ch| ((frame * (ch + 1)) as f64 * 0.017).sin() as $t)
                    .collect();
                let mut output = vec![0.; 64];
                let mode = MODES[(frame / 11) % 6];
                bank.process_frame(&input, &mut output, mode).unwrap();
                for ch in 0..64 {
                    assert_eq!(
                        output[ch],
                        independent[ch].process_sample(input[ch]).select(mode)
                    );
                    assert_eq!(
                        bank.capacitor_state(ch).unwrap(),
                        independent[ch].capacitor_state()
                    );
                    assert_eq!(bank.last_solve(ch).unwrap(), independent[ch].last_solve());
                }
            }
            let states: Vec<_> = (0..64)
                .map(|ch| bank.capacitor_state(ch).unwrap())
                .collect();
            let infos: Vec<_> = (0..64).map(|ch| bank.last_solve(ch).unwrap()).collect();
            let data: Vec<Vec<$t>> = (0..64).map(|_| vec![0.2; 9]).collect();
            for fault in 0..9 {
                let mut output: Vec<Vec<$t>> = (0..64).map(|_| vec![99.; 9]).collect();
                let mut inputs: Vec<&[$t]> = data.iter().map(Vec::as_slice).collect();
                if fault == 0 {
                    inputs.pop();
                }
                if fault == 2 {
                    inputs[63] = &data[63][..1];
                }
                let mut views: Vec<&mut [$t]> = output.iter_mut().map(Vec::as_mut_slice).collect();
                if fault == 1 {
                    views.pop();
                }
                if fault == 3 {
                    views[63] = &mut [];
                }
                let expected = match fault {
                    0 | 1 | 5 | 8 => E::ChannelCount,
                    2 | 3 | 7 => E::BufferLength,
                    _ => E::TooManyFrames,
                };
                let result = match fault {
                    0..=4 => bank.process(
                        &inputs,
                        &mut views,
                        if fault == 4 { 9 } else { 2 },
                        M::LowPass24,
                    ),
                    5 => {
                        views.pop();
                        bank.process_in_place(&mut views, 2, M::LowPass24)
                    }
                    6 => bank.process_in_place(&mut views, 9, M::LowPass24),
                    7 => {
                        views[63] = &mut [];
                        bank.process_in_place(&mut views, 2, M::LowPass24)
                    }
                    _ => bank.process_frame(&[0.; 63], &mut [0.; 64], M::LowPass24),
                };
                assert_eq!(result, Err(expected));
                for ch in 0..64 {
                    assert_eq!(bank.capacitor_state(ch).unwrap(), states[ch]);
                    assert_eq!(bank.last_solve(ch).unwrap(), infos[ch]);
                    assert!(output[ch].iter().all(|x| *x == 99.));
                }
            }
            let mut wrong = [99.; 63];
            assert_eq!(
                bank.process_frame(&[0.; 64], &mut wrong, M::HighPass24),
                Err(E::ChannelCount)
            );
            assert_eq!(wrong, [99.; 63]);
            assert_eq!(bank.process_channel(64, 0.).err(), Some(E::ChannelCount));
            assert_eq!(bank.last_solve(64).err(), Some(E::ChannelCount));
            assert_eq!(bank.capacitor_state(64).err(), Some(E::ChannelCount));
            bank.set_design(changed);
            assert_eq!(bank.design(), changed);
            for ch in 0..64 {
                assert_eq!(bank.capacitor_state(ch).unwrap(), states[ch]);
            }
            bank.set_bypassed(true);
            let input: Vec<$t> = (0..64).map(|ch| ch as $t / 64.).collect();
            let mut output = vec![0.; 64];
            bank.process_frame(&input, &mut output, M::BandPass24)
                .unwrap();
            assert_eq!(input, output);
            for ch in 0..64 {
                assert_eq!(bank.capacitor_state(ch).unwrap(), states[ch]);
                assert_eq!(bank.last_solve(ch).unwrap(), infos[ch]);
            }
            bank.replace_design(design);
            assert_eq!(bank.design(), design);
            assert!(bank.is_bypassed());
            for ch in 0..64 {
                assert_eq!(bank.capacitor_state(ch).unwrap(), [0.; 4]);
            }
            bank.set_bypassed(false);
            bank.snap_to_zero();
            bank.reset();
            for ch in 0..64 {
                assert_eq!(bank.capacitor_state(ch).unwrap(), [0.; 4]);
            }
            for channel in 0..64 {
                bank.process_channel(channel, <$t>::MIN_POSITIVE * 0.5)
                    .unwrap();
                assert!(
                    bank.capacitor_state(channel)
                        .unwrap()
                        .iter()
                        .any(|x| x.is_subnormal())
                );
            }
            bank.snap_to_zero();
            for channel in 0..64 {
                assert_eq!(bank.capacitor_state(channel).unwrap(), [0.; 4]);
            }
            let mut single = <$f>::new(design);
            let before = single.capacitor_state();
            let mut output = vec![99.; 65537];
            assert_eq!(
                single.process(&vec![0.; 65537], &mut output, M::LowPass24),
                Err(E::TooManyFrames)
            );
            assert!(output.iter().all(|x| *x == 99.));
            let mut input = vec![0.1; 65537];
            assert_eq!(
                single.process_in_place(&mut input, M::LowPass24),
                Err(E::TooManyFrames)
            );
            assert!(input.iter().all(|x| *x == 0.1));
            assert_eq!(single.capacitor_state(), before);
        }
    };
}
bank!(
    native64_all_bank_refusals_every_history_and64_independent_filters,
    f64,
    B,
    F,
    |d: D| d
);
bank!(
    native32_all_bank_refusals_every_history_and64_independent_filters,
    f32,
    B32,
    F32,
    |d: D| d.quantize_f32().unwrap()
);
// For F(u) = u - tanh(c - lambda*u), lambda >= 0 implies F'(u) >= 1.
// Tanh is 1-Lipschitz, so moving the equation at a reference root changes
// its residual by at most |delta c| + |delta lambda| * |reference root|.
// Monotonicity transfers that residual bound to root distance. These helpers
// round each nonzero floating operation upward; exactly matching equations
// retain zero drift, rather than quietly relaxing their strict native bound.
fn upward_positive(value: f64) -> f64 {
    if value == 0. { 0. } else { value.next_up() }
}
fn equation_drift_bound(
    c: f64,
    lambda: f64,
    reference_c: f64,
    reference_lambda: f64,
    root: f64,
) -> f64 {
    let dc = upward_positive((c - reference_c).abs());
    let dl = upward_positive((lambda - reference_lambda).abs());
    let product = if dl == 0. || root == 0. {
        0.
    } else {
        (dl * root.abs()).next_up()
    };
    upward_positive(dc + product)
}
// The exact actual-equation root is bounded by |reference root| + drift.
// Scale the selected native relative-error budget by that upper bound; matching
// equations execute the original strict formula unchanged.
fn root_error_allowance(root: f64, drift: f64, epsilon: f64, minimum: f64) -> f64 {
    if drift == 0. {
        4. * epsilon * root.abs() + 2. * minimum
    } else {
        let magnitude = upward_positive(root.abs() + drift);
        let native = upward_positive(4. * epsilon * magnitude);
        upward_positive(upward_positive(native + 2. * minimum) + drift)
    }
}
#[test]
fn independent450_digit_nonzero_history_cancellation_knee_roots_tight_native_bounds() {
    let mut maxima = [0_f64; 2];
    let mut counts = [0; 2];
    let mut exact_equations = [0; 2];
    let mut max_drift = [0_f64; 2];
    for row in include_str!("ladder-history-decimal.csv").lines() {
        let values: Vec<f64> = row.split(',').map(|v| v.parse().unwrap()).collect();
        assert_eq!(values.len(), 10);
        let d = D::new(48000., 48000. * values[1], values[2], values[3]).unwrap();
        let pattern = values[4] as usize;
        let warmup = values[5] as usize;
        let input = |i: usize| match pattern {
            0 => 0.3,
            1 => {
                if i.is_multiple_of(2) {
                    0.4
                } else {
                    -0.2
                }
            }
            _ => 0.17 * (i as f64 * 0.041).sin(),
        };
        let (actual, c, lambda, eps, min, index) = if values[0] == 64. {
            let mut f = F::new(d);
            for i in 0..warmup {
                f.process_sample(input(i));
            }
            let g = d.gain();
            let k = d.feedback();
            let state = f.capacitor_state().map(|v| (1. - g) * v);
            let b = ((state[0] * g + state[1]) * g + state[2]) * g + state[3];
            let c = d.input_drive() * values[6] - k * b;
            let lambda = k * (((g * g) * g) * g);
            f.process_sample(values[6]);
            (
                f.last_solve().saturated_input,
                c,
                lambda,
                f64::EPSILON,
                f64::from_bits(1),
                0,
            )
        } else {
            let d = d.quantize_f32().unwrap();
            let mut f = F32::new(d);
            for i in 0..warmup {
                f.process_sample(input(i) as f32);
            }
            let g = d.gain();
            let k = d.feedback();
            let state = f.capacitor_state().map(|v| (1. - g) * v);
            let b = ((state[0] * g + state[1]) * g + state[2]) * g + state[3];
            let c = d.input_drive() * values[6] as f32 - k * b;
            let lambda = k * (((g * g) * g) * g);
            f.process_sample(values[6] as f32);
            (
                f.last_solve().saturated_input as f64,
                c as f64,
                lambda as f64,
                f32::EPSILON as f64,
                f32::from_bits(1) as f64,
                1,
            )
        };
        let root = values[7];
        let reference_c = values[8];
        let reference_lambda = values[9];
        let exact =
            c.to_bits() == reference_c.to_bits() && lambda.to_bits() == reference_lambda.to_bits();
        exact_equations[index] += usize::from(exact);
        let drift = equation_drift_bound(c, lambda, reference_c, reference_lambda, root);
        max_drift[index] = max_drift[index].max(drift);
        if exact {
            assert_eq!(drift, 0.);
        }
        let error = (actual - root).abs();
        let strict = 4. * eps * root.abs() + 2. * min;
        assert!(
            error <= root_error_allowance(root, drift, eps, min),
            "native{} error={error:e} strict={strict:e} drift={drift:e} actual={actual:e} expected={root:e} actual c/lambda={c:e}/{lambda:e} reference={reference_c:e}/{reference_lambda:e}",
            values[0]
        );
        if exact && root.is_normal() {
            maxima[index] = maxima[index].max(error / root.abs());
        }
        println!(
            "HISTORY equation native{} case={} actual_c={c:.17e} actual_lambda={lambda:.17e} reference_c={reference_c:.17e} reference_lambda={reference_lambda:.17e} exact={exact} drift={drift:e} error={error:e}",
            values[0], counts[index]
        );
        counts[index] += 1;
    }
    assert_eq!(counts, [1008, 1008]);
    println!(
        "HISTORY Decimal450 counts={counts:?}; exact-backend equations={exact_equations:?}; exact-equation native64/native32 max relative={maxima:?}; max independent equation drift={max_drift:?}; rounded native residual selection, no Decimal-neighbour optimality assertion"
    );
}
#[test]
fn independent_alternate_equations_admit_drift_but_reject_solver_error() {
    let mut count = 0;
    let mut strict_rejections = 0;
    let mut genuine_error_rejections = 0;
    let mut max_root_shift = 0_f64;
    let mut max_allowance = 0_f64;
    for row in include_str!("ladder-equation-drift.csv").lines() {
        let v: Vec<f64> = row.split(',').map(|x| x.parse().unwrap()).collect();
        assert_eq!(v.len(), 6);
        let [
            reference_c,
            reference_lambda,
            reference_root,
            actual_c,
            actual_lambda,
            actual_root,
        ] = v[..].try_into().unwrap();
        let strict = 4. * f64::EPSILON * reference_root.abs() + 2. * f64::from_bits(1);
        let drift = equation_drift_bound(
            actual_c,
            actual_lambda,
            reference_c,
            reference_lambda,
            reference_root,
        );
        let allowance =
            root_error_allowance(reference_root, drift, f64::EPSILON, f64::from_bits(1));
        let root_shift = (actual_root - reference_root).abs();
        assert!(
            root_shift <= allowance,
            "independent root shift {root_shift:e} > {allowance:e}"
        );
        strict_rejections += usize::from(root_shift > strict);
        // Adversarial validator candidates, not reported production faults.
        // Their equation is unchanged; the extra error must not be admitted
        // merely because a separately bounded equation drift is allowed.
        let erroneous_solver_result = actual_root + 4. * allowance;
        assert!((erroneous_solver_result - reference_root).abs() > allowance);
        genuine_error_rejections += 1;
        max_root_shift = max_root_shift.max(root_shift);
        max_allowance = max_allowance.max(allowance);
        count += 1;
    }
    assert_eq!(count, 60);
    assert_eq!(genuine_error_rejections, 60);
    assert!(strict_rejections >= 50);
    println!(
        "SYNTHETIC independent Decimal450 equation perturbations={count}; strict-only rejects={strict_rejections}; drift admits all, genuine-error rejects={genuine_error_rejections}; max root shift={max_root_shift:e} max allowance={max_allowance:e}; deliberate 1e-4 perturbations, no observed-platform-drift claim"
    );
}
macro_rules! modal_tail {
    ($name:ident,$t:ty,$f:ty,$convert:expr)=>{
        #[test]
        fn $name() {
            for ratio in [0.07, 0.13, 0.44] {
                for resonance in [0.9, 1.] {
                    let mut f = <$f>::new($convert(
                        D::new(48000., 48000. * ratio, resonance, 1.).unwrap(),
                    ));
                    let mut peaks = [0_f64; 6];
                    let mut nonzero = [0usize; 6];
                    let mut subnormal = [0usize; 6];
                    for frame in 0..32768 {
                        let x = if frame == 0 {
                            0.001 as $t
                        } else {
                            0.
                        };
                        let y = f.process_sample(x);
                        for (i, mode) in MODES.iter().enumerate() {
                            let value = y.select(*mode);
                            assert!(value.is_finite());
                            if frame >= 32000 {
                                peaks[i] = peaks[i].max(value.abs() as f64);
                                nonzero[i] += usize::from(value != 0.);
                                subnormal[i] += usize::from(value.is_subnormal());
                            }
                        }
                    }
                    if resonance == 0.9 {
                        assert!(peaks.iter().all(|v| *v < 1e-20));
                    } else {
                        assert!(peaks.iter().all(|v| *v > 1e-6 && *v < 0.02));
                    }
                    println!(
                        "ALL-MODE finite-tail native{} ratio={ratio} resonance={resonance}; peaks={peaks:?} nonzero={nonzero:?} subnormal={subnormal:?}",
                        std::mem::size_of::<$t>() * 8
                    );
                }
            }
        }
    }
}
modal_tail!(
    native64_all_six_mode_finite_tail_tracking_not_universal_decay,
    f64,
    F,
    |d: D| d
);
modal_tail!(
    native32_all_six_mode_finite_tail_tracking_not_universal_decay,
    f32,
    F32,
    |d: D| d.quantize_f32().unwrap()
);
#[test]
fn caller_visible_error_messages_preserve_numeric_spacing() {
    for (error, text) in [
        (
            E::SampleRate,
            "ladder sample rate must be finite 8000..768000 Hz",
        ),
        (
            E::Cutoff,
            "ladder original and normalized cutoff must be representable 1e-5..0.45 of rate",
        ),
        (E::Resonance, "ladder resonance must be finite 0..1"),
        (E::Drive, "ladder input drive must be finite 1..32"),
        (
            E::ChannelCount,
            "ladder channel count must match prepared 1..64 layout",
        ),
        (E::MaximumFrames, "ladder maximum frames must be 1..65536"),
    ] {
        assert_eq!(error.to_string(), text);
    }
}
