use ravet_signal::{
    Biquad, BiquadCoefficients, KWeightingDesign as Design, LoudnessChannelRole as Role,
    LoudnessError as Error, LoudnessRecordCapacities as Capacities, LoudnessTailPolicy as Tail,
    PreparedLoudnessMeter64 as Meter,
};
fn capacities(integrated: usize, lra: usize) -> Capacities {
    Capacities { integrated, lra }
}
fn meter(rate: u32, roles: &[Role], maximum: usize) -> Meter {
    Meter::new(
        &Design::new(rate).unwrap(),
        roles,
        maximum,
        capacities(1024, 1024),
        true,
    )
    .unwrap()
}
// Direct-form I using independently retained input/output values, not production
// transposed-state recurrence. Coefficients have a separate Decimal oracle.
fn filtered(d: Design, input: &[f64]) -> Vec<f64> {
    let mut result = input.to_vec();
    for stage in 0..2 {
        let (b, a) = d.section(stage).unwrap();
        let mut x = [0.; 2];
        let mut y = [0.; 2];
        for sample in &mut result {
            let source = if sample.is_finite() { *sample } else { 0. };
            let value = b[0] * source + b[1] * x[0] + b[2] * x[1] - a[0] * y[0] - a[1] * y[1];
            x = [source, x[0]];
            y = [value, y[0]];
            *sample = value;
        }
    }
    result
}
fn endpoint(k: u64, rate: u32) -> usize {
    ((k as u128 * rate as u128) / 10) as usize
}
fn offline_records(d: Design, roles: &[Role], input: &[Vec<f64>], count: u64) -> Vec<f64> {
    let source: Vec<_> = input.iter().map(|v| filtered(d, v)).collect();
    let mut result = Vec::new();
    for k in count.. {
        let end = endpoint(k, d.sample_rate());
        if end > input[0].len() {
            break;
        }
        let start = endpoint(k - count, d.sample_rate());
        let mut energy = 0.;
        for (values, role) in source.iter().zip(roles) {
            energy += role.power_weight() * values[start..end].iter().map(|v| v * v).sum::<f64>();
        }
        result.push(-0.691 + 10. * (energy / (end - start) as f64).log10());
    }
    result
}
fn signal(rate: u32, n: usize, phase: f64) -> Vec<f64> {
    (0..n)
        .map(|i| {
            0.37 * (std::f64::consts::TAU * 997. * i as f64 / rate as f64 + phase).sin()
                + 0.11 * (std::f64::consts::TAU * 137. * i as f64 / rate as f64).cos()
        })
        .collect()
}
fn close_records(actual: &[f64], expected: &[f64], tolerance: f64) -> f64 {
    assert_eq!(actual.len(), expected.len());
    let mut maximum = 0_f64;
    for (a, e) in actual.iter().zip(expected) {
        if e.is_infinite() {
            assert_eq!(a, e);
            continue;
        }
        let error = (a - e).abs();
        maximum = maximum.max(error);
        assert!(
            error < tolerance,
            "actual={a:.17e} expected={e:.17e} error={error:.17e}"
        );
    }
    maximum
}
#[test]
fn all_integer_rates_strict_admission_actual_metadata_and_physical_response() {
    let mut margin = f64::INFINITY;
    for rate in 8000..=768000 {
        let d = Design::new(rate).unwrap();
        assert_eq!(d.sample_rate(), rate);
        for stage in 0..2 {
            let (b, a) = d.section(stage).unwrap();
            assert!(b.iter().chain(&a).all(|v| v.is_finite()));
            let actual = [1. - a[1].abs(), 1. + a[0] + a[1], 1. - a[0] + a[1]];
            assert!(actual.iter().all(|v| *v > 0.));
            margin = margin.min(actual.into_iter().fold(f64::INFINITY, f64::min));
        }
    }
    for rate in [8000, 16000, 22050, 44100, 48000, 96000, 192000, 768000] {
        let d = Design::new(rate).unwrap();
        assert_eq!(d.magnitude_at(0.).unwrap(), 0.);
        assert!(d.magnitude_at(rate as f64 / 2.).unwrap().is_finite());
        let expected = -0.691 + 20. * d.magnitude_at(997.).unwrap().log10() - 10. * 2_f64.log10();
        println!(
            "CALIBRATION rate={rate} actual997Hz amplitude-one steady sine LUFS={expected:.17e}; no rate-dependent gain repair"
        );
        assert!(d.section(2).is_none());
        for invalid in [
            f64::NAN,
            f64::INFINITY,
            -f64::MIN_POSITIVE,
            (rate as f64 / 2.).next_up(),
        ] {
            assert_eq!(d.magnitude_at(invalid), Err(Error::Coefficient));
        }
    }
    for rate in [0, 7999, 768001, u32::MAX] {
        assert!(matches!(Design::new(rate), Err(Error::SampleRate)));
    }
    println!("ADMISSION all760001 integer rates rounded minimum Jury margin={margin:.17e}");
}
#[test]
fn actual_biquad_delay_memories_are_observable_without_recurrence_changes() {
    let c = BiquadCoefficients::new([0.25, -0.5, 0.125], [-0.2, 0.3]).unwrap();
    let mut f = Biquad::new(c);
    let mut expected = [0_f64; 2];
    for x in [1., -0.2, 0., 0.9] {
        let y = 0.25 * x + expected[0];
        expected = [-0.5 * x + 0.2 * y + expected[1], 0.125 * x - 0.3 * y];
        assert_eq!(f.process_sample(x), y);
        assert_eq!(f.delay_state(), expected);
    }
    f.reset();
    assert_eq!(f.delay_state(), [0.; 2]);
    let c = BiquadCoefficients::new([1., 2., 0.], [0., 0.]).unwrap();
    let mut f = Biquad::new(c);
    assert_eq!(f.process_sample(f64::MAX), f64::MAX);
    assert!(
        f.delay_state()[0].is_infinite(),
        "finite output is not a valid-state proof"
    );
}
#[test]
fn all_ten_grid_remainders_independent_absolute_windows_and_partition_identity() {
    let roles = [Role::Front, Role::Surround, Role::Excluded];
    let mut worst = 0_f64;
    for rate in (8000..8010).chain([22051, 44101, 48007]) {
        let d = Design::new(rate).unwrap();
        let n = 3 * rate as usize + rate as usize / 3 + 19;
        let source = [signal(rate, n, 0.), signal(rate, n, 0.4), vec![9.; n]];
        let mut whole = Meter::new(&d, &roles, 65536, capacities(1024, 1024), true).unwrap();
        // One complete call where allowed; larger profiles use the same full max.
        for offset in (0..n).step_by(65536) {
            let end = (offset + 65536).min(n);
            whole
                .observe_planar(&source.each_ref().map(|v| &v[offset..end]), end - offset)
                .unwrap();
        }
        let mut parts = meter(rate, &roles, 67);
        let mut offset = 0;
        for chunk in [0, 1, 67, 2, 31, 5].into_iter().cycle() {
            if offset == n {
                break;
            }
            let take = chunk.min(n - offset);
            parts
                .observe_planar(&source.each_ref().map(|v| &v[offset..offset + take]), take)
                .unwrap();
            offset += take;
        }
        assert_eq!(whole.reading(), parts.reading());
        assert_eq!(whole.integrated_records(), parts.integrated_records());
        assert_eq!(whole.lra_records(), parts.lra_records());
        worst = worst.max(close_records(
            whole.integrated_records(),
            &offline_records(d, &roles, &source, 4),
            2e-9,
        ));
        worst = worst.max(close_records(
            whole.lra_records(),
            &offline_records(d, &roles, &source, 30),
            2e-9,
        ));
        let r = whole.reading();
        assert_eq!(r.source_frames, n as u64);
        let k = (n as u128 * 10 + 9) / rate as u128;
        assert_eq!(
            r.short_term_endpoint_frame,
            Some(endpoint(k as u64, rate) as u64)
        );
        assert_eq!(
            whole.analyse_programme().unwrap(),
            parts.analyse_programme().unwrap()
        );
        let before = whole.reading();
        whole.observe_planar(&[&[], &[], &[]], 0).unwrap();
        assert_eq!(before, whole.reading());
        whole.finish(Tail::None).unwrap();
        assert_eq!(whole.reading().tail_frames, 0);
        assert_eq!(whole.observe_frame(&[0.; 3]), Err(Error::Finalized));
        assert_eq!(
            whole.observe_planar(&[&[], &[], &[]], 0),
            Err(Error::Finalized)
        );
    }
    println!(
        "WINDOW direct-form-I absolute source oracle all10 remainders; max LUFS discrepancy={worst:.17e}"
    );
}
#[test]
fn all_shape_prefix_channel_maximum_refusals_preserve64_future_histories() {
    let roles = [Role::Front; 64];
    let mut m = meter(8003, &roles, 5);
    let mut oracle = meter(8003, &roles, 5);
    let frame: Vec<f64> = (0..64).map(|i| i as f64 / 64. - 0.5).collect();
    m.observe_frame(&frame).unwrap();
    oracle.observe_frame(&frame).unwrap();
    let before = m.reading();
    assert_eq!(m.observe_frame(&frame[..63]), Err(Error::ChannelCount));
    assert_eq!(m.observe_frame_f32(&[0.; 65]), Err(Error::ChannelCount));
    let full: [&[f64]; 64] = [&[0.; 5]; 64];
    assert_eq!(m.observe_planar(&full, 6), Err(Error::TooManyFrames));
    assert_eq!(m.observe_planar(&full[..63], 1), Err(Error::ChannelCount));
    let mut short = full;
    short[63] = &[];
    assert_eq!(m.observe_planar(&short, 1), Err(Error::BufferLength));
    let full32: [&[f32]; 64] = [&[0.; 5]; 64];
    let mut short32 = full32;
    short32[63] = &[];
    assert_eq!(m.observe_planar_f32(&short32, 1), Err(Error::BufferLength));
    let mut storage = vec![vec![0.7; 5]; 64];
    storage[63].clear();
    assert_eq!(
        m.observe_in_place(
            &storage
                .iter_mut()
                .map(|v| v.as_mut_slice())
                .collect::<Vec<_>>(),
            1
        ),
        Err(Error::BufferLength)
    );
    assert!(storage[..63].iter().all(|v| v == &[0.7; 5]));
    let mut storage32 = vec![vec![0.7_f32; 5]; 64];
    storage32[63].clear();
    assert_eq!(
        m.observe_in_place_f32(
            &storage32
                .iter_mut()
                .map(|v| v.as_mut_slice())
                .collect::<Vec<_>>(),
            1
        ),
        Err(Error::BufferLength)
    );
    assert!(storage32[..63].iter().all(|v| v == &[0.7; 5]));
    assert_eq!(m.reading(), before);
    for _ in 0..3300 {
        m.observe_frame(&frame).unwrap();
        oracle.observe_frame(&frame).unwrap();
    }
    assert_eq!(m.reading(), oracle.reading());
    assert_eq!(m.integrated_records(), oracle.integrated_records());
    m.reset();
    oracle.reset();
    assert_eq!(m.reading(), oracle.reading());
    let d = Design::new(8000).unwrap();
    for channels in [0, 65] {
        assert!(matches!(
            Meter::new(&d, &vec![Role::Front; channels], 1, capacities(1, 1), true),
            Err(Error::ChannelCount)
        ));
    }
    for maximum in [0, 65537] {
        assert!(matches!(
            Meter::new(&d, &[Role::Front], maximum, capacities(1, 1), true),
            Err(Error::MaximumFrames)
        ));
    }
    for capacity in [0, 1_000_001, usize::MAX] {
        assert!(matches!(
            Meter::new(&d, &[Role::Front], 1, capacities(capacity, 1), true),
            Err(Error::RecordCapacity)
        ));
    }
}
#[test]
fn standby_retains_records_live_history_resume_does_not_splice_partial_windows() {
    let rate = 8001;
    let q = rate as usize / 10;
    let mut m = meter(rate, &[Role::Front], 65536);
    let source = signal(rate, q * 7 + 3, 0.1);
    m.observe_planar(&[&source], source.len()).unwrap();
    let records = m.integrated_records().to_vec();
    let admitted = m.reading().programme_source_frames;
    m.set_programme_running(false).unwrap();
    let paused = m.reading();
    m.set_programme_running(false).unwrap();
    assert_eq!(m.reading(), paused);
    assert_eq!(m.finish(Tail::LraSilence1500ms), Err(Error::TailInStandby));
    assert_eq!(m.reading(), paused);
    let audio = signal(rate, q * 32, 0.7);
    m.observe_planar(&[&audio], audio.len()).unwrap();
    assert_eq!(m.integrated_records(), records);
    assert_eq!(m.reading().programme_source_frames, admitted);
    assert!(m.reading().short_term_lufs.is_some());
    m.set_programme_running(true).unwrap();
    let running = m.reading();
    m.set_programme_running(true).unwrap();
    assert_eq!(running, m.reading());
    m.observe_planar(&[&source[..q * 3]], q * 3).unwrap();
    assert_eq!(m.integrated_records(), records);
    m.observe_planar(&[&source[..q + 1]], q + 1).unwrap();
    assert_eq!(m.integrated_records().len(), records.len() + 1);
    m.finish(Tail::None).unwrap();
    let final_reading = m.reading();
    m.finish(Tail::LraSilence1500ms).unwrap();
    assert_eq!(m.reading(), final_reading);
    assert_eq!(m.set_programme_running(false), Err(Error::Finalized));
    m.reset();
    assert_eq!(m.reading().source_frames, 0);
    assert!(m.reading().running);
    assert_eq!(m.reading().momentary_lufs, None);
    m.set_programme_running(false).unwrap();
    m.reset();
    assert!(!m.reading().running);
}
#[test]
fn nonfinite_audio_untouched_silence_measurement_numeric_poison_progress_and_reset() {
    let roles = [Role::Front, Role::Excluded];
    let mut m = meter(8000, &roles, 65536);
    let mut oracle = meter(8000, &roles, 65536);
    let mut source = [vec![0.2; 3300], vec![0.7; 3300]];
    source[0][3298] = f64::NAN;
    source[1][3299] = f64::INFINITY;
    let mut finite = source.clone();
    finite[0][3298] = 0.;
    finite[1][3299] = 0.;
    let [a, b] = &mut source;
    m.observe_in_place(&[a.as_mut_slice(), b.as_mut_slice()], 3300)
        .unwrap();
    oracle
        .observe_planar(&finite.each_ref().map(|v| v.as_slice()), 3300)
        .unwrap();
    assert!(source[0][3298].is_nan());
    assert!(source[1][3299].is_infinite());
    assert!(m.reading().nonfinite_input);
    assert_eq!(m.integrated_records(), oracle.integrated_records());
    let mut fault = meter(8000, &roles, 5);
    fault
        .observe_planar(&[&[f64::MAX; 5], &[0.; 5]], 5)
        .unwrap();
    let r = fault.reading();
    assert_eq!(r.source_frames, 5);
    assert_eq!(r.programme_source_frames, 5);
    let first = r.numeric_fault.unwrap();
    assert_eq!(first.coordinate, 0);
    assert_eq!(first.channel, 0);
    assert!(!first.artificial_tail);
    assert_eq!(r.momentary_lufs, None);
    assert_eq!(fault.analyse_programme(), Err(Error::NumericFault));
    fault.observe_frame(&[1., 2.]).unwrap();
    assert_eq!(fault.reading().numeric_fault, Some(first));
    assert_eq!(fault.reading().source_frames, 6);
    fault.reset();
    assert!(fault.reading().numeric_fault.is_none());
    fault.observe_frame(&[0.2, 1.]).unwrap();
    assert!(fault.reading().numeric_fault.is_none());
}
#[test]
fn exact_odd_rate_tail16_observations_refusal_preserves_prefix_then_none_retry() {
    for remainder in 0..10 {
        let rate = 8000 + remainder;
        let d = Design::new(rate).unwrap();
        let n = 3 * rate as usize - 1;
        let source = signal(rate, n, 0.3);
        let mut success = Meter::new(&d, &[Role::Front], 65536, capacities(32, 16), true).unwrap();
        success.observe_planar(&[&source], n).unwrap();
        let before = success.reading();
        let i = success.integrated_records().to_vec();
        assert_eq!(before.integrated_records, 26);
        assert_eq!(before.lra_records, 0);
        let tail = (3 * rate as u64).div_ceil(2);
        let completed = ((n as u128 + tail as u128 + 1) * 10 - 1) / rate as u128;
        let expected = (completed - 29) as usize;
        assert!((15..=16).contains(&expected));
        success.finish(Tail::LraSilence1500ms).unwrap();
        let r = success.reading();
        assert_eq!(r.source_frames, n as u64);
        assert_eq!(r.programme_source_frames, n as u64);
        assert_eq!(r.tail_frames, tail);
        assert_eq!(r.lra_records, expected);
        assert_eq!(success.integrated_records(), i);
        let mut padded = source.clone();
        padded.resize(n + tail as usize, 0.);
        close_records(
            success.lra_records(),
            &offline_records(d, &[Role::Front], &[padded], 30),
            2e-9,
        );
        success.finish(Tail::None).unwrap();
        assert_eq!(success.reading(), r);
        if rate == 8001 {
            assert_eq!(expected, 16);
            let mut refused =
                Meter::new(&d, &[Role::Front], 65536, capacities(32, 15), true).unwrap();
            refused.observe_planar(&[&source], n).unwrap();
            let before = refused.reading();
            let i = refused.integrated_records().to_vec();
            assert_eq!(
                refused.finish(Tail::LraSilence1500ms),
                Err(Error::RecordLimit)
            );
            assert_eq!(refused.reading(), before);
            assert_eq!(refused.integrated_records(), i);
            assert_eq!(
                refused.finish(Tail::LraSilence1500ms),
                Err(Error::RecordLimit)
            );
            assert_eq!(refused.reading(), before);
            // Fixed capacities cannot silently grow; a deliberate no-tail retry is valid.
            refused.finish(Tail::None).unwrap();
            assert_eq!(refused.reading().tail_frames, 0);
            assert_eq!(refused.integrated_records(), i);
        }
        println!(
            "TAIL rate={rate} original={n} ceil1500ms={tail} exact new LRA={expected} source unchanged; independent full padded-source oracle"
        );
    }
}
#[test]
fn record_capacity_whole_call_failure_preserves_actual_counts_and_future_filter_history() {
    let d = Design::new(8001).unwrap();
    let roles = [Role::Front, Role::Surround];
    let mut m = Meter::new(&d, &roles, 65536, capacities(1, 1024), true).unwrap();
    let mut oracle = Meter::new(&d, &roles, 65536, capacities(1, 1024), true).unwrap();
    let n = endpoint(4, 8001);
    let source = [signal(8001, n + 900, 0.3), signal(8001, n + 900, 0.8)];
    let views = source.each_ref().map(|v| &v[..n]);
    m.observe_planar(&views, n).unwrap();
    oracle.observe_planar(&views, n).unwrap();
    let before = m.reading();
    let records = m.integrated_records().to_vec();
    assert_eq!(
        m.observe_planar(&source.each_ref().map(|v| &v[n..]), 900),
        Err(Error::RecordLimit)
    );
    assert_eq!(m.reading(), before);
    assert_eq!(m.integrated_records(), records);
    let allowed = endpoint(5, 8001) - n - 1;
    let views = source.each_ref().map(|v| &v[n..n + allowed]);
    m.observe_planar(&views, allowed).unwrap();
    oracle.observe_planar(&views, allowed).unwrap();
    assert_eq!(m.reading(), oracle.reading());
    assert_eq!(m.integrated_records(), oracle.integrated_records());
    m.set_programme_running(false).unwrap();
    oracle.set_programme_running(false).unwrap();
    for _ in 0..3500 {
        m.observe_frame(&[0.2, -0.4]).unwrap();
        oracle.observe_frame(&[0.2, -0.4]).unwrap();
    }
    assert_eq!(m.reading(), oracle.reading());
    for invalid in [0, 1_000_001, usize::MAX] {
        assert!(matches!(
            Meter::new(&d, &roles, 5, capacities(1, invalid), true),
            Err(Error::RecordCapacity)
        ));
    }
}
#[test]
fn native32_planar_inplace_exact_widening_and64_independent_weighted_channels() {
    let rate = 8007;
    let n = endpoint(4, rate) + 19;
    let roles: [Role; 64] = std::array::from_fn(|i| {
        if i % 3 == 0 {
            Role::Surround
        } else {
            Role::Front
        }
    });
    let source: Vec<Vec<f32>> = (0..64)
        .map(|i| {
            signal(rate, n, i as f64 * 0.3)
                .into_iter()
                .map(|x| x as f32)
                .collect()
        })
        .collect();
    let wide: Vec<Vec<f64>> = source
        .iter()
        .map(|v| v.iter().map(|&x| x as f64).collect())
        .collect();
    let mut planar = meter(rate, &roles, 65536);
    let mut inplace = meter(rate, &roles, 65536);
    let mut reference = meter(rate, &roles, 65536);
    planar
        .observe_planar_f32(&source.iter().map(|v| v.as_slice()).collect::<Vec<_>>(), n)
        .unwrap();
    let mut mutable = source.clone();
    inplace
        .observe_in_place_f32(
            &mutable
                .iter_mut()
                .map(|v| v.as_mut_slice())
                .collect::<Vec<_>>(),
            n,
        )
        .unwrap();
    reference
        .observe_planar(&wide.iter().map(|v| v.as_slice()).collect::<Vec<_>>(), n)
        .unwrap();
    assert_eq!(source, mutable);
    assert_eq!(planar.reading(), inplace.reading());
    assert_eq!(planar.reading(), reference.reading());
    assert_eq!(planar.integrated_records(), reference.integrated_records());
    let mut energy = 0.;
    for input in &wide {
        let mut individual = meter(rate, &[Role::Front], 65536);
        individual.observe_planar(&[input], n).unwrap();
        energy += 10_f64.powf((individual.integrated_records()[0] + 0.691) / 10.);
    }
    // This first independent sum is intentionally front-only; redo explicit roles.
    let mut weighted = 0.;
    for (input, role) in wide.iter().zip(roles) {
        let mut individual = meter(rate, &[role], 65536);
        individual.observe_planar(&[input], n).unwrap();
        weighted += 10_f64.powf((individual.integrated_records()[0] + 0.691) / 10.);
    }
    assert!(weighted > energy);
    assert!((planar.integrated_records()[0] - (-0.691 + 10. * weighted.log10())).abs() < 3e-13);
    let expected = offline_records(Design::new(rate).unwrap(), &roles, &wide, 4);
    close_records(planar.integrated_records(), &expected, 2e-9);
}
#[test]
fn observation_reset_control_analysis_and_lra_tail_have_zero_heap_work() {
    let mut m = meter(8001, &[Role::Front, Role::Surround], 65536);
    let a = signal(8001, 3 * 8001 - 1, 0.2);
    let b = signal(8001, a.len(), 0.5);
    let measured = allocation_counter::measure(|| {
        m.observe_planar(&[&a, &b], a.len()).unwrap();
        m.observe_planar(&[&[], &[]], 0).unwrap();
        m.finish(Tail::LraSilence1500ms).unwrap();
        let report = m.analyse_programme().unwrap();
        assert!(report.integrated_lufs.is_some());
        assert!(report.loudness_range_lu.is_some());
        m.reset();
        m.observe_frame(&[0.2, 0.7]).unwrap();
        m.finish(Tail::None).unwrap();
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
    assert_eq!(measured.bytes_total, 0);
}

// ---------------------------------------------------------------------------------------
// Discriminating follow-up evidence. Every oracle below uses direct-form I over the whole
// concatenated source and absolute source coordinates; none reuses the production
// transposed recurrence, rings or counters. These are SELECTED-input numerical and
// ownership checks of this meter's own declared policy, not standards conformance.

// Level of `count` consecutive grid blocks starting at block `first` of a grid that was
// (re)started at source frame `start`. `source` must already be K-filtered.
fn grid_level(
    roles: &[Role],
    source: &[Vec<f64>],
    rate: u32,
    start: usize,
    first: u64,
    count: u64,
) -> f64 {
    let begin = start + endpoint(first, rate);
    let end = start + endpoint(first + count, rate);
    let energy: f64 = source
        .iter()
        .zip(roles)
        .map(|(values, role)| {
            role.power_weight() * values[begin..end].iter().map(|v| v * v).sum::<f64>()
        })
        .sum();
    -0.691 + 10. * (energy / (end - begin) as f64).log10()
}
fn filter_all(d: Design, source: &[Vec<f64>]) -> Vec<Vec<f64>> {
    source.iter().map(|v| filtered(d, v)).collect()
}
// Whole-call observation in prepared-maximum chunks, so long programmes stay legal.
fn feed(m: &mut Meter, source: &[Vec<f64>], range: std::ops::Range<usize>) {
    for start in range.clone().step_by(65536) {
        let end = (start + 65536).min(range.end);
        let views: Vec<&[f64]> = source.iter().map(|v| &v[start..end]).collect();
        m.observe_planar(&views, end - start).unwrap();
    }
}
fn peak(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}
#[test]
fn reset_after_nonzero_k_history_makes_the_first_zero_record_exactly_negative_infinity() {
    let sets: [&[Role]; 3] = [
        &[Role::Front],
        &[Role::Surround],
        &[Role::Front, Role::Surround, Role::Excluded],
    ];
    for roles in sets {
        for rate in [8000, 8001, 48007] {
            let d = Design::new(rate).unwrap();
            let window = endpoint(4, rate);
            // 123 frames of the fifth block remain as partial grid energy at reset time.
            let history = window + 123;
            let total = endpoint(9, rate);
            let audio: Vec<Vec<f64>> = (0..roles.len())
                .map(|i| signal(rate, history, 0.3 * i as f64))
                .collect();
            let padded: Vec<Vec<f64>> = audio
                .iter()
                .map(|v| {
                    let mut v = v.clone();
                    v.resize(total, 0.);
                    v
                })
                .collect();
            // Control: identical history followed by zeros and NO reset. Record 5 covers
            // blocks 5..9, wholly after the audio, so a finite level proves that this
            // history leaves nonzero K recurrence memory in every non-excluded channel.
            let mut control = meter(rate, roles, 65536);
            feed(&mut control, &padded, 0..total);
            let ring = control.integrated_records();
            assert_eq!(ring.len(), 6);
            close_records(
                &ring[..5],
                &offline_records(d, roles, &padded, 4)[..5],
                2e-9,
            );
            assert!(
                ring[5].is_finite(),
                "rate={rate} roles={roles:?}: pure-ringing record {}",
                ring[5]
            );
            // Subject: same history, actual reset, then one zero window.
            let mut m = meter(rate, roles, 65536);
            feed(&mut m, &audio, 0..history);
            let before = m.reading();
            assert_eq!(before.source_frames, history as u64);
            assert!(before.integrated_records >= 1 && before.momentary_lufs.is_some());
            m.reset();
            let zeros = vec![vec![0.; window]; roles.len()];
            feed(&mut m, &zeros, 0..window - 1);
            assert!(
                m.integrated_records().is_empty(),
                "a stale grid/window ring would complete early"
            );
            assert_eq!(m.reading().momentary_lufs, None);
            feed(&mut m, &zeros, window - 1..window);
            let r = m.reading();
            assert_eq!(
                m.integrated_records(),
                [f64::NEG_INFINITY],
                "rate={rate} roles={roles:?}: stale filter history would be finite"
            );
            assert_eq!(r.momentary_lufs, Some(f64::NEG_INFINITY));
            assert_eq!(r.maximum_momentary_lufs, Some(f64::NEG_INFINITY));
            assert_eq!(r.momentary_endpoint_frame, Some(window as u64));
            assert_eq!(r.source_frames, window as u64);
            assert!(!r.nonfinite_input && r.numeric_fault.is_none());
            let mut fresh = meter(rate, roles, 65536);
            feed(&mut fresh, &zeros, 0..window);
            assert_eq!(r, fresh.reading());
            assert_eq!(m.lra_records(), fresh.lra_records());
            m.finish(Tail::None).unwrap();
            let report = m.analyse_programme().unwrap();
            assert_eq!(report.integrated_lufs, None);
            assert_eq!(report.integrated_absolute_count, 0);
        }
    }
}
#[test]
fn live_values_maxima_and_report_endpoints_follow_the_rational_oracle_and_tail_keeps_live_state() {
    let roles = [Role::Front, Role::Surround, Role::Excluded];
    for rate in [8000, 8001, 8003, 8009, 48007] {
        let d = Design::new(rate).unwrap();
        let n = 3 * rate as usize + rate as usize / 3 + 19;
        let mut source = [signal(rate, n, 0.), signal(rate, n, 0.4), vec![9.; n]];
        // A loud opening makes the maxima differ from the latest windows.
        let loud = rate as usize * 3 / 10;
        for channel in &mut source[..2] {
            for value in &mut channel[..loud] {
                *value *= 4.;
            }
        }
        let expected4 = offline_records(d, &roles, &source, 4);
        let expected30 = offline_records(d, &roles, &source, 30);
        let mut m = meter(rate, &roles, 65536);
        // One frame short of three seconds: no complete 30-block window yet.
        feed(&mut m, &source, 0..3 * rate as usize - 1);
        let early = m.reading();
        assert!(early.momentary_lufs.is_some());
        assert_eq!(early.short_term_lufs, None);
        assert_eq!(early.maximum_short_term_lufs, None);
        assert_eq!(early.short_term_endpoint_frame, None);
        assert_eq!(early.lra_records, 0);
        feed(&mut m, &source, 3 * rate as usize - 1..n);
        let r = m.reading();
        let k = ((n as u128 * 10 + 9) / rate as u128) as u64;
        assert!(k >= 33, "fixture must include complete short-term windows");
        let end = Some(endpoint(k, rate) as u64);
        let last_i = *m.integrated_records().last().unwrap();
        let last_s = *m.lra_records().last().unwrap();
        assert_eq!(r.momentary_lufs, Some(last_i));
        assert_eq!(r.short_term_lufs, Some(last_s));
        assert!((last_i - expected4.last().unwrap()).abs() < 2e-9);
        assert!((last_s - expected30.last().unwrap()).abs() < 2e-9);
        assert_eq!(r.momentary_endpoint_frame, end);
        assert_eq!(r.short_term_endpoint_frame, end);
        assert_eq!(r.programme_interval_frames, n as u64);
        assert_eq!(r.programme_source_frames, n as u64);
        assert_eq!(r.integrated_records, expected4.len());
        assert_eq!(r.lra_records, expected30.len());
        let max_i = peak(m.integrated_records());
        let max_s = peak(m.lra_records());
        assert_eq!(r.maximum_momentary_lufs, Some(max_i));
        assert_eq!(r.maximum_short_term_lufs, Some(max_s));
        assert!((max_i - peak(&expected4)).abs() < 2e-9);
        assert!((max_s - peak(&expected30)).abs() < 2e-9);
        assert!(
            max_i > last_i + 1.,
            "momentary maximum must not be the latest"
        );
        assert!(
            max_s > last_s + 1.,
            "short-term maximum must not be the latest"
        );
        let before = m.analyse_programme().unwrap();
        assert_eq!(before.integrated_endpoint_frame, end);
        assert_eq!(before.lra_endpoint_frame, end);
        assert_eq!(before.tail_frames, 0);
        m.finish(Tail::LraSilence1500ms).unwrap();
        let tail = (3 * rate as u64).div_ceil(2);
        let total = n as u64 + tail;
        let kt = ((total as u128 * 10 + 9) / rate as u128) as u64;
        let after = m.reading();
        // Artificial silence never reaches live windows, maxima or integrated records.
        assert_eq!(after.momentary_lufs, r.momentary_lufs);
        assert_eq!(after.short_term_lufs, r.short_term_lufs);
        assert_eq!(after.maximum_momentary_lufs, r.maximum_momentary_lufs);
        assert_eq!(after.maximum_short_term_lufs, r.maximum_short_term_lufs);
        assert_eq!(after.momentary_endpoint_frame, r.momentary_endpoint_frame);
        assert_eq!(after.short_term_endpoint_frame, r.short_term_endpoint_frame);
        assert_eq!(after.source_frames, n as u64);
        assert_eq!(after.programme_source_frames, n as u64);
        assert_eq!(after.integrated_records, r.integrated_records);
        assert_eq!(after.tail_frames, tail);
        assert_eq!(after.programme_interval_frames, total);
        assert!(after.finalized && after.lra_records > r.lra_records);
        let report = m.analyse_programme().unwrap();
        assert_eq!(report.integrated_endpoint_frame, end);
        assert_eq!(report.lra_endpoint_frame, Some(endpoint(kt, rate) as u64));
        assert!(report.lra_endpoint_frame > before.lra_endpoint_frame);
        assert_eq!(report.integrated_lufs, before.integrated_lufs);
        assert_eq!(
            report.integrated_relative_count,
            before.integrated_relative_count
        );
        assert_eq!(report.source_frames, n as u64);
        assert_eq!(report.tail_frames, tail);
        let padded: Vec<Vec<f64>> = source
            .iter()
            .map(|v| {
                let mut v = v.clone();
                v.resize(n + tail as usize, 0.);
                v
            })
            .collect();
        close_records(
            m.lra_records(),
            &offline_records(d, &roles, &padded, 30),
            2e-9,
        );
    }
}
#[test]
fn pause_resume_matches_direct_form_one_with_retained_k_history_and_no_partial_splice() {
    let rate = 8001;
    let d = Design::new(rate).unwrap();
    let roles = [Role::Front];
    let block4 = endpoint(4, rate);
    // Block 7 ends at frame 5600; three loud frames remain as partial block-8 energy.
    let first = endpoint(7, rate) + 3;
    let standby = 32 * (rate as usize / 10);
    let resumed = endpoint(6, rate) + 7;
    let mut audio: Vec<f64> = signal(rate, first, 0.1)
        .into_iter()
        .map(|v| v * 0.05)
        .collect();
    for value in &mut audio[first - 3..] {
        *value = 0.9;
    }
    audio.extend(signal(rate, standby, 0.7));
    let resume_at = audio.len();
    audio.extend(signal(rate, resumed, 1.3));
    let source = [audio];
    let weighted = filter_all(d, &source);
    let mut m = meter(rate, &roles, 65536);
    feed(&mut m, &source, 0..first);
    let records = m.integrated_records().to_vec();
    assert_eq!(records.len(), 4);
    for (i, &record) in records.iter().enumerate() {
        let expected = grid_level(&roles, &weighted, rate, 0, i as u64, 4);
        assert!((record - expected).abs() < 2e-9, "pre-pause record {i}");
    }
    m.set_programme_running(false).unwrap();
    let paused = m.reading();
    assert_eq!(paused.programme_interval_frames, 0);
    assert_eq!(paused.programme_source_frames, first as u64);
    feed(&mut m, &source, first..resume_at);
    assert_eq!(m.integrated_records(), records);
    m.set_programme_running(true).unwrap();
    feed(&mut m, &source, resume_at..resume_at + block4 - 1);
    assert_eq!(
        m.integrated_records(),
        records,
        "a stale partial block would complete a record early"
    );
    feed(&mut m, &source, resume_at + block4 - 1..resume_at + block4);
    assert_eq!(m.integrated_records().len(), 5);
    feed(&mut m, &source, resume_at + block4..resume_at + resumed);
    assert_eq!(m.integrated_records().len(), 7);
    for (i, &record) in m.integrated_records()[4..].iter().enumerate() {
        // Programme windows restart at the resume coordinate; the K history does not.
        let expected = grid_level(&roles, &weighted, rate, resume_at, i as u64, 4);
        assert!(
            (record - expected).abs() < 2e-9,
            "post-resume record {i}: actual={record:.17e} expected={expected:.17e}"
        );
    }
    // The 2e-9 tolerance is far below both plausible regressions on this fixture.
    let first_record = m.integrated_records()[4];
    let first_energy = 10_f64.powf((first_record + 0.691) / 10.) * block4 as f64;
    let stale: f64 = weighted[0][first - 3..first].iter().map(|v| v * v).sum();
    let spliced = -0.691 + 10. * ((first_energy + stale) / block4 as f64).log10();
    assert!(
        spliced - first_record > 1e-6,
        "splice margin {}",
        spliced - first_record
    );
    let restarted = [filtered(d, &source[0][resume_at..])];
    let reset_level = grid_level(&roles, &restarted, rate, 0, 0, 4);
    assert!(
        (reset_level - first_record).abs() > 1e-6,
        "K-history reset margin {}",
        reset_level - first_record
    );
    // Live windows never paused: absolute grid from frame 0, over the continuous K history.
    let total = source[0].len();
    let k = ((total as u128 * 10 + 9) / rate as u128) as u64;
    let live = m.reading();
    assert_eq!(
        live.momentary_endpoint_frame,
        Some(endpoint(k, rate) as u64)
    );
    let expected = grid_level(&roles, &weighted, rate, 0, k - 4, 4);
    assert!((live.momentary_lufs.unwrap() - expected).abs() < 2e-9);
    assert_eq!(live.source_frames, total as u64);
    assert_eq!(live.programme_source_frames, (first + resumed) as u64);
    assert_eq!(live.programme_interval_frames, resumed as u64);
    // Programme endpoints are absolute source coordinates across the standby gap.
    let report = m.analyse_programme().unwrap();
    assert_eq!(
        report.integrated_endpoint_frame,
        Some((resume_at + endpoint(6, rate)) as u64)
    );
}
#[test]
fn programme_pause_clears_the_lra_ring_and_needs_three_resumed_seconds() {
    let rate = 8000;
    let d = Design::new(rate).unwrap();
    let roles = [Role::Front];
    let q = rate as usize / 10;
    let first = 20 * rate as usize;
    let mut audio = signal(rate, first, 0.2);
    audio.extend(signal(rate, 5 * rate as usize, 0.6));
    let resume_at = audio.len();
    audio.extend(signal(rate, 3 * rate as usize, 1.1));
    let source = [audio];
    let weighted = filter_all(d, &source);
    let mut m = meter(rate, &roles, 65536);
    feed(&mut m, &source, 0..first);
    let before = m.lra_records().to_vec();
    assert_eq!(before.len(), 200 - 29);
    m.set_programme_running(false).unwrap();
    feed(&mut m, &source, first..resume_at);
    assert_eq!(m.lra_records(), before);
    m.set_programme_running(true).unwrap();
    let mut at = 0;
    for end in (1..=29).map(|block| block * q).chain([30 * q - 1]) {
        feed(&mut m, &source, resume_at + at..resume_at + end);
        at = end;
        assert_eq!(
            m.lra_records().len(),
            before.len(),
            "{end} resumed frames: a stale 30-block ring would emit a record"
        );
    }
    feed(&mut m, &source, resume_at + at..resume_at + 30 * q);
    assert_eq!(m.lra_records().len(), before.len() + 1);
    assert_eq!(&m.lra_records()[..before.len()], &before[..]);
    let expected = grid_level(&roles, &weighted, rate, resume_at, 0, 30);
    let actual = m.lra_records()[before.len()];
    assert!(
        (actual - expected).abs() < 2e-9,
        "actual={actual:.17e} expected={expected:.17e}"
    );
    assert_eq!(m.reading().lra_records, before.len() + 1);
    // Complete records from before the gap remain part of the retained distribution.
    assert_eq!(
        m.analyse_programme().unwrap().lra_absolute_count,
        before.len() + 1
    );
}
#[test]
fn excluded_channels_run_no_filter_and_only_set_the_nonfinite_flag() {
    let rate = 8000;
    let n = 3 * rate as usize + 1234;
    let front: Vec<f64> = signal(rate, n, 0.2).into_iter().map(|v| v * 0.25).collect();
    let mut reference = meter(rate, &[Role::Front], 65536);
    feed(&mut reference, std::slice::from_ref(&front), 0..n);
    let expected = reference.reading();
    assert!(expected.short_term_lufs.is_some() && expected.integrated_records > 0);
    // A running Excluded filter would overflow on f64::MAX (the shelf b0 is above one).
    for value in [f64::MAX, -f64::MAX, 9., 0.] {
        let mut m = meter(rate, &[Role::Front, Role::Excluded], 65536);
        feed(&mut m, &[front.clone(), vec![value; n]], 0..n);
        let r = m.reading();
        assert!(r.numeric_fault.is_none(), "excluded constant {value}");
        assert!(!r.nonfinite_input, "finite excluded constant {value}");
        assert_eq!(r, expected);
        assert_eq!(m.integrated_records(), reference.integrated_records());
        assert_eq!(m.lra_records(), reference.lra_records());
        assert_eq!(
            m.analyse_programme().unwrap(),
            reference.analyse_programme().unwrap()
        );
    }
    let narrow: Vec<f32> = front.iter().map(|&v| v as f32).collect();
    let wide: Vec<f64> = narrow.iter().map(|&v| f64::from(v)).collect();
    let mut reference32 = meter(rate, &[Role::Front], 65536);
    feed(&mut reference32, std::slice::from_ref(&wide), 0..n);
    let excluded = vec![f32::MAX; n];
    let mut m = meter(rate, &[Role::Front, Role::Excluded], 65536);
    m.observe_planar_f32(&[&narrow[..], &excluded[..]], n)
        .unwrap();
    assert!(m.reading().numeric_fault.is_none());
    assert_eq!(m.reading(), reference32.reading());
    assert_eq!(m.integrated_records(), reference32.integrated_records());
    // Only the nonfinite flag observes an Excluded channel; Front stays finite throughout.
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut poisoned = vec![0.5; n];
        poisoned[n / 2] = bad;
        let mut m = meter(rate, &[Role::Front, Role::Excluded], 65536);
        feed(&mut m, &[front.clone(), poisoned], 0..n);
        let mut flagged = expected;
        flagged.nonfinite_input = true;
        assert_eq!(m.reading(), flagged, "excluded {bad}");
        assert_eq!(m.integrated_records(), reference.integrated_records());
        assert_eq!(m.lra_records(), reference.lra_records());
    }
}
#[test]
fn initial_standby_has_live_history_no_programme_records_and_refuses_tail_until_resumed() {
    let rate = 8000;
    let d = Design::new(rate).unwrap();
    let roles = [Role::Front];
    let q = rate as usize / 10;
    let standby = 4 * rate as usize;
    let mut audio = signal(rate, standby, 0.2);
    audio.extend(signal(rate, 6 * q, 0.9));
    let source = [audio];
    let weighted = filter_all(d, &source);
    let mut m = Meter::new(&d, &roles, 65536, capacities(1024, 1024), false).unwrap();
    assert!(!m.reading().running);
    feed(&mut m, &source, 0..standby);
    let r = m.reading();
    assert!(!r.running && !r.finalized);
    assert!(m.integrated_records().is_empty() && m.lra_records().is_empty());
    assert_eq!(r.programme_source_frames, 0);
    assert_eq!(r.programme_interval_frames, 0);
    assert_eq!((r.integrated_records, r.lra_records), (0, 0));
    assert_eq!(r.source_frames, standby as u64);
    // Live windows run in standby: four seconds are forty complete blocks.
    assert_eq!(r.momentary_endpoint_frame, Some(standby as u64));
    assert_eq!(r.short_term_endpoint_frame, Some(standby as u64));
    let momentary = grid_level(&roles, &weighted, rate, 0, 36, 4);
    let short = grid_level(&roles, &weighted, rate, 0, 10, 30);
    assert!((r.momentary_lufs.unwrap() - momentary).abs() < 2e-9);
    assert!((r.short_term_lufs.unwrap() - short).abs() < 2e-9);
    assert_eq!(m.finish(Tail::LraSilence1500ms), Err(Error::TailInStandby));
    assert_eq!(m.reading(), r);
    let report = m.analyse_programme().unwrap();
    assert_eq!(report.integrated_lufs, None);
    assert_eq!(report.loudness_range_lu, None);
    assert_eq!(report.integrated_endpoint_frame, None);
    assert_eq!(report.lra_endpoint_frame, None);
    assert_eq!(report.programme_source_frames, 0);
    assert_eq!(report.integrated_absolute_count, 0);
    m.set_programme_running(true).unwrap();
    feed(&mut m, &source, standby..standby + 4 * q - 1);
    assert!(
        m.integrated_records().is_empty(),
        "first record needs four grid blocks"
    );
    feed(&mut m, &source, standby + 4 * q - 1..standby + 4 * q);
    assert_eq!(m.integrated_records().len(), 1);
    feed(&mut m, &source, standby + 4 * q..standby + 6 * q);
    assert_eq!(m.integrated_records().len(), 3);
    for (i, &record) in m.integrated_records().iter().enumerate() {
        // The K filters ran during standby, so the first programme window sees that history.
        let expected = grid_level(&roles, &weighted, rate, standby, i as u64, 4);
        assert!((record - expected).abs() < 2e-9, "record {i}");
    }
    assert_eq!(m.reading().programme_source_frames, 6 * q as u64);
    m.finish(Tail::LraSilence1500ms).unwrap();
    let r = m.reading();
    assert_eq!(r.tail_frames, 3 * rate as u64 / 2);
    assert_eq!(
        r.lra_records, 0,
        "21 programme blocks cannot complete a 30-block window"
    );
    m.reset();
    assert!(
        m.reading().running,
        "reset keeps the current running choice"
    );
    let mut standby_owner = Meter::new(&d, &roles, 1, capacities(1, 1), false).unwrap();
    standby_owner.reset();
    assert!(!standby_owner.reading().running);
    // Declared no-tail case: running, but nothing was ever admitted to the programme.
    let mut empty = Meter::new(&d, &roles, 65536, capacities(1024, 1024), false).unwrap();
    feed(&mut empty, &source, 0..standby);
    empty.set_programme_running(true).unwrap();
    empty.finish(Tail::LraSilence1500ms).unwrap();
    let r = empty.reading();
    assert!(r.finalized);
    assert_eq!((r.tail_frames, r.programme_source_frames), (0, 0));
}
type SampleFn = fn(usize, usize) -> f32;
fn ordinary_f32(frame: usize, channel: usize) -> f32 {
    (0.1 + 1e-5 * (frame % 997) as f32) * if channel == 1 { -1. } else { 1. }
}
#[test]
fn single_frame_f32_equals_exact_widening_for_ordinary_extreme_and_nonfinite_values() {
    let roles = [Role::Front, Role::Surround, Role::Excluded];
    let rate = 8000;
    let n = 3 * rate as usize + 100;
    let cases: [(&str, SampleFn); 3] = [
        ("ordinary", ordinary_f32),
        ("f32::MAX", |frame, channel| {
            if channel == 2 {
                0.1
            } else if frame == 0 {
                f32::MAX
            } else {
                -f32::MAX
            }
        }),
        ("nonfinite", |frame, channel| {
            match (frame % 1000, channel) {
                (3, 0) => f32::NAN,
                (500, 1) => f32::INFINITY,
                (700, 2) => f32::NEG_INFINITY,
                _ => ordinary_f32(frame, channel),
            }
        }),
    ];
    for (name, make) in cases {
        let frames: Vec<[f32; 3]> = (0..n)
            .map(|frame| std::array::from_fn(|channel| make(frame, channel)))
            .collect();
        let columns: Vec<Vec<f32>> = (0..3)
            .map(|channel| frames.iter().map(|f| f[channel]).collect())
            .collect();
        let mut single = meter(rate, &roles, 65536);
        let mut widened = meter(rate, &roles, 65536);
        let mut planar = meter(rate, &roles, 65536);
        for frame in &frames {
            single.observe_frame_f32(frame).unwrap();
            widened.observe_frame(&frame.map(f64::from)).unwrap();
        }
        let views: Vec<&[f32]> = columns.iter().map(|v| v.as_slice()).collect();
        planar.observe_planar_f32(&views, n).unwrap();
        let r = single.reading();
        assert_eq!(r, widened.reading(), "{name}: per-frame f32 vs exact f64");
        assert_eq!(r, planar.reading(), "{name}: per-frame vs planar f32");
        assert_eq!(single.integrated_records(), widened.integrated_records());
        assert_eq!(single.lra_records(), widened.lra_records());
        assert_eq!(single.integrated_records(), planar.integrated_records());
        assert_eq!(r.nonfinite_input, name == "nonfinite", "{name}");
        assert!(r.numeric_fault.is_none(), "{name}");
        assert_eq!(
            single.analyse_programme().unwrap(),
            widened.analyse_programme().unwrap()
        );
        if name == "f32::MAX" {
            // 20*log10(1.3216*f32::MAX) is about 773 dB and the first window keeps that
            // transient, so a lower bound near 737 and a loose upper bound hold only if
            // the whole range was measured in f64 without an f32 intermediate.
            let maximum = r.maximum_momentary_lufs.unwrap();
            assert!((700. ..800.).contains(&maximum), "{name}: {maximum}");
        }
    }
}
#[test]
fn surround_overflow_in_a_later_call_latches_its_coordinate_and_keeps_raw_records_diagnostic() {
    let rate = 8000;
    let roles = [Role::Front, Role::Surround];
    let total = endpoint(10, rate);
    let block5 = endpoint(5, rate);
    let fault_at = block5 + 7;
    let clean = [signal(rate, total, 0.1), signal(rate, total, 0.6)];
    let mut poisoned = clean.clone();
    poisoned[1][fault_at] = f64::MAX;
    let mut control = meter(rate, &roles, 65536);
    feed(&mut control, &clean, 0..total);
    let mut m = meter(rate, &roles, 65536);
    feed(&mut m, &poisoned, 0..block5);
    assert!(m.reading().numeric_fault.is_none());
    let prefix = m.integrated_records().to_vec();
    assert_eq!(prefix.len(), 2);
    feed(&mut m, &poisoned, block5..total);
    let r = m.reading();
    let first = r.numeric_fault.unwrap();
    assert_eq!(first.coordinate, fault_at as u64);
    assert_eq!((first.channel, first.section), (1, 0));
    assert!(!first.artificial_tail);
    // Accepted progress continues after the latch, and reports/live values are invalid.
    assert_eq!(r.source_frames, total as u64);
    assert_eq!(r.programme_source_frames, total as u64);
    assert_eq!(r.integrated_records, 7);
    assert_eq!(r.momentary_lufs, None);
    assert_eq!(r.short_term_lufs, None);
    assert_eq!(r.maximum_momentary_lufs, None);
    assert_eq!(m.analyse_programme(), Err(Error::NumericFault));
    // Raw records are diagnostic history only, never a valid programme result.
    let raw = m.integrated_records();
    let valid = control.integrated_records();
    assert_eq!(raw.len(), valid.len());
    assert_eq!(&raw[..2], &prefix[..]);
    assert_eq!(&raw[..2], &valid[..2]);
    for (i, (&faulted, &clean)) in raw.iter().zip(valid).enumerate().skip(2).take(4) {
        assert!(
            faulted.is_finite() && faulted < clean,
            "record {i}: partial post-fault block must lose energy"
        );
    }
    assert_eq!(raw[6], f64::NEG_INFINITY);
    assert!(valid[6].is_finite());
    m.observe_frame(&[f64::MAX, f64::MAX]).unwrap();
    assert_eq!(
        m.reading().numeric_fault,
        Some(first),
        "first fault stays latched"
    );
    assert_eq!(m.reading().source_frames, total as u64 + 1);
    m.reset();
    assert!(m.integrated_records().is_empty());
    assert_eq!(m.reading().numeric_fault, None);
}
#[test]
fn nonfinite_poison_programme_control_and_reset_are_heap_free_after_preparation() {
    // Harness sanity: CONTROL construction is outside the no-heap scope by design.
    let construction = allocation_counter::measure(|| {
        let _ = meter(8000, &[Role::Front, Role::Surround], 65536);
    });
    assert!(construction.count_total > 0);
    let n = 3300;
    let mut a = signal(8000, n, 0.2);
    let mut b = signal(8000, n, 0.5);
    for (frame, value) in [(3, f64::NAN), (4, f64::NAN), (500, f64::INFINITY)] {
        a[frame] = value;
    }
    for (frame, value) in [(900, f64::NEG_INFINITY), (901, f64::NAN)] {
        b[frame] = value;
    }
    let a32: Vec<f32> = a.iter().map(|&v| v as f32).collect();
    let b32: Vec<f32> = b.iter().map(|&v| v as f32).collect();
    let (mut in_place_a, mut in_place_b) = (a.clone(), b.clone());
    let mut m = meter(8000, &[Role::Front, Role::Surround], 65536);
    let measured = allocation_counter::measure(|| {
        m.observe_planar(&[&a, &b], n).unwrap();
        m.observe_planar_f32(&[&a32, &b32], n).unwrap();
        m.observe_in_place(&[in_place_a.as_mut_slice(), in_place_b.as_mut_slice()], n)
            .unwrap();
        m.observe_frame(&[f64::NAN, 0.1]).unwrap();
        m.observe_frame_f32(&[0.1, f32::INFINITY]).unwrap();
        m.set_programme_running(false).unwrap();
        m.observe_planar(&[&a, &b], n).unwrap();
        m.set_programme_running(true).unwrap();
        m.observe_planar(&[&a, &b], n).unwrap();
        assert!(m.reading().nonfinite_input);
        m.reset();
        m.set_programme_running(false).unwrap();
        m.set_programme_running(true).unwrap();
        m.observe_frame(&[0.2, 0.3]).unwrap();
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
    assert_eq!(measured.bytes_total, 0);
    let r = m.reading();
    assert!(!r.nonfinite_input && r.numeric_fault.is_none() && r.running);
    assert_eq!(r.source_frames, 1);
    assert!(a[3].is_nan() && in_place_a[3].is_nan() && b[900].is_infinite());
}
