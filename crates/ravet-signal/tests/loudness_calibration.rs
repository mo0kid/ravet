use ravet_signal::{
    KWeightingDesign as Design, LoudnessChannelRole as Role,
    LoudnessRecordCapacities as Capacities, LoudnessTailPolicy as Tail,
    PreparedLoudnessMeter64 as Meter,
};
#[test]
fn independent_decimal_bilinear_coefficients_and_complex_transfer() {
    let mut coefficients = 0_f64;
    let mut responses = 0_f64;
    for line in include_str!("data/loudness-coefficient-oracle.csv").lines() {
        let parts: Vec<_> = line.split(',').collect();
        let rate = parts[1].parse::<u32>().unwrap();
        let d = Design::new(rate).unwrap();
        let expected = parts[4].parse::<f64>().unwrap();
        let actual = if parts[0] == "C" {
            let (b, a) = d.section(parts[2].parse().unwrap()).unwrap();
            let index = parts[3].parse::<usize>().unwrap();
            if index < 3 { b[index] } else { a[index - 3] }
        } else {
            d.magnitude_at(parts[2].parse().unwrap()).unwrap()
        };
        let error = (actual - expected).abs();
        if parts[0] == "C" {
            coefficients = coefficients.max(error);
            assert!(error < 3e-15, "{line} actual={actual:.17e}");
        } else {
            responses = responses.max(error / (1. + expected));
            assert!(
                error < 2e-8 * (1. + expected),
                "{line} actual={actual:.17e} error={error:.17e}"
            );
        }
    }
    println!(
        "DECIMAL90 170 coefficients/85 physical complex responses: max_abs_coeff={coefficients:.17e} max_response_error/(1+H)={responses:.17e}"
    );
}
fn meter(rate: u32, roles: &[Role]) -> Meter {
    Meter::new(
        &Design::new(rate).unwrap(),
        roles,
        1024,
        Capacities {
            integrated: 2048,
            lra: 2048,
        },
        true,
    )
    .unwrap()
}
fn feed(m: &mut Meter, rate: u32, length: usize, amplitude: impl Fn(usize) -> f64) {
    let mut samples = [0_f64; 1024];
    let channels = m.roles().len();
    for start in (0..length).step_by(1024) {
        let n = (length - start).min(1024);
        for (j, sample) in samples[..n].iter_mut().enumerate() {
            let i = start + j;
            *sample = amplitude(i) * (std::f64::consts::TAU * 997. * i as f64 / rate as f64).sin();
        }
        let refs: [&[f64]; 64] = std::array::from_fn(|_| &samples[..n]);
        m.observe_planar(&refs[..channels], n).unwrap();
    }
}
#[test]
fn own_steady_tone_mono_stereo_surround_and_lfe_have_declared_power_weights() {
    let profiles: &[&[Role]] = &[
        &[Role::Front],
        &[Role::Front, Role::Front],
        &[
            Role::Front,
            Role::Front,
            Role::Front,
            Role::Surround,
            Role::Surround,
            Role::Excluded,
        ],
    ];
    let mut maximum = 0_f64;
    for rate in [8000, 8001, 16000, 44100, 48000, 96000, 192000, 768000] {
        for roles in profiles {
            let mut m = meter(rate, roles);
            feed(&mut m, rate, 5 * rate as usize, |_| 0.1);
            let weight: f64 = roles.iter().map(|r| r.power_weight()).sum();
            let expected = -0.691
                + 20. * (0.1 * Design::new(rate).unwrap().magnitude_at(997.).unwrap()).log10()
                + 10. * (weight / 2.).log10();
            let actual = m.reading().short_term_lufs.unwrap();
            let error = (actual - expected).abs();
            maximum = maximum.max(error);
            assert!(
                error < 2e-7,
                "rate={rate} weights={weight} actual={actual} expected={expected}"
            );
            println!(
                "TONE rate={rate} channels={} weights={weight} actual3s={actual:.17e} expected_transfer={expected:.17e} error={error:.17e}",
                roles.len()
            );
        }
    }
    println!("TONE full profile max3s-vs-transfer discrepancy={maximum:.17e}LU");
}
#[test]
fn generated_forty_second_level_steps_independent_lra_levels_and_repetition() {
    for rate in [8000, 8001, 48000] {
        for range in [5., 10., 20.] {
            let amplitude = 0.1;
            let quiet = amplitude * 10_f64.powf(-range / 20.);
            let mut m = meter(rate, &[Role::Front]);
            feed(&mut m, rate, 40 * rate as usize, |n| {
                if n < 20 * rate as usize {
                    quiet
                } else {
                    amplitude
                }
            });
            m.finish(Tail::LraSilence1500ms).unwrap();
            let r = m.analyse_programme().unwrap();
            let actual = r.loudness_range_lu.unwrap();
            assert!(
                (actual - range).abs() < 2e-6,
                "rate={rate} requested={range} observed={actual}"
            );
            assert_eq!(r.source_frames, 40 * rate as u64);
            assert_eq!(r.tail_frames, (3 * rate as u64).div_ceil(2));
            println!(
                "LRA_STEPS rate={rate} 40s range={range} actual={actual:.17e}, I={:?}, absolute/relativeLRA={}/{}, tail={}",
                r.integrated_lufs, r.lra_absolute_count, r.lra_relative_count, r.tail_frames
            );
            let original = r;
            m.reset();
            feed(&mut m, rate, 80 * rate as usize, |n| {
                if n % (40 * rate as usize) < 20 * rate as usize {
                    quiet
                } else {
                    amplitude
                }
            });
            m.finish(Tail::LraSilence1500ms).unwrap();
            let repeated = m.analyse_programme().unwrap();
            assert!(
                (repeated.loudness_range_lu.unwrap() - original.loudness_range_lu.unwrap()).abs()
                    < 2e-6
            );
            // Long added silence cannot enter the final absolute/relative gates.
            m.reset();
            feed(&mut m, rate, 60 * rate as usize, |n| {
                if n < 20 * rate as usize {
                    quiet
                } else if n < 40 * rate as usize {
                    amplitude
                } else {
                    0.
                }
            });
            m.finish(Tail::None).unwrap();
            let silent = m.analyse_programme().unwrap();
            assert!((silent.loudness_range_lu.unwrap() - range).abs() < 2e-6);
        }
    }
}
#[test]
fn external_reference_percentile_convention_and_histogram_difference_are_explicit() {
    // This is a diagnostic of the reviewed FFmpeg6.1.1 rank/bin policy, not a
    // replacement oracle for this meter's publicly chosen nearest-index policy.
    for seconds in [5., 3.3] {
        let rate = 48000;
        let mut m = meter(rate, &[Role::Front]);
        feed(&mut m, rate, (seconds * rate as f64) as usize, |_| 0.1);
        m.finish(Tail::LraSilence1500ms).unwrap();
        let r = m.analyse_programme().unwrap();
        let mut values = m.lra_records().to_vec();
        values.sort_by(f64::total_cmp);
        let n = values.len();
        let own_low = ((n - 1) * 10 + 50) / 100;
        let own_high = ((n - 1) * 95 + 50) / 100;
        let external_low = ((n * 10 + 50) / 100).saturating_sub(1);
        let external_high = ((n * 95 + 50) / 100).saturating_sub(1);
        let bin = |x: f64| ((x + 70.) * 100.).trunc() / 100. - 70.;
        let external = bin(values[external_high]) - bin(values[external_low]);
        assert_eq!(r.lra_low_lufs.unwrap(), values[own_low]);
        assert_eq!(r.lra_high_lufs.unwrap(), values[own_high]);
        assert_ne!(external_low, own_low);
        // Actual selected external summaries2.2/2.7 agree with their own distinct
        // convention to the printed0.1LU precision; our exact rank is unchanged.
        let printed = if seconds == 5. { 2.2 } else { 2.7 };
        assert!((external - printed).abs() < 0.051);
        println!(
            "LRA_RANK_DIAG original_seconds={seconds} padded_records={n} own_low/high_indices={own_low}/{own_high} external_low/high_indices={external_low}/{external_high} own_actual={} external_bins_actual={external:.17e} printed={printed}",
            r.loudness_range_lu.unwrap()
        );
    }
}

// ---------------------------------------------------------------------------------------
// Discriminating follow-up evidence.
//
// Scope: the CSV below is a Decimal recurrence over the SAME calibration mathematical
// facts that production uses (f0, Q, gain, exponent). It independently checks arithmetic
// and rounding of the bilinear transform, not the calibration parameters themselves. The
// independent anchor for the parameters is the 48 kHz literal block further down. Rate
// tables here are SELECTED-transfer facts at named frequencies only: they are neither a
// wideband supremum, a profile accuracy claim nor a conformity claim.
const ORACLE: &str = include_str!("data/loudness-coefficient-oracle.csv");
const ORACLE_RATES: [u32; 17] = [
    8000, 8001, 8002, 8003, 8004, 8005, 8006, 8007, 8008, 8009, 16000, 22050, 44100, 48000, 96000,
    192000, 768000,
];
fn oracle_response(rate: u32, frequency: f64) -> f64 {
    let row = ORACLE
        .lines()
        .map(|line| line.split(',').collect::<Vec<_>>())
        .find(|p| {
            p[0] == "R"
                && p[1].parse::<u32>().unwrap() == rate
                && p[2].parse::<f64>().unwrap() == frequency
        })
        .unwrap_or_else(|| panic!("missing response row R,{rate},{frequency}"));
    row[4].parse().unwrap()
}
fn gain_db(numerator: f64, denominator: f64) -> f64 {
    20. * (numerator / denominator).log10()
}
#[test]
fn oracle_csv_has_exactly_the_170_coefficient_85_response_255_row_identities() {
    let mut seen = std::collections::BTreeSet::new();
    let (mut coefficients, mut responses) = (0, 0);
    for line in ORACLE.lines() {
        let parts: Vec<_> = line.split(',').collect();
        assert_eq!(parts.len(), 5, "no missing or extra fields: {line:?}");
        let rate: u32 = parts[1].parse().unwrap();
        let field: f64 = parts[2].parse().unwrap();
        let index: usize = parts[3].parse().unwrap();
        assert!(parts[4].parse::<f64>().unwrap().is_finite(), "{line}");
        let kind = match parts[0] {
            "C" => {
                coefficients += 1;
                0_u8
            }
            "R" => {
                responses += 1;
                1
            }
            other => panic!("unknown row kind {other:?}"),
        };
        assert!(
            seen.insert((kind, rate, field.to_bits(), index)),
            "duplicate {line}"
        );
    }
    let mut expected = std::collections::BTreeSet::new();
    for rate in ORACLE_RATES {
        for section in 0..2 {
            for index in 0..5 {
                expected.insert((0_u8, rate, f64::from(section).to_bits(), index));
            }
        }
        let nyquist = rate as f64 / 2.;
        for frequency in [37., 137., 997., nyquist / 2., nyquist] {
            expected.insert((1_u8, rate, frequency.to_bits(), 0_usize));
        }
    }
    assert_eq!((coefficients, responses), (170, 85));
    assert_eq!(seen.len(), 255);
    assert_eq!(seen, expected, "every seventeen-rate identity exactly once");
}
// Numeric facts read from the primary ITU-R BS.1770-5 (11/2023) Annex 1 Tables 1 and 2 for
// the 48 kHz filter stages; no vendor source, table body or programme fixture is reproduced.
// Root measured at most 1.56e-12 between these literals and the CSV, so 3e-12 is the
// smallest justified round figure. This anchors the 48 kHz NUMERICAL calibration only.
const SHELF_B: [f64; 3] = [1.53512485958697, -2.69169618940638, 1.19839281085285];
const SHELF_A: [f64; 2] = [-1.69065929318241, 0.73248077421585];
const RLB_B: [f64; 3] = [1., -2., 1.];
const RLB_A: [f64; 2] = [-1.99004745483398, 0.99007225036621];
fn direct_form_one(b: [f64; 3], a: [f64; 2], input: &[f64]) -> Vec<f64> {
    let (mut x, mut y) = ([0.; 2], [0.; 2]);
    input
        .iter()
        .map(|&sample| {
            let value = b[0] * sample + b[1] * x[0] + b[2] * x[1] - a[0] * y[0] - a[1] * y[1];
            x = [sample, x[0]];
            y = [value, y[0]];
            value
        })
        .collect()
}
// |H(e^{jw})| from b0+b1 z^-1+b2 z^-2 over 1+a1 z^-1+a2 z^-2, with z^-1 = cos w - j sin w.
fn transfer(b: [f64; 3], a: [f64; 2], frequency: f64, rate: f64) -> f64 {
    let w = std::f64::consts::TAU * frequency / rate;
    let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2. * w).cos(), (2. * w).sin());
    let numerator = (b[0] + b[1] * c1 + b[2] * c2).hypot(b[1] * s1 + b[2] * s2);
    let denominator = (1. + a[0] * c1 + a[1] * c2).hypot(a[0] * s1 + a[1] * s2);
    numerator / denominator
}
#[test]
fn rate_48000_matches_independent_literal_coefficients_direct_form_and_transfer() {
    let rate = 48000;
    let d = Design::new(rate).unwrap();
    for (stage, (b, a)) in [(SHELF_B, SHELF_A), (RLB_B, RLB_A)].into_iter().enumerate() {
        let (actual_b, actual_a) = d.section(stage).unwrap();
        for (actual, expected) in actual_b.iter().chain(&actual_a).zip(b.iter().chain(&a)) {
            assert!(
                (actual - expected).abs() < 3e-12,
                "stage {stage}: actual={actual:.17e} literal={expected:.17e}"
            );
        }
    }
    assert_eq!(
        d.section(1).unwrap().0,
        RLB_B,
        "RLB numerator is exactly (1,-2,1)"
    );
    // Five seconds of the same 0.1-amplitude 997 Hz sine that `feed` generates, filtered by
    // direct-form I with the LITERAL coefficients, not production `section()`.
    let n = 5 * rate as usize;
    let samples: Vec<f64> = (0..n)
        .map(|i| 0.1 * (std::f64::consts::TAU * 997. * i as f64 / rate as f64).sin())
        .collect();
    let weighted = direct_form_one(RLB_B, RLB_A, &direct_form_one(SHELF_B, SHELF_A, &samples));
    let level = |from: usize, to: usize| {
        -0.691
            + 10.
                * (weighted[from..to].iter().map(|v| v * v).sum::<f64>() / (to - from) as f64)
                    .log10()
    };
    let mut m = meter(rate, &[Role::Front]);
    feed(&mut m, rate, n, |_| 0.1);
    let r = m.reading();
    assert_eq!(r.momentary_endpoint_frame, Some(n as u64));
    assert_eq!(r.short_term_endpoint_frame, Some(n as u64));
    let momentary = level(n - rate as usize * 4 / 10, n);
    let short = level(n - 3 * rate as usize, n);
    // 1e-8 LU: a few 1e-12 of coefficient difference is expected to move a 997 Hz level by
    // far less than this (hand estimate, not measured); the existing direct-form versus
    // transposed rounding discrepancy is recorded at the 1e-11 LU level.
    assert!((r.momentary_lufs.unwrap() - momentary).abs() < 1e-8);
    assert!((r.short_term_lufs.unwrap() - short).abs() < 1e-8);
    // The 3-s window holds 2991 whole cycles, so its mean square is the fixed mathematical
    // transfer exactly. 2e-7 LU is the tolerance of the existing own-tone test above.
    let h =
        transfer(SHELF_B, SHELF_A, 997., rate as f64) * transfer(RLB_B, RLB_A, 997., rate as f64);
    let expected = -0.691 + 20. * (0.1 * h).log10() - 10. * 2_f64.log10();
    assert!((short - expected).abs() < 2e-7, "{short} vs {expected}");
    assert!((r.short_term_lufs.unwrap() - expected).abs() < 2e-7);
    // Deliberately NOT asserted: a -3.01 LUFS 0 dBFS-sine target. This calibration reads
    // its own value for that tone, and no normative tone target is claimed here.
}
#[test]
fn selected_fixed_frequency_rate_response_differences_match_oracle_and_actual_same_frequency() {
    // Root-computed gain(rate)/gain(48 kHz) in dB at the SAME physical frequency, from the
    // CSV rows. Selected transfer facts only; no wideband or programme error is implied.
    const FIXED: [f64; 3] = [37., 137., 997.];
    const SELECTED: [(u32, [f64; 3]); 3] = [
        (
            8000,
            [
                0.21476563606221552,
                0.2157589211071858,
                0.014085366223794828,
            ],
        ),
        (
            16000,
            [
                0.08622600067613795,
                0.08644830208423482,
                0.04022748896751117,
            ],
        ),
        (
            768000,
            [
                -0.04056808322627038,
                -0.04059555469438051,
                -0.034856203438225195,
            ],
        ),
    ];
    let reference = Design::new(48000).unwrap();
    for (rate, table) in SELECTED {
        let d = Design::new(rate).unwrap();
        for (&frequency, &expected) in FIXED.iter().zip(&table) {
            let oracle = gain_db(
                oracle_response(rate, frequency),
                oracle_response(48000, frequency),
            );
            // The table literals are CSV-derived: only double rounding can differ.
            assert!(
                (oracle - expected).abs() < 1e-12,
                "rate={rate} {frequency} Hz CSV difference {oracle:.17e} vs table {expected:.17e}"
            );
            // 1e-7 dB: the observed response error is at most 1.95e-10 relative to (1+H), at
            // most about 1e-8 dB for this ratio of two responses, and far below these 0.014+ dB.
            let actual = gain_db(
                d.magnitude_at(frequency).unwrap(),
                reference.magnitude_at(frequency).unwrap(),
            );
            assert!(
                (actual - expected).abs() < 1e-7,
                "rate={rate} {frequency} Hz actual {actual:.17e} vs table {expected:.17e}"
            );
            assert!(
                expected.abs() > 0.01,
                "selected fact is not a negligible difference"
            );
        }
    }
}
#[test]
fn nyquist_relative_rate_responses_are_individually_scoped_against_48k_relative_points() {
    // Fs/4 and Nyquist are different physical frequencies at each rate, so each row compares
    // the same RELATIVE position of the band. They are independent from the fixed-Hz table.
    let reference = Design::new(48000).unwrap();
    let mut largest = 0_f64;
    for rate in ORACLE_RATES.into_iter().filter(|&rate| rate != 48000) {
        let d = Design::new(rate).unwrap();
        for (label, fraction) in [("Fs/4", 0.25), ("Nyquist", 0.5)] {
            let own = rate as f64 * fraction;
            let base = 48000. * fraction;
            let oracle = gain_db(oracle_response(rate, own), oracle_response(48000, base));
            let actual = gain_db(
                d.magnitude_at(own).unwrap(),
                reference.magnitude_at(base).unwrap(),
            );
            assert!(
                (actual - oracle).abs() < 1e-7,
                "rate={rate} {label} ({own} Hz vs {base} Hz): actual={actual:.17e} oracle={oracle:.17e}"
            );
            largest = largest.max(oracle.abs());
        }
    }
    assert!(
        largest > 0.1,
        "the table must contain real rate dependence: {largest}"
    );
}
fn step_records(d: Design, samples: &[f64]) -> Vec<f64> {
    let weighted = (0..2).fold(samples.to_vec(), |signal, stage| {
        let (b, a) = d.section(stage).unwrap();
        direct_form_one(b, a, &signal)
    });
    let end = |k: usize| k * d.sample_rate() as usize / 10;
    (4..)
        .map_while(|k| {
            (end(k) <= weighted.len()).then(|| {
                let window = &weighted[end(k - 4)..end(k)];
                -0.691
                    + 10.
                        * (window.iter().map(|v| v * v).sum::<f64>() / window.len() as f64).log10()
            })
        })
        .collect()
}
fn power_mean_level(values: &[f64]) -> f64 {
    10. * (values.iter().map(|v| 10_f64.powf(v / 10.)).sum::<f64>() / values.len() as f64).log10()
}
#[test]
fn forty_second_step_integrated_loudness_matches_independent_gates_and_closed_form() {
    for rate in [8000, 8001, 48000] {
        let d = Design::new(rate).unwrap();
        let loud_level =
            -0.691 + 20. * (0.1 * d.magnitude_at(997.).unwrap()).log10() - 10. * 2_f64.log10();
        for range in [5., 10., 20.] {
            let amplitude = 0.1;
            let quiet = amplitude * 10_f64.powf(-range / 20.);
            let level = |n: usize| {
                if n < 20 * rate as usize {
                    quiet
                } else {
                    amplitude
                }
            };
            let samples: Vec<f64> = (0..40 * rate as usize)
                .map(|i| level(i) * (std::f64::consts::TAU * 997. * i as f64 / rate as f64).sin())
                .collect();
            let mut m = meter(rate, &[Role::Front]);
            feed(&mut m, rate, 40 * rate as usize, level);
            m.finish(Tail::None).unwrap();
            let report = m.analyse_programme().unwrap();
            // Independent direct-form I records and independently written strict gates.
            let records = step_records(d, &samples);
            assert_eq!(records.len(), 397, "400 blocks give 397 four-block windows");
            assert_eq!(m.integrated_records().len(), records.len());
            for (i, (actual, expected)) in m.integrated_records().iter().zip(&records).enumerate() {
                assert!(
                    (actual - expected).abs() < 2e-9,
                    "rate={rate} range={range} record {i}"
                );
            }
            let absolute: Vec<f64> = records.iter().copied().filter(|v| *v > -70.).collect();
            let gate = power_mean_level(&absolute) - 10.;
            let relative: Vec<f64> = absolute.iter().copied().filter(|v| *v > gate).collect();
            let integrated = report.integrated_lufs.unwrap();
            assert!((integrated - power_mean_level(&relative)).abs() < 2e-9);
            assert!((report.integrated_relative_gate_lufs.unwrap() - gate).abs() < 2e-9);
            assert_eq!(report.integrated_absolute_count, absolute.len());
            assert_eq!(report.integrated_relative_count, relative.len());
            // Closed form from block counts: 197 pure-quiet, 197 pure-loud and three boundary
            // windows holding 1, 2 and 3 loud blocks. Quiet windows sit 5 and 10 LU below
            // loud, inside the gate about 12.6 LU below loud; 20 LU is outside it.
            let epsilon = 10_f64.powf(-range / 10.);
            let (count, mean_energy) = if range < 15. {
                (397, 0.5 * (1. + epsilon))
            } else {
                (200, (197. + 1.5 * (1. + epsilon)) / 200.)
            };
            assert_eq!(absolute.len(), 397);
            assert_eq!(relative.len(), count, "rate={rate} range={range}");
            assert_eq!(
                records[..197].iter().filter(|v| **v > gate).count(),
                if range < 15. { 197 } else { 0 }
            );
            // 0.02 LU covers non-integer 997 Hz cycles per block and the step transient;
            // the competing wrong gates differ from this closed form by 0.03 to 3 LU.
            let closed = loud_level + 10. * mean_energy.log10();
            assert!(
                (integrated - closed).abs() < 0.02,
                "rate={rate} range={range}: integrated={integrated} closed form={closed}"
            );
        }
    }
}
