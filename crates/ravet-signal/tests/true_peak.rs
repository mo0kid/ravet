use ravet_signal::{
    TruePeakBank64 as Bank, TruePeakDesign as Design, TruePeakError as E, TruePeakFactor as Factor,
    TruePeakMeter64 as Meter,
};
use std::collections::BTreeSet;
const FACTORS: [Factor; 3] = [Factor::Four, Factor::Eight, Factor::Sixteen];
fn make(half: usize, factor: Factor, beta: f64) -> Design {
    Design::new(48000, half, factor, beta).unwrap()
}
fn direct(design: &Design, input: &[f64]) -> f64 {
    let mut peak = 0_f64;
    for t in 0..input.len() + 2 * design.half_width() {
        for phase in 0..design.factor().factor() {
            let row = design.phase_coefficients(phase).unwrap();
            let mut y = 0.;
            for (j, h) in row.iter().enumerate() {
                if let Some(source) = t.checked_sub(j).and_then(|i| input.get(i)) {
                    y += h * source;
                }
            }
            peak = peak.max(y.abs());
        }
    }
    peak
}
// Textbook Neumaier accumulation, re-typed here so the oracle below does not
// share the crate's private summation helper.
fn fsum(values: impl Iterator<Item = f64>) -> f64 {
    let (mut sum, mut compensation) = (0_f64, 0_f64);
    for value in values {
        let next = sum + value;
        compensation += if sum.abs() >= value.abs() {
            (sum - next) + value
        } else {
            (value - next) + sum
        };
        sum = next;
    }
    sum + compensation
}
// Absolute phase-coordinate oracle. Coordinate numerator `c` is the source
// coordinate tau = c/F (source index units, negative before the first frame).
// With m = floor(c/F) and p = c mod F the value is
//     y(c) = sum_k x[k] * row_p[m + L - k],   0 <= m + L - k <= 2L,
// which never touches the bank's ring, scale, normalized scratch or step order.
// Only the prepared coefficient rows (separately qualified by the Decimal90
// oracle) are shared.
fn coordinate_value(design: &Design, x: &[f64], c: i64) -> f64 {
    let factor = design.factor().factor() as i64;
    let half = design.half_width() as i64;
    let (m, p) = (c.div_euclid(factor), c.rem_euclid(factor));
    let row = design.phase_coefficients(p as usize).unwrap();
    let lo = (m - half).max(0);
    let hi = (m + half).min(x.len() as i64 - 1);
    fsum((lo..=hi).map(|k| x[k as usize] * row[(m + half - k) as usize]))
}
// Maximum |y(c)| over exactly the coordinates reached after N source steps and
// `tail` zero steps: t = 0..N+tail, c = (t-L)*F + p, i.e.
// c in [-L*F, (N+tail-L)*F). Returns the first maximizing coordinate too.
fn oracle_scan(design: &Design, x: &[f64], tail: usize) -> (f64, i64) {
    let factor = design.factor().factor() as i64;
    let half = design.half_width() as i64;
    let steps = (x.len() + tail) as i64;
    let mut best = (0_f64, i64::MIN);
    for c in -half * factor..(steps - half) * factor {
        let value = coordinate_value(design, x, c).abs();
        if value > best.0 {
            best = (value, c);
        }
    }
    best
}
// Alternating-sign run whose magnitude ramps up to +0.9 on the very last frame;
// zero before the run. A truncated alternating (near-Nyquist) run overshoots
// just beyond its final sample, so the global interpolated maximum lies outside
// the source (coordinate > N-1) and needs the finish tail. The ramp makes the
// two ends asymmetric: the starting end is nearly silent.
fn ramped_alternating_end(n: usize, ramp: usize) -> Vec<f64> {
    (0..n)
        .map(|k| {
            if k + ramp < n {
                0.
            } else {
                let rise = (k + ramp - n + 1) as f64 / ramp as f64;
                let sign = if (n - 1 - k).is_multiple_of(2) {
                    1.
                } else {
                    -1.
                };
                0.9 * rise * sign
            }
        })
        .collect()
}
// Tapered quarter-rate cosine; the continuous peak lies at tau = 4n - 2*phase/pi.
fn quarter_rate_source(half: usize, length: usize, phase: f64) -> Vec<f64> {
    (0..length)
        .map(|i| {
            let ramp = (i.min(length - 1 - i) as f64 / (2 * half) as f64).min(1.);
            let envelope = 0.5 - 0.5 * (std::f64::consts::PI * ramp).cos();
            envelope * (std::f64::consts::FRAC_PI_2 * i as f64 + phase).cos()
        })
        .collect()
}
// Finite-window allowance around the analytic cos(pi/(4F)) quarter-rate peak.
// A-priori, from truncated-sinc/Kaiser leakage at 0.25 cycles/sample (distance
// 0.25 from the band edge), not from a recorded run: rectangular (beta 0)
// truncated-sinc tail ~ 0.4/L doubled for margin; Kaiser beta 6 leakage
// (~63 dB) 1e-3; beta >= 12 (>~117 dB) 1e-4. If a root run shows these are
// violated, report the measured value; do not silently widen.
fn finite_window_tolerance(half: usize, beta: f64) -> f64 {
    if beta == 0. {
        0.8 / half as f64
    } else if beta < 12. {
        1e-3
    } else {
        1e-4
    }
}
#[test]
fn every_profile_domain_dc_cardinal_support_and_actual_metadata() {
    for rate in [8000, 44100, 48000, 96000, 192000, 768000] {
        for factor in FACTORS {
            for half in [32, 128, 512] {
                for beta in [0., 6., 12., 16.] {
                    let d = Design::new(rate, half, factor, beta).unwrap();
                    assert_eq!(d.sample_rate(), rate);
                    assert_eq!(d.half_width(), half);
                    assert_eq!(d.factor(), factor);
                    assert_eq!(d.beta(), beta);
                    assert_eq!(d.taps(), 2 * half + 1);
                    assert!(d.phase_coefficients(factor.factor()).is_none());
                    assert_eq!(d.phase_dc_sum(factor.factor()), None);
                    for phase in 0..factor.factor() {
                        let row = d.phase_coefficients(phase).unwrap();
                        assert_eq!(row.len(), d.taps());
                        assert!(row.iter().all(|x| x.is_finite()));
                        assert!((d.phase_dc_sum(phase).unwrap() - 1.).abs() < 3e-15);
                        if phase == 0 {
                            for (j, &h) in row.iter().enumerate() {
                                assert_eq!(h, f64::from(j == half));
                            }
                        } else {
                            assert_eq!(row[2 * half], 0.);
                        }
                    }
                }
            }
        }
    }
    for (rate, half, beta, error) in [
        (7999, 32, 12., E::SampleRate),
        (768001, 32, 12., E::SampleRate),
        (48000, 31, 12., E::HalfWidth),
        (48000, 513, 12., E::HalfWidth),
        (48000, 32, -f64::MIN_POSITIVE, E::Beta),
        (48000, 32, 16_f64.next_up(), E::Beta),
        (48000, 32, f64::NAN, E::Beta),
        (48000, 32, f64::INFINITY, E::Beta),
    ] {
        assert_eq!(
            Design::new(rate, half, Factor::Eight, beta).unwrap_err(),
            error
        );
    }
}
#[test]
fn sample_rate_is_validated_metadata_and_216_admissions_are_36_coefficient_profiles() {
    // The library validates 8000..768000 and stores the rate, but rows depend only
    // on half-width, factor and beta (normalized sample coordinates). It cannot
    // verify the real source clock: a caller passing 8000 for a 768 kHz stream
    // gets bitwise-identical numerics. This test pins that actual behaviour; it is
    // not an endorsement of rate-independent filtering for every use.
    let rates = [8000, 44100, 48000, 96000, 192000, 768000];
    let (mut profiles, mut admissions) = (0, 0);
    for factor in FACTORS {
        for half in [32, 128, 512] {
            for beta in [0., 6., 12., 16.] {
                profiles += 1;
                let reference = make(half, factor, beta);
                let input: Vec<f64> = (0..97).map(|i| 0.5 * (i as f64 * 0.31).sin()).collect();
                let expected = (half == 32).then(|| {
                    let mut meter = Meter::new(&reference, 97).unwrap();
                    meter.observe(&input).unwrap();
                    meter.finish();
                    meter.reading()
                });
                for rate in rates {
                    admissions += 1;
                    let d = Design::new(rate, half, factor, beta).unwrap();
                    assert_eq!(d.sample_rate(), rate);
                    for phase in 0..factor.factor() {
                        assert_eq!(
                            d.phase_coefficients(phase).unwrap(),
                            reference.phase_coefficients(phase).unwrap(),
                            "rate{rate} changed coefficients"
                        );
                    }
                    if let Some(expected) = expected {
                        // Bank readings are also rate-independent (cheap profile only).
                        let mut meter = Meter::new(&d, 97).unwrap();
                        meter.observe(&input).unwrap();
                        meter.finish();
                        assert_eq!(meter.reading(), expected);
                    }
                }
            }
        }
    }
    assert_eq!(profiles, 36);
    assert_eq!(admissions, 216);
}
#[test]
fn phase_rows_are_mirror_images_supporting_the_time_reversal_proof() {
    // Row p tap j lies at d = j - L + p/F; row F-p tap j' at j' - L + 1 - p/F.
    // Setting d' = -d gives j' = 2L-1-j. The Kaiser-sinc kernel is even, the
    // support test |d| <= L is symmetric, tap 2L of every p>0 row is outside the
    // support (d = L + p/F > L) and the F-p row's own tap 2L likewise. Hence the
    // F-p row is the exact mirror of the p row on taps 0..2L-1 before
    // normalization, and both normalizations sum the same multiset. Numerically
    // the sums differ only in addition order, so mirror rows agree to ~1e-16.
    for factor in FACTORS {
        for half in [32, 33, 64, 127, 128, 511, 512] {
            for beta in [0., 0.5, 6., 15.999, 16.] {
                let d = make(half, factor, beta);
                let f = factor.factor();
                for p in 1..f {
                    let a = d.phase_coefficients(p).unwrap();
                    let b = d.phase_coefficients(f - p).unwrap();
                    assert_eq!(a[2 * half], 0.);
                    assert_eq!(b[2 * half], 0.);
                    for j in 0..2 * half {
                        assert!(
                            (a[j] - b[2 * half - 1 - j]).abs() < 1e-15,
                            "F{f} L{half} beta{beta} p{p} j{j}"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn independent_absolute_support_dot_products_partition_tail_and_exact_counts() {
    for factor in FACTORS {
        for half in [32, 128] {
            for length in [0, 1, 17, 67, 299] {
                let d = make(half, factor, 12.);
                let input: Vec<f64> = (0..length)
                    .map(|i| 0.43 * (i as f64 * 0.391).sin() + 0.17 * (i as f64 * 0.087).cos())
                    .collect();
                let mut single = Meter::new(&d, 65536).unwrap();
                assert_eq!(single.maximum_frames(), 65536);
                assert_eq!(single.design().half_width(), half);
                assert_eq!(single.last_phase_numerator(), None);
                single.observe(&input).unwrap();
                let live = single.reading();
                assert_eq!(
                    single.pending_source_suffix_frames(),
                    length.min(half) as u64
                );
                single.observe(&[]).unwrap();
                single.observe_f32(&[]).unwrap();
                assert_eq!(single.reading(), live);
                let mut partitioned = Meter::new(&d, 67).unwrap();
                let mut cursor = 0;
                for n in [0, 1, 67, 2, 31, 5].into_iter().cycle() {
                    if cursor == input.len() {
                        break;
                    }
                    let n = n.min(input.len() - cursor);
                    partitioned.observe(&input[cursor..cursor + n]).unwrap();
                    cursor += n;
                }
                assert_eq!(single.reading(), partitioned.reading());
                single.finish();
                partitioned.finish();
                assert_eq!(single.reading(), partitioned.reading());
                let reading = single.reading();
                assert_eq!(reading.source_frames, length as u64);
                assert_eq!(
                    reading.tail_frames,
                    if length == 0 { 0 } else { (2 * half) as u64 }
                );
                assert_eq!(
                    reading.phase_observations,
                    (reading.source_frames + reading.tail_frames) * factor.factor() as u64
                );
                assert!(reading.finalized);
                let expected = if input.is_empty() {
                    0.
                } else {
                    direct(&d, &input)
                };
                assert!(
                    (reading.interpolated_peak - expected).abs() < 2e-15 * (1. + expected),
                    "factor{factor:?} half{half} N{length} measured={} expected={expected}",
                    reading.interpolated_peak
                );
                assert_eq!(
                    reading.sample_peak,
                    input.iter().fold(0_f64, |a, v| a.max(v.abs()))
                );
                assert!(reading.interpolated_peak + 1e-15 >= reading.sample_peak);
                let before = single.reading();
                single.finish();
                assert_eq!(single.reading(), before);
                assert_eq!(single.observe_sample(0.), Err(E::Finalized));
                assert_eq!(single.observe(&[]), Err(E::Finalized));
                assert_eq!(single.observe_f32(&[]), Err(E::Finalized));
                assert_eq!(single.pending_source_suffix_frames(), 0);
                assert_eq!(single.reading(), before);
                single.reset();
                assert_eq!(single.reading(), Default::default());
                single.observe(&input).unwrap();
                single.finish();
                assert_eq!(single.reading(), before);
            }
        }
    }
}
#[test]
fn hand_cardinal_first_impulse_delay_phase_coordinates_and_leading_support_counters() {
    // Proves: a first-frame impulse is delayed by exactly L (phase zero),
    // leading support/counters/pending suffix and the last evaluated coordinate
    // numerator. It does NOT exercise emphasised trailing support; see the
    // last-source and emphasised-end tests below.
    for factor in FACTORS {
        let d = make(32, factor, 12.);
        let mut bank = Bank::new(&d, 1, 67).unwrap();
        assert_eq!(bank.last_phase_numerator(), None);
        assert_eq!(bank.pending_source_suffix_frames(), 0);
        bank.observe_frame(&[1.]).unwrap();
        assert_eq!(bank.source_frames(), 1);
        assert_eq!(
            bank.last_phase_numerator(),
            Some((factor.factor() - 1) as i128 - 32 * factor.factor() as i128)
        );
        assert_eq!(bank.pending_source_suffix_frames(), 1);
        let prefix = bank.reading(0).unwrap();
        assert_eq!(prefix.sample_peak, 1.);
        assert!(prefix.interpolated_peak < 0.001);
        bank.observe_planar(&[&[0.; 31]], 31).unwrap();
        assert!(bank.reading(0).unwrap().interpolated_peak < 1.);
        assert_eq!(bank.pending_source_suffix_frames(), 32);
        bank.observe_frame(&[0.]).unwrap();
        assert_eq!(bank.reading(0).unwrap().interpolated_peak, 1.);
        bank.finish();
        assert_eq!(
            bank.last_phase_numerator(),
            Some((33 + 64) * factor.factor() as i128 - 1 - 32 * factor.factor() as i128)
        );
        assert_eq!(bank.pending_source_suffix_frames(), 0);
        assert_eq!(bank.reading(0).unwrap().interpolated_peak, 1.);
    }
}
#[test]
fn last_source_impulse_reads_every_prepared_row_tap_in_absolute_coordinates() {
    // Single nonzero frame at the very end. The bank scales by |a|, so the
    // normalized window holds exactly 1.0 at one tap and the compensated dot
    // product returns that row entry unchanged: the bank peak must equal the
    // absolute-coordinate oracle and a*max|h| bit for bit. Rows are visited from
    // j=0 at t=N-1 to j=2L at t=N-1+2L, so all trailing taps of the last source
    // are read. (The peak is not expected to discriminate a tail much longer than
    // ~L steps, since far-tail coordinates only see small window edge weights;
    // the counters below do evidence the full 2L tail.)
    for factor in FACTORS {
        for half in [32, 128] {
            for amplitude in [0.75, -0.75] {
                let d = make(half, factor, 12.);
                let n = 2 * half + 3;
                let mut x = vec![0.; n];
                x[n - 1] = amplitude;
                let max_entry = (0..factor.factor())
                    .flat_map(|p| d.phase_coefficients(p).unwrap().iter().copied())
                    .fold(0_f64, |a, h| a.max(h.abs()));
                let mut meter = Meter::new(&d, 65536).unwrap();
                meter.observe(&x).unwrap();
                meter.finish();
                let reading = meter.reading();
                assert_eq!(reading.sample_peak, 0.75);
                assert_eq!(reading.interpolated_peak, 0.75 * max_entry);
                assert_eq!(oracle_scan(&d, &x, 2 * half).0, 0.75 * max_entry);
                assert_eq!(reading.tail_frames, 2 * half as u64);
                assert_eq!(
                    meter.last_phase_numerator(),
                    Some(
                        ((n + 2 * half) * factor.factor()) as i128
                            - 1
                            - (half * factor.factor()) as i128
                    )
                );
            }
        }
    }
}
#[test]
fn emphasised_end_and_start_match_absolute_coordinate_oracle_with_proved_mirror_symmetry() {
    // Time-reversal is asserted only because it is proved for this normalized
    // finite discrete design (also checked row by row in
    // phase_rows_are_mirror_images_supporting_the_time_reversal_proof):
    //  1. Grid of a finished N-frame run: G = {j/F : -L*F <= j <= (N+L)*F - 1}.
    //  2. Mirror tau' = (N-1) - tau maps j to (N-1)*F - j. Every coordinate with a
    //     nonzero value lies in the open interval (-L, N-1+L), i.e.
    //     j in (-L*F, (N-1+L)*F), which this map sends onto itself, and both G and
    //     the mirrored grid contain it. Coordinates outside it are exact zeros
    //     (distance > L from every source frame or exact-delta phase zero at d=L).
    //  3. Row F-p is the exact mirror of row p (see the row test), and
    //     y_rev(tau') = sum_k x[N-1-k] h_{F-p}(tau'-(N-1-k)) pairs the same
    //     (x, h) products as y(tau) in reversed order.
    // So the peak of reverse(x) equals the peak of x mathematically; numerically
    // only addition order differs, hence the explicit 4e-15 (2e-14 for L512,
    // up to 1025 terms) absolute-plus-relative allowance. No other symmetry is
    // assumed.
    let mut cases = Vec::new();
    for factor in FACTORS {
        for beta in [6., 12.] {
            cases.push((32, factor, beta, 209, 128));
        }
        cases.push((128, factor, 12., 800, 512));
    }
    // L512: more than 2*L+1 = 1025 frames so the history ring wraps. Work is
    // (N+2L)*F = 8,496 coordinates of <=1025 products per oracle and a similar
    // count per bank run.
    cases.push((512, Factor::Four, 12., 1100, 1000));
    for (half, factor, beta, n, ramp) in cases {
        let d = make(half, factor, beta);
        assert!(n > d.taps(), "history ring must wrap");
        let forward = ramped_alternating_end(n, ramp);
        let reverse: Vec<f64> = forward.iter().rev().copied().collect();
        assert_eq!(forward[n - 1], 0.9);
        assert_eq!(reverse[0], 0.9);
        let f = factor.factor() as i64;
        let tolerance = |expected: f64| (if half == 512 { 2e-14 } else { 4e-15 }) * (1. + expected);
        let (expected, at) = oracle_scan(&d, &forward, 2 * half);
        let (expected_reverse, at_reverse) = oracle_scan(&d, &reverse, 2 * half);
        // Fixture must discriminate: global maximum lies beyond the last source
        // coordinate (resp. before the first one) and a tail of L-1 zero steps
        // (which also omits the last frame's own coordinate) misses it.
        assert!(expected > 0.9 + 0.05, "F{f} L{half} beta{beta}: {expected}");
        assert!(at > (n as i64 - 1) * f, "F{f} L{half}: argmax {at}");
        assert!(at_reverse < 0, "F{f} L{half}: reverse argmax {at_reverse}");
        let (truncated, _) = oracle_scan(&d, &forward, half - 1);
        assert!(truncated + 0.05 < expected, "{truncated} vs {expected}");
        assert!((expected - expected_reverse).abs() <= tolerance(expected));
        let mut meter = Meter::new(&d, n).unwrap();
        meter.observe(&forward).unwrap();
        let live = meter.reading();
        assert!(
            live.interpolated_peak + 0.05 < expected,
            "finish omission must be visible"
        );
        assert_eq!(live.sample_peak, 0.9);
        meter.finish();
        let finished = meter.reading();
        assert!(
            (finished.interpolated_peak - expected).abs() <= tolerance(expected),
            "F{f} L{half} beta{beta}: bank {} oracle {expected}",
            finished.interpolated_peak
        );
        assert_eq!(finished.tail_frames, 2 * half as u64);
        assert_eq!(
            finished.phase_observations,
            (n + 2 * half) as u64 * f as u64
        );
        assert_eq!(
            meter.last_phase_numerator(),
            Some(((n + half - 1) as i128) * f as i128 + f as i128 - 1)
        );
        let mut partitioned = Meter::new(&d, 97).unwrap();
        for chunk in forward.chunks(97) {
            partitioned.observe(chunk).unwrap();
        }
        partitioned.finish();
        assert_eq!(partitioned.reading(), finished);
        let mut mirrored = Meter::new(&d, n).unwrap();
        mirrored.observe(&reverse).unwrap();
        // The leading overshoot (coordinate < 0) is complete before any finish.
        let live_reverse = mirrored.reading().interpolated_peak;
        assert!((live_reverse - expected_reverse).abs() <= tolerance(expected));
        mirrored.finish();
        let finished_reverse = mirrored.reading();
        assert_eq!(finished_reverse.interpolated_peak, live_reverse);
        assert!(
            (finished_reverse.interpolated_peak - expected_reverse).abs()
                <= tolerance(expected_reverse)
        );
        assert!(
            (finished.interpolated_peak - finished_reverse.interpolated_peak).abs()
                <= tolerance(expected)
        );
        assert_eq!(finished_reverse.sample_peak, 0.9);
    }
}
#[test]
fn l512_ring_wrap_with_dense_source_matches_absolute_oracle_across_partition_boundaries() {
    let d = make(512, Factor::Four, 12.);
    let n = 1200;
    assert!(n > d.taps());
    let end = ramped_alternating_end(n, 300);
    let x: Vec<f64> = (0..n)
        .map(|i| 0.43 * (i as f64 * 0.391).sin() + 0.17 * (i as f64 * 0.087).cos() + end[i])
        .collect();
    let (expected, _) = oracle_scan(&d, &x, 1024);
    let mut single = Meter::new(&d, 65536).unwrap();
    single.observe(&x).unwrap();
    single.finish();
    // Bounded split around the wrap: write index 1025 returns to slot zero.
    let mut split = Meter::new(&d, 1024).unwrap();
    split.observe(&x[..1024]).unwrap();
    split.observe(&x[1024..1025]).unwrap();
    split.observe(&x[1025..]).unwrap();
    split.finish();
    assert_eq!(split.reading(), single.reading());
    let r = single.reading();
    assert!(
        (r.interpolated_peak - expected).abs() <= 2e-14 * (1. + expected),
        "bank {} oracle {expected}",
        r.interpolated_peak
    );
    assert_eq!(r.source_frames, 1200);
    assert_eq!(r.tail_frames, 1024);
    assert_eq!(r.phase_observations, (1200 + 1024) * 4);
    assert_eq!(single.last_phase_numerator(), Some((1200 + 511) * 4 + 3));
}
#[test]
fn whole_bank_refusals_preserve_all64_readings_and_future_histories() {
    let d = make(32, Factor::Four, 12.);
    let mut bank = Bank::new(&d, 64, 5).unwrap();
    let mut oracle = Bank::new(&d, 64, 5).unwrap();
    let signal: Vec<f64> = (0..64).map(|i| i as f64 / 64. - 0.5).collect();
    bank.observe_frame(&signal).unwrap();
    oracle.observe_frame(&signal).unwrap();
    let snapshots: Vec<_> = (0..64).map(|i| bank.reading(i).unwrap()).collect();
    assert_eq!(bank.observe_frame(&signal[..63]), Err(E::ChannelCount));
    assert_eq!(bank.observe_frame_f32(&[0.; 65]), Err(E::ChannelCount));
    let full: [&[f64]; 64] = [&[0.; 5]; 64];
    assert_eq!(bank.observe_planar(&full, 6), Err(E::TooManyFrames));
    assert_eq!(bank.observe_planar(&full[..63], 1), Err(E::ChannelCount));
    let mut short = full;
    short[63] = &[];
    assert_eq!(bank.observe_planar(&short, 1), Err(E::BufferLength));
    let full32: [&[f32]; 64] = [&[0.; 5]; 64];
    let mut short32 = full32;
    short32[63] = &[];
    assert_eq!(bank.observe_planar_f32(&short32, 1), Err(E::BufferLength));
    let mut storage: Vec<Vec<f64>> = vec![vec![1.; 5]; 64];
    storage[63].clear();
    let slices: Vec<&mut [f64]> = storage.iter_mut().map(|s| s.as_mut_slice()).collect();
    assert_eq!(bank.observe_in_place(&slices, 1), Err(E::BufferLength));
    assert!(storage[..63].iter().all(|s| s == &[1.; 5]));
    let mut storage32: Vec<Vec<f32>> = vec![vec![1.; 5]; 64];
    storage32[63].clear();
    let slices32: Vec<&mut [f32]> = storage32.iter_mut().map(|s| s.as_mut_slice()).collect();
    assert_eq!(
        bank.observe_in_place_f32(&slices32, 1),
        Err(E::BufferLength)
    );
    assert!(storage32[..63].iter().all(|s| s == &[1.; 5]));
    for (i, snapshot) in snapshots.into_iter().enumerate() {
        assert_eq!(bank.reading(i).unwrap(), snapshot);
    }
    assert_eq!(bank.reading(64), Err(E::ChannelCount));
    for _ in 0..131 {
        bank.observe_frame(&signal).unwrap();
        oracle.observe_frame(&signal).unwrap();
    }
    bank.finish();
    oracle.finish();
    for i in 0..64 {
        assert_eq!(bank.reading(i).unwrap(), oracle.reading(i).unwrap());
    }
    for channels in [0, 65] {
        assert_eq!(Bank::new(&d, channels, 5).unwrap_err(), E::ChannelCount);
    }
    for max in [0, 65537] {
        assert_eq!(Bank::new(&d, 1, max).unwrap_err(), E::MaximumFrames);
    }
}
#[test]
fn native32_widens_exactly_with_planar_inplace_and64_independent_meters() {
    let d = make(32, Factor::Eight, 12.);
    let mut bank = Bank::new(&d, 64, 67).unwrap();
    let mut singles: Vec<Meter> = (0..64).map(|_| Meter::new(&d, 67).unwrap()).collect();
    for chunk in 0usize..5 {
        let mut storage: Vec<Vec<f32>> = (0..64)
            .map(|ch| {
                (0..67)
                    .map(|i| (0.3 * ((i + chunk * 67) as f64 * 0.13 + ch as f64).sin()) as f32)
                    .collect()
            })
            .collect();
        let before = storage.clone();
        if chunk.is_multiple_of(2) {
            let slices: Vec<&[f32]> = storage.iter().map(|s| s.as_slice()).collect();
            bank.observe_planar_f32(&slices, 67).unwrap();
        } else {
            let slices: Vec<&mut [f32]> = storage.iter_mut().map(|s| s.as_mut_slice()).collect();
            bank.observe_in_place_f32(&slices, 67).unwrap();
        }
        assert_eq!(storage, before);
        for (i, s) in singles.iter_mut().enumerate() {
            s.observe_f32(&storage[i]).unwrap();
            assert_eq!(bank.reading(i).unwrap(), s.reading());
        }
    }
    bank.finish();
    for (i, s) in singles.iter_mut().enumerate() {
        s.finish();
        assert_eq!(bank.reading(i).unwrap(), s.reading());
    }
}
#[test]
fn nonfinite_measurement_silence_audio_unchanged_zero_subnormal_and_true_overflow() {
    let d = make(32, Factor::Eight, 12.);
    let mut meter = Meter::new(&d, 67).unwrap();
    assert_eq!(meter.reading().db_tp(), f64::NEG_INFINITY);
    let input = [0., f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.1];
    meter.observe(&input).unwrap();
    assert!(input[1].is_nan());
    assert_eq!(input[2], f64::INFINITY);
    meter.finish();
    assert!(meter.reading().nonfinite_input);
    assert_eq!(meter.reading().sample_peak, 0.1);
    assert_eq!(meter.reading().source_frames, 5);
    let mut ordinary = Meter::new(&d, 67).unwrap();
    ordinary.observe(&[0., 0., 0., 0., 0.1]).unwrap();
    ordinary.finish();
    assert_eq!(
        ordinary.reading().interpolated_peak,
        meter.reading().interpolated_peak
    );
    meter.reset();
    meter.observe_sample(f64::from_bits(1)).unwrap();
    meter.finish();
    assert_eq!(meter.reading().sample_peak, f64::from_bits(1));
    assert_eq!(meter.reading().interpolated_peak, f64::from_bits(1));
    assert!(!meter.reading().amplitude_overflow);
    let row = d.phase_coefficients(4).unwrap();
    assert!(row.iter().map(|v| v.abs()).sum::<f64>() > 1.);
    let input: Vec<f64> = row
        .iter()
        .rev()
        .map(|h| if *h >= 0. { f64::MAX } else { -f64::MAX })
        .collect();
    meter.reset();
    meter.observe(&input).unwrap();
    meter.finish();
    assert!(meter.reading().amplitude_overflow);
    assert_eq!(meter.reading().interpolated_peak, f64::INFINITY);
    assert_eq!(meter.reading().db_tp(), f64::INFINITY);
    assert!(!meter.reading().nonfinite_input);
}
#[derive(Clone, Copy, Debug)]
enum Hazard {
    Nan,
    PositiveInfinity,
    NegativeInfinity,
    Overflow,
}
fn neighbour_stream(channel: usize) -> Vec<f64> {
    (0..80)
        .map(|i| 0.3 * (i as f64 * 0.37 + channel as f64 * 0.5).sin() + channel as f64 / 1260.)
        .collect()
}
fn hazard_stream(hazard: Hazard, channel: usize, overflow_pattern: &[f64]) -> Vec<f64> {
    let mut stream = neighbour_stream(channel);
    match hazard {
        Hazard::Nan => {
            stream[7] = f64::NAN;
            stream[40] = f64::NAN;
        }
        Hazard::PositiveInfinity => {
            stream[7] = f64::INFINITY;
            stream[40] = f64::INFINITY;
        }
        Hazard::NegativeInfinity => {
            stream[7] = f64::NEG_INFINITY;
            stream[40] = f64::NEG_INFINITY;
        }
        Hazard::Overflow => {
            stream.fill(0.);
            stream[..overflow_pattern.len()].copy_from_slice(overflow_pattern);
        }
    }
    stream
}
fn feed_bank(bank: &mut Bank, streams: &[Vec<f64>]) {
    let frames = streams[0].len();
    let mut done = 0;
    while done < frames {
        let n = 33.min(frames - done);
        let views: Vec<&[f64]> = streams.iter().map(|s| &s[done..done + n]).collect();
        bank.observe_planar(&views, n).unwrap();
        done += n;
    }
}
fn finished_meter(d: &Design, stream: &[f64]) -> Meter {
    let mut meter = Meter::new(d, 33).unwrap();
    for chunk in stream.chunks(33) {
        meter.observe(chunk).unwrap();
    }
    meter.finish();
    meter
}
fn bits(streams: &[Vec<f64>]) -> Vec<Vec<u64>> {
    streams
        .iter()
        .map(|s| s.iter().map(|v| v.to_bits()).collect())
        .collect()
}
#[test]
fn bank_channel_isolation_for_nonfinite_and_true_overflow_at_first_middle_last_positions() {
    let d = make(32, Factor::Eight, 12.);
    let row = d.phase_coefficients(4).unwrap();
    assert!(row.iter().map(|v| v.abs()).sum::<f64>() > 1.);
    let pattern: Vec<f64> = row
        .iter()
        .rev()
        .map(|h| if *h >= 0. { f64::MAX } else { -f64::MAX })
        .collect();
    let baseline_streams: Vec<Vec<f64>> = (0..64).map(neighbour_stream).collect();
    let mut baseline = Bank::new(&d, 64, 33).unwrap();
    feed_bank(&mut baseline, &baseline_streams);
    baseline.finish();
    for (c, baseline_channel) in baseline_streams.iter().enumerate() {
        let r = baseline.reading(c).unwrap();
        assert!(!r.nonfinite_input && !r.amplitude_overflow && r.interpolated_peak.is_finite());
        assert_eq!(r, finished_meter(&d, baseline_channel).reading());
    }
    for position in [0, 31, 63] {
        for hazard in [
            Hazard::Nan,
            Hazard::PositiveInfinity,
            Hazard::NegativeInfinity,
            Hazard::Overflow,
        ] {
            let mut streams = baseline_streams.clone();
            streams[position] = hazard_stream(hazard, position, &pattern);
            let audio_before = bits(&streams);
            let mut bank = Bank::new(&d, 64, 33).unwrap();
            feed_bank(&mut bank, &streams);
            assert_eq!(bits(&streams), audio_before, "observation edited audio");
            bank.finish();
            for (c, channel) in streams.iter().enumerate() {
                let r = bank.reading(c).unwrap();
                // Every channel equals an independent single-channel meter fed
                // the same stream, flags included.
                assert_eq!(
                    r,
                    finished_meter(&d, channel).reading(),
                    "{hazard:?}@{position} ch{c}"
                );
                if c != position {
                    // Finite neighbours match the all-finite baseline bank, which
                    // also covers their histories through the 64-step tail replay.
                    assert_eq!(
                        r,
                        baseline.reading(c).unwrap(),
                        "{hazard:?}@{position} ch{c}"
                    );
                    assert!(!r.nonfinite_input && !r.amplitude_overflow);
                }
            }
            let r = bank.reading(position).unwrap();
            match hazard {
                Hazard::Nan | Hazard::PositiveInfinity | Hazard::NegativeInfinity => {
                    // Substituted samples: measurement silence plus a latched
                    // flag, never an arithmetic-overflow flag.
                    assert!(r.nonfinite_input && !r.amplitude_overflow);
                    let silent: Vec<f64> = streams[position]
                        .iter()
                        .map(|v| if v.is_finite() { *v } else { 0. })
                        .collect();
                    let mut expected = finished_meter(&d, &silent).reading();
                    assert!(!expected.nonfinite_input);
                    expected.nonfinite_input = true;
                    assert_eq!(r, expected);
                }
                Hazard::Overflow => {
                    // Finite samples whose final reconstruction exceeds f64::MAX.
                    assert!(r.amplitude_overflow && !r.nonfinite_input);
                    assert_eq!(r.interpolated_peak, f64::INFINITY);
                    assert_eq!(r.sample_peak, f64::MAX);
                    assert_eq!(r.db_tp(), f64::INFINITY);
                }
            }
        }
    }
    // First, middle and last positions poisoned in one bank at once.
    let mut streams = baseline_streams.clone();
    streams[0] = hazard_stream(Hazard::Nan, 0, &pattern);
    streams[31] = hazard_stream(Hazard::Overflow, 31, &pattern);
    streams[63] = hazard_stream(Hazard::PositiveInfinity, 63, &pattern);
    let mut bank = Bank::new(&d, 64, 33).unwrap();
    feed_bank(&mut bank, &streams);
    bank.finish();
    for (c, channel) in streams.iter().enumerate() {
        let r = bank.reading(c).unwrap();
        assert_eq!(r.nonfinite_input, c == 0 || c == 63, "ch{c}");
        assert_eq!(r.amplitude_overflow, c == 31, "ch{c}");
        assert_eq!(r, finished_meter(&d, channel).reading());
        if ![0, 31, 63].contains(&c) {
            assert_eq!(r, baseline.reading(c).unwrap());
        }
    }
}
#[test]
fn observation_reset_and_complete_tail_allocate_and_deallocate_zero() {
    let d = make(32, Factor::Eight, 12.);
    let mut bank = Bank::new(&d, 3, 67).unwrap();
    let input = [0.2; 67];
    let channels: [&[f64]; 3] = [&input; 3];
    let allocations = allocation_counter::measure(|| {
        for _ in 0..10 {
            bank.observe_planar(&channels, 67).unwrap();
            bank.observe_planar(&channels, 0).unwrap();
        }
        bank.finish();
        bank.finish();
        bank.reset();
        bank.observe_frame(&[1., 0., -1.]).unwrap();
        bank.finish();
    });
    assert_eq!(allocations.count_total, 0);
    assert_eq!(allocations.count_current, 0);
    assert_eq!(allocations.bytes_total, 0);
}
#[test]
fn quarter_rate_between_sample_peaks_and_profile_error_receipts() {
    // Independent derivation. The source is a unit-amplitude flat-envelope
    // cos(pi/2*tau + phase) with phase = pi/4 + pi/(4F), so its continuous maxima
    // (value 1) sit at tau = 4n - 1/2 - 1/(2F): exactly half a grid step (1/(2F))
    // from the two nearest coordinates j/F. An ideal sinc interpolator evaluated
    // there gives cos(pi/2 * 1/(2F)) = cos(pi/(4F)); that is -0.1685/-0.0419/
    // -0.0105 dB for F=4/8/16. The window is finite, so the assertion allows
    // finite_window_tolerance(half, beta) (a-priori leakage bound, see above)
    // instead of claiming an exact ideal-sinc identity, and the flat envelope
    // (length 8L+2048, ramps 2L) keeps all reconstruction windows at full amplitude.
    for half in [32, 128, 512] {
        for beta in [0., 6., 12., 16.] {
            let mut grid_peaks = Vec::new();
            for factor in FACTORS {
                let d = make(half, factor, beta);
                let mut meter = Meter::new(&d, 65536).unwrap();
                let length = 8 * half + 2048;
                let phase = std::f64::consts::FRAC_PI_4
                    + std::f64::consts::PI / (4. * factor.factor() as f64);
                let input = quarter_rate_source(half, length, phase);
                meter.observe(&input).unwrap();
                meter.finish();
                let r = meter.reading();
                assert!(r.interpolated_peak > r.sample_peak + 0.02);
                assert!((r.interpolated_peak - 1.).abs() < 0.05);
                let expected = (std::f64::consts::PI / (4. * factor.factor() as f64)).cos();
                let tolerance = finite_window_tolerance(half, beta);
                assert!(
                    (r.interpolated_peak - expected).abs() <= tolerance,
                    "F{} L{half} beta{beta}: measured {:0.17e} analytic cos(pi/4F) {expected:0.17e} tolerance {tolerance:e}",
                    factor.factor(),
                    r.interpolated_peak
                );
                if beta >= 12. {
                    // Grid quantization is visible, not merely bounded below 1.
                    assert!(1. - r.interpolated_peak > 0.5 * (1. - expected));
                }
                grid_peaks.push(r.interpolated_peak);
                println!(
                    "TRUE-PEAK quarter-rate profile factor={} half={half} beta={beta} sample_peak={:0.17e} estimated={:0.17e} analytic_cos_pi_over_4F={expected:0.17e} error_db={:0.17e}; selected finite tapered waveform, no universal continuous-maximum/conformance claim",
                    factor.factor(),
                    r.sample_peak,
                    r.interpolated_peak,
                    r.db_tp()
                );
            }
            if beta >= 12. {
                assert!(grid_peaks[0] < grid_peaks[1] && grid_peaks[1] < grid_peaks[2]);
            }
        }
    }
}
#[test]
fn off_grid_quarter_rate_peak_follows_independent_distance_to_nearest_grid_point() {
    // Peak position q = 1/2 + s/F samples (phase = pi/2 * q) puts the continuous
    // maximum s grid steps from a coordinate j/F because F is even, so
    // F*q = F/2 + s. For s in {0, 1/4} the nearest grid point is s steps away and
    // an ideal sinc reads cos(pi/2 * s/F): 1 for s=0 and cos(pi/(8F)) for s=1/4
    // (the s=1/2 case is the quarter-rate test above). Strong windows only
    // (beta >= 12), where the finite-window allowance 1e-4 is below half the
    // F=4/8/16 differences (3.6e-3 / 9e-4 for s=1/4).
    for half in [32, 128] {
        for beta in [12., 16.] {
            let mut offgrid = Vec::new();
            for factor in FACTORS {
                for s in [0., 0.25] {
                    let d = make(half, factor, beta);
                    let f = factor.factor() as f64;
                    let length = 8 * half + 2048;
                    let phase = std::f64::consts::FRAC_PI_2 * (0.5 + s / f);
                    let input = quarter_rate_source(half, length, phase);
                    let mut meter = Meter::new(&d, 65536).unwrap();
                    meter.observe(&input).unwrap();
                    meter.finish();
                    let r = meter.reading();
                    let expected = (std::f64::consts::FRAC_PI_2 * s / f).cos();
                    assert!(
                        (r.interpolated_peak - expected).abs()
                            <= finite_window_tolerance(half, beta),
                        "F{f} L{half} beta{beta} s{s}: measured {:0.17e} expected {expected:0.17e}",
                        r.interpolated_peak
                    );
                    assert!(r.interpolated_peak > r.sample_peak + 0.1);
                    if s == 0.25 {
                        offgrid.push(r.interpolated_peak);
                    }
                }
            }
            assert!(offgrid[0] < offgrid[1] && offgrid[1] < offgrid[2]);
        }
    }
}

#[test]
fn independent_decimal90_taylor_bessel_coefficients_qualify_actual_prepared_rows() {
    let mut maxima = 0_f64;
    let mut count = 0;
    let mut last_profile = None;
    let mut design = None;
    for row in include_str!("true-peak-decimal.csv").lines() {
        let v: Vec<f64> = row.split(',').map(|s| s.parse().unwrap()).collect();
        let profile = (v[0] as usize, v[1] as usize, v[2] as u8);
        if last_profile != Some(profile) {
            let factor = match profile.1 {
                4 => Factor::Four,
                8 => Factor::Eight,
                16 => Factor::Sixteen,
                _ => unreachable!(),
            };
            design = Some(make(profile.0, factor, profile.2 as f64));
            last_profile = Some(profile);
        }
        let value = design
            .as_ref()
            .unwrap()
            .phase_coefficients(v[3] as usize)
            .unwrap()[v[4] as usize];
        let error = (value - v[5]).abs();
        maxima = maxima.max(error);
        assert!(
            error < 4e-15,
            "profile{profile:?} phase{} tap{} actual{value:e} expected{:e} error{error:e}",
            v[3],
            v[4],
            v[5]
        );
        count += 1;
    }
    assert_eq!(count, 1276);
    println!(
        "COEFFICIENT Decimal90 Taylor/Bessel samples={count}, max absolute difference={maxima:e}; independent pi/sin/I0/normalization, selected native f64 qualification"
    );
}
#[test]
fn narrow_high_frequency_wavepacket_preserves_profile_accuracy_failures() {
    // Every analytic cosine and the continuous taper are bounded by one; their
    // positive average reaches exactly one at the flat-envelope half-sample
    // center. Thus the reference continuous maximum is independently one.
    // This finite selected waveform is not a universal bandwidth certificate.
    // The half-sample center lies on every 4/8/16 grid, so this fixture does not
    // by itself demonstrate grid quantization (see the quarter-rate tests). It has
    // 36 profile combinations; how many distinct sampled peaks they produce is
    // printed below for information only and is deliberately not asserted.
    let mut entries = Vec::new();
    let mut distinct_peaks = BTreeSet::new();
    for factor in FACTORS {
        for half in [32, 128, 512] {
            for beta in [0., 6., 12., 16.] {
                let d = make(half, factor, beta);
                let n = 4 * half + 512;
                let center = n as f64 * 0.5 + 0.5;
                let source: Vec<f64> = (0..n)
                    .map(|i| {
                        let taper = 0.5
                            - 0.5
                                * (std::f64::consts::PI
                                    * (i.min(n - 1 - i) as f64 / (2 * half) as f64).min(1.))
                                .cos();
                        taper
                            * (0..16)
                                .map(|k| {
                                    (std::f64::consts::TAU
                                        * (0.46 + (k as f64 + 0.5) * 0.03 / 16.)
                                        * (i as f64 - center))
                                        .cos()
                                })
                                .sum::<f64>()
                            / 16.
                    })
                    .collect();
                let mut meter = Meter::new(&d, 65536).unwrap();
                meter.observe(&source).unwrap();
                meter.finish();
                let r = meter.reading();
                let error = r.db_tp();
                entries.push((factor.factor(), half, beta, error));
                distinct_peaks.insert(r.interpolated_peak.to_bits());
                assert!(r.interpolated_peak >= r.sample_peak);
                assert!(r.interpolated_peak.is_finite());
                if half == 512 && beta == 12. {
                    assert!(error.abs() < 0.01, "long selected profile error{error:e}");
                }
                println!(
                    "HIGH-BAND analytic wavepacket factor={} half={half} beta={beta} sample_peak={:0.17e} estimated={:0.17e} ideal_continuous_peak=1 error_db={error:0.17e}; EBU-style tolerance failure={} retained, no conformance claim",
                    factor.factor(),
                    r.sample_peak,
                    r.interpolated_peak,
                    !(-0.4..=0.2).contains(&error)
                );
            }
        }
    }
    // Requested observation interval is unchanged (+0.2/-0.4 dB): failures are
    // evidence and are neither waived nor widened. Regression receipt from the
    // recorded qualification: exactly the twelve half-width-32 profiles fail,
    // three over-read at beta 0 (about +0.2952 dB) and nine under-read at beta 6,
    // 12 and 16 (-0.769..-1.190 dB); the worst is -1.189524690758779 dB. Only
    // the worst value is pinned at full precision (1e-9 dB, far above libm
    // noise ~1e-14 dB); the others use their recorded 3-4 decimal ranges.
    assert_eq!(entries.len(), 36);
    let mut tolerance_failures = 0;
    let (mut worst, mut best) = (0_f64, 0_f64);
    let mut worst_half = 0;
    for &(factor, half, beta, error) in &entries {
        let outside = !(-0.4..=0.2).contains(&error);
        tolerance_failures += usize::from(outside);
        assert_eq!(
            outside,
            half == 32,
            "F{factor} L{half} beta{beta}: {error:e}"
        );
        if error < worst {
            (worst, worst_half) = (error, half);
        }
        best = best.max(error);
        if half == 32 && beta == 0. {
            assert!(
                (0.2951..=0.2953).contains(&error),
                "beta0 over-reading F{factor}: {error:e}"
            );
        } else if half == 32 {
            assert!(beta >= 6.);
            assert!(
                (-1.1896..=-0.7685).contains(&error),
                "beta>=6 under-reading F{factor} beta{beta}: {error:e}"
            );
        }
    }
    assert_eq!(tolerance_failures, 12);
    assert_eq!(worst_half, 32);
    assert!(
        (worst - (-1.189524690758779)).abs() < 1e-9,
        "worst signed error {worst:0.17e}"
    );
    assert!(
        (0.2951..=0.2953).contains(&best),
        "largest over-reading {best:e}"
    );
    println!(
        "HIGH-BAND selected profile tolerance failures={tolerance_failures}/36 worst_error_db={worst:e} largest_over_reading_db={best:e}; explicit finite-FIR limits, not repaired/waived by changing reference; distinct_sampled_peaks={} (informational only, not asserted)",
        distinct_peaks.len()
    );
}
