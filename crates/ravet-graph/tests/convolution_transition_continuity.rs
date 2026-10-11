#![cfg(feature = "convolution-transitions")]
//! Continuity policies for live convolution transitions (ADR 0275): history
//! priming and latency alignment against independently computed references,
//! measured level dips and alignment errors, impulse onsets, refusals that leave
//! state unchanged, allocation-free audio and a seeded concurrent stress run.
use ravet_graph::{
    AlignmentPolicy, ConvolutionTransition, ConvolutionTransitionControl, CrossfadeLaw,
    CrossfadePolicy, LatencyPolicy, MidFadePolicy, PrimingPolicy, ReturnReason,
    TRANSITION_MAXIMUM_ALIGNMENT_FRAMES, TRANSITION_MAXIMUM_PRIMING_FRAMES, TransitionContinuity,
    TransitionEngine, TransitionEvent, TransitionOfferFailure, TransitionOptions, TransitionReport,
    TransitionSetupFailure, TransitionStatus, convolution_transition,
    convolution_transition_with_continuity,
};
use ravet_signal::{ConvolutionBank32, ConvolutionBank64, ConvolutionPartitioning as P};
use std::f64::consts::FRAC_PI_2;

const MAX: usize = 64;
const BUDGET: usize = 32 << 20;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 * 2. - 1.
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

trait Kind: TransitionEngine + Sized {
    fn make(h: &[f64], p: P) -> Box<Self>;
    fn gain(g: f64) -> Self::Sample;
    fn sample(x: f64) -> Self::Sample;
    fn wide(x: Self::Sample) -> f64;
    fn bits(x: Self::Sample) -> u64;
}
fn stereo(h: &[f64]) -> (Vec<f64>, Vec<f64>) {
    (h.to_vec(), h.iter().rev().map(|x| -0.5 * x).collect())
}
impl Kind for ConvolutionBank64 {
    fn make(h: &[f64], p: P) -> Box<Self> {
        let (a, b) = stereo(h);
        Box::new(ConvolutionBank64::new(&[&a, &b], p, MAX, BUDGET).unwrap())
    }
    fn gain(g: f64) -> f64 {
        g
    }
    fn sample(x: f64) -> f64 {
        x
    }
    fn wide(x: f64) -> f64 {
        x
    }
    fn bits(x: f64) -> u64 {
        x.to_bits()
    }
}
impl Kind for ConvolutionBank32 {
    fn make(h: &[f64], p: P) -> Box<Self> {
        let (a, b) = stereo(h);
        let a: Vec<f32> = a.iter().map(|&x| x as f32).collect();
        let b: Vec<f32> = b.iter().map(|&x| x as f32).collect();
        Box::new(ConvolutionBank32::new(&[&a, &b], p, MAX, BUDGET).unwrap())
    }
    fn gain(g: f64) -> f32 {
        g as f32
    }
    fn sample(x: f64) -> f32 {
        x as f32
    }
    fn wide(x: f32) -> f64 {
        f64::from(x)
    }
    fn bits(x: f32) -> u64 {
        u64::from(x.to_bits())
    }
}

fn response(seed: u64, taps: usize) -> Vec<f64> {
    let mut rng = Rng(seed);
    (0..taps)
        .map(|i| rng.unit() * (-(i as f64) / (taps as f64 / 4.)).exp() * 0.3)
        .collect()
}
/// A slowly decaying long response: most energy lies beyond a short fade.
fn long_response(seed: u64, taps: usize) -> Vec<f64> {
    let mut rng = Rng(seed);
    (0..taps)
        .map(|i| rng.unit() * (-(i as f64) / (taps as f64 / 1.5)).exp() * 0.05)
        .collect()
}
fn noise<E: Kind>(seed: u64, frames: usize) -> Vec<Vec<E::Sample>> {
    let mut rng = Rng(seed);
    (0..2)
        .map(|_| (0..frames).map(|_| E::sample(rng.unit() * 0.5)).collect())
        .collect()
}
fn options(frames: usize, law: CrossfadeLaw, mid: MidFadePolicy) -> TransitionOptions {
    TransitionOptions {
        crossfade: CrossfadePolicy { frames, law },
        mid_fade: mid,
        latency: LatencyPolicy::Report,
    }
}
fn continuity(priming: PrimingPolicy, alignment: AlignmentPolicy) -> TransitionContinuity {
    TransitionContinuity { priming, alignment }
}
fn prime(frames: usize, frames_per_block: usize) -> PrimingPolicy {
    PrimingPolicy::History {
        frames,
        frames_per_block,
    }
}
fn align(maximum_difference: usize) -> AlignmentPolicy {
    AlignmentPolicy::DelayLower { maximum_difference }
}

/// A freshly prepared engine over `input[start..]` (zero output before `start`).
fn solo<E: Kind>(h: &[f64], p: P, input: &[Vec<E::Sample>], start: usize) -> Vec<Vec<E::Sample>> {
    let mut engine = E::make(h, p);
    let total = input[0].len();
    let mut out = vec![vec![E::Sample::default(); total]; 2];
    let mut at = start;
    while at < total {
        let n = MAX.min(total - at);
        for c in 0..2 {
            engine
                .transition_process(c, &input[c][at..at + n], &mut out[c][at..at + n], n)
                .unwrap();
        }
        at += n;
    }
    out
}

/// Independent model of one transition whose fade starts at `start`: the old
/// path is `old[t - old_delay]` and the new path `new[t - new_delay]` during the
/// fade (zero before frame 0), `old[t]` before and `new[t]` after.
fn model<E: Kind>(
    old: &[Vec<E::Sample>],
    new: &[Vec<E::Sample>],
    start: usize,
    fade: CrossfadePolicy,
    old_delay: usize,
    new_delay: usize,
) -> Vec<Vec<E::Sample>> {
    let total = old[0].len();
    let at = |s: &[E::Sample], t: usize, d: usize| {
        if t >= d {
            s[t - d]
        } else {
            E::Sample::default()
        }
    };
    let mut out = vec![vec![E::Sample::default(); total]; 2];
    for c in 0..2 {
        for t in 0..total {
            out[c][t] = if t < start {
                old[c][t]
            } else if t - start < fade.frames {
                let x = (t - start + 1) as f64 / fade.frames as f64;
                let (g_old, g_new) = match fade.law {
                    CrossfadeLaw::EqualGain => (1. - x, x),
                    CrossfadeLaw::EqualPower => ((x * FRAC_PI_2).cos(), (x * FRAC_PI_2).sin()),
                };
                E::gain(g_old) * at(&old[c], t, old_delay)
                    + E::gain(g_new) * at(&new[c], t, new_delay)
            } else {
                new[c][t]
            };
        }
    }
    out
}

struct Run<S> {
    output: Vec<Vec<S>>,
    statuses: Vec<(usize, TransitionStatus)>,
    returned: Vec<(ReturnReason, u64)>,
    reports: Vec<TransitionReport>,
    output_latency: Option<usize>,
}
fn drive<E: Kind>(
    initial: Box<E>,
    opts: TransitionOptions,
    policies: TransitionContinuity,
    input: &[Vec<E::Sample>],
    blocks: &[usize],
    offers: Vec<(usize, Box<E>)>,
) -> Run<E::Sample> {
    drive_with_resets(initial, opts, policies, input, blocks, offers, &[])
}
/// As [`drive`], calling `reset` before the blocks that start at `resets`.
fn drive_with_resets<E: Kind>(
    initial: Box<E>,
    opts: TransitionOptions,
    policies: TransitionContinuity,
    input: &[Vec<E::Sample>],
    blocks: &[usize],
    mut offers: Vec<(usize, Box<E>)>,
    resets: &[usize],
) -> Run<E::Sample> {
    let (mut control, mut audio) =
        convolution_transition_with_continuity(initial, MAX, opts, policies).unwrap();
    let total = input[0].len();
    let mut run = Run {
        output: vec![vec![E::Sample::default(); total]; 2],
        statuses: Vec::new(),
        returned: Vec::new(),
        reports: Vec::new(),
        output_latency: None,
    };
    offers.reverse();
    let mut at = 0;
    for &n in blocks {
        while offers.last().is_some_and(|(f, _)| *f <= at) {
            let (f, engine) = offers.pop().unwrap();
            assert_eq!(f, at, "offers must align with block starts");
            control.offer(engine).unwrap();
        }
        if resets.contains(&at) {
            audio.reset();
        }
        let (l, r) = run.output.split_at_mut(1);
        let status = audio
            .process(
                &[&input[0][at..at + n], &input[1][at..at + n]],
                &mut [&mut l[0][at..at + n], &mut r[0][at..at + n]],
                n,
            )
            .unwrap();
        run.statuses.push((at, status));
        at += n;
        while let Some(x) = control.take_returned() {
            run.returned.push((x.reason, x.sequence));
        }
        while let Some(x) = control.take_report() {
            run.reports.push(x);
        }
    }
    assert_eq!(at, total);
    run.output_latency = audio.output_latency_samples();
    run
}
fn uniform(total: usize, n: usize) -> Vec<usize> {
    let mut v = vec![n; total / n];
    if !total.is_multiple_of(n) {
        v.push(total % n);
    }
    v
}
fn random_blocks(seed: u64, splits: &[usize], total: usize) -> Vec<usize> {
    let mut rng = Rng(seed);
    let mut v = Vec::new();
    let mut at = 0;
    for &end in splits.iter().chain([&total]) {
        while at < end {
            let n = (1 + rng.below(MAX)).min(end - at);
            v.push(n);
            at += n;
        }
    }
    v
}
/// Uniform blocks of `n` frames restarted at `split`.
fn uniform_split(split: usize, total: usize, n: usize) -> Vec<usize> {
    let mut v = uniform(split, n);
    v.extend(uniform(total - split, n));
    v
}
fn segmentations(split: usize, total: usize) -> Vec<Vec<usize>> {
    vec![
        uniform_split(split, total, MAX),
        uniform(total, 1),
        uniform_split(split, total, 7),
        random_blocks(99, &[split], total),
        random_blocks(1234, &[split], total),
    ]
}
fn assert_same<S: Copy>(a: &[Vec<S>], b: &[Vec<S>], bits: fn(S) -> u64, what: &str) {
    for c in 0..2 {
        for (t, (x, y)) in a[c].iter().zip(&b[c]).enumerate() {
            assert_eq!(bits(*x), bits(*y), "{what}: channel {c} frame {t}");
        }
    }
}
fn max_error<E: Kind>(a: &[Vec<E::Sample>], b: &[Vec<E::Sample>], from: usize, to: usize) -> f64 {
    (0..2)
        .flat_map(|c| (from..to).map(move |t| (c, t)))
        .map(|(c, t)| (E::wide(a[c][t]) - E::wide(b[c][t])).abs())
        .fold(0., f64::max)
}
fn peak<E: Kind>(a: &[Vec<E::Sample>]) -> f64 {
    a.iter()
        .flatten()
        .map(|&x| E::wide(x).abs())
        .fold(0., f64::max)
}
/// Smallest windowed RMS ratio `output / ideal` (channel 0) over `from..to`, in dB.
fn dip_db<E: Kind>(
    output: &[Vec<E::Sample>],
    ideal: &[Vec<E::Sample>],
    from: usize,
    to: usize,
    window: usize,
) -> (f64, usize) {
    let mut worst = (f64::INFINITY, from);
    let mut at = from;
    while at + window <= to {
        let energy = |s: &[E::Sample]| {
            s[at..at + window]
                .iter()
                .map(|&x| E::wide(x).powi(2))
                .sum::<f64>()
        };
        let ratio = (energy(&output[0]) / energy(&ideal[0])).sqrt();
        let db = 20. * ratio.log10();
        if db < worst.0 {
            worst = (db, at);
        }
        at += window / 2;
    }
    worst
}

// ---- Default behaviour ----

/// `convolution_transition` delegates to `convolution_transition_with_continuity`
/// with `NONE`, so this compares one code path through its two entry points; it
/// is not evidence against the ADR 0272 build. That evidence is
/// `default_constructor_matches_digests_recorded_from_the_w259_build` and the
/// unchanged `convolution_transition.rs` tests.
#[test]
fn continuity_none_entry_point_equals_the_default_constructor() {
    type E = ConvolutionBank64;
    let (a, b) = (response(1, 200), response(2, 333));
    let (pa, pb) = (
        P::ZeroLatency { head: 16 },
        P::UniformDelayed { partition: 32 },
    );
    let input = noise::<E>(3, 1600);
    let opts = options(300, CrossfadeLaw::EqualPower, MidFadePolicy::Refuse);
    for blocks in segmentations(448, 1600) {
        let with = drive(
            E::make(&a, pa),
            opts,
            TransitionContinuity::NONE,
            &input,
            &blocks,
            vec![(448, E::make(&b, pb))],
        );
        // The same run through the default constructor.
        let (mut control, mut audio) = convolution_transition(E::make(&a, pa), MAX, opts).unwrap();
        let mut output = vec![vec![0.; 1600]; 2];
        let mut statuses = Vec::new();
        let mut reports = Vec::new();
        let mut at = 0;
        for &n in &blocks {
            if at == 448 {
                control.offer(E::make(&b, pb)).unwrap();
            }
            let (l, r) = output.split_at_mut(1);
            let status = audio
                .process(
                    &[&input[0][at..at + n], &input[1][at..at + n]],
                    &mut [&mut l[0][at..at + n], &mut r[0][at..at + n]],
                    n,
                )
                .unwrap();
            statuses.push((at, status));
            while let Some(x) = control.take_report() {
                reports.push(x);
            }
            at += n;
        }
        assert_same(&with.output, &output, E::bits, "NONE equals default");
        assert_eq!(with.statuses, statuses);
        assert_eq!(with.reports, reports);
        assert_eq!(with.output_latency, None);
        assert_eq!(audio.output_latency_samples(), None);
        assert_eq!(audio.continuity(), TransitionContinuity::default());
    }
}

// ---- History priming ----

const LONG: usize = 4096;
const PF: usize = 6144;
const PL: usize = 256;
const PTOTAL: usize = PF + LONG + 1024;

fn priming_case<E: Kind>() {
    let a = response(21, 300);
    let b = long_response(22, LONG);
    let p = P::NonUniform {
        head: 16,
        tail: 256,
    };
    let input = noise::<E>(23, PTOTAL);
    let fade = CrossfadePolicy {
        frames: PL,
        law: CrossfadeLaw::EqualGain,
    };
    let opts = options(PL, fade.law, MidFadePolicy::Refuse);
    let old = solo::<E>(&a, p, &input, 0);
    let steady = solo::<E>(&b, p, &input, 0);
    let ideal = model::<E>(&old, &steady, PF, fade, 0, 0);
    let unprimed_expected = model::<E>(&old, &solo::<E>(&b, p, &input, PF), PF, fade, 0, 0);
    let primed_expected = model::<E>(&old, &solo::<E>(&b, p, &input, PF - LONG), PF, fade, 0, 0);
    // Rounding tolerance relative to peak for unaligned partition phase.
    let tolerance = if std::mem::size_of::<E::Sample>() == 8 {
        1e-12
    } else {
        1e-5
    };
    let mut unprimed_dip = 0.;
    let mut primed_dip = 0.;
    let mut steady_error = 0f64;
    for blocks in segmentations(PF, PTOTAL) {
        let none = drive(
            E::make(&a, p),
            opts,
            TransitionContinuity::NONE,
            &input,
            &blocks,
            vec![(PF, E::make(&b, p))],
        );
        assert_same(&none.output, &unprimed_expected, E::bits, "unprimed");
        let primed = drive(
            E::make(&a, p),
            opts,
            continuity(prime(LONG, LONG), AlignmentPolicy::None),
            &input,
            &blocks,
            vec![(PF, E::make(&b, p))],
        );
        // Exact: the primed engine equals a fresh engine fed input[PF - LONG..].
        assert_same(&primed.output, &primed_expected, E::bits, "primed");
        let started: Vec<_> = primed
            .statuses
            .iter()
            .filter(|s| s.1.started.is_some())
            .collect();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].0, PF);
        assert_eq!(started[0].1.primed_frames, LONG);
        assert!(!started[0].1.priming);
        assert_eq!(
            primed.reports[0],
            TransitionReport {
                sequence: 1,
                event: TransitionEvent::PrimingStarted {
                    history_frames: LONG
                },
                frame: PF as u64,
                lost_before: 0
            }
        );
        assert!(matches!(
            primed.reports[1].event,
            TransitionEvent::Started { .. }
        ));
        // Independent steady-state reference: a never-switched engine.
        steady_error = steady_error.max(max_error::<E>(&primed.output, &ideal, PF, PTOTAL));
        unprimed_dip = dip_db::<E>(&none.output, &ideal, PF, PF + LONG + PL, 128).0;
        primed_dip = dip_db::<E>(&primed.output, &ideal, PF, PF + LONG + PL, 128).0;
    }
    // History not a multiple of any partition: exact against the suffix reference,
    // and equal to the never-switched engine to rounding only.
    const ODD: usize = 4100;
    let odd = drive(
        E::make(&a, p),
        opts,
        continuity(prime(ODD, ODD), AlignmentPolicy::None),
        &input,
        &uniform_split(PF, PTOTAL, MAX),
        vec![(PF, E::make(&b, p))],
    );
    let odd_expected = model::<E>(&old, &solo::<E>(&b, p, &input, PF - ODD), PF, fade, 0, 0);
    assert_same(&odd.output, &odd_expected, E::bits, "unaligned history");
    let odd_error = max_error::<E>(&odd.output, &ideal, PF, PTOTAL) / peak::<E>(&ideal);
    let odd_dip = dip_db::<E>(&odd.output, &ideal, PF, PF + LONG + PL, 128).0;
    // Predicted worst dip at the fade end for white input: missing tail energy.
    let energy = |h: &[f64]| h.iter().map(|x| x * x).sum::<f64>();
    let predicted = 10. * (energy(&b[..PL]) / energy(&b)).log10();
    let relative = steady_error / peak::<E>(&ideal);
    println!(
        "W264 priming {}: dip without priming {unprimed_dip:.2} dB (predicted at fade end {predicted:.2} dB), with priming {primed_dip:.6} dB; max error vs never-switched {steady_error:.3e} ({relative:.3e} of peak); unaligned 4100-frame history: {odd_error:.3e} of peak, dip {odd_dip:.2e} dB",
        std::any::type_name::<E>()
    );
    assert!(odd_error < tolerance, "{odd_error}");
    assert!(odd_dip.abs() < 1e-3, "{odd_dip}");
    assert!(unprimed_dip < -6., "{unprimed_dip}");
    assert!(primed_dip.abs() < 1e-3, "{primed_dip}");
    assert!(relative < tolerance, "{relative}");
}

#[test]
fn priming_matches_never_switched_engine_and_removes_the_dip_f64() {
    priming_case::<ConvolutionBank64>();
}
#[test]
fn priming_matches_never_switched_engine_and_removes_the_dip_f32() {
    priming_case::<ConvolutionBank32>();
}

#[test]
fn delayed_fade_priming_is_bounded_per_block_and_exact() {
    type E = ConvolutionBank64;
    const B: usize = 200;
    let a = response(31, 300);
    let b = long_response(32, LONG);
    let p = P::NonUniform {
        head: 16,
        tail: 256,
    };
    let input = noise::<E>(33, PTOTAL);
    let fade = CrossfadePolicy {
        frames: PL,
        law: CrossfadeLaw::EqualPower,
    };
    let opts = options(PL, fade.law, MidFadePolicy::Refuse);
    let old = solo::<E>(&a, p, &input, 0);
    let suffix = solo::<E>(&b, p, &input, PF - LONG);
    let steady = solo::<E>(&b, p, &input, 0);
    for blocks in [
        uniform(PTOTAL, MAX),
        uniform_split(PF, PTOTAL, 7),
        random_blocks(5, &[PF], PTOTAL),
    ] {
        let run = drive(
            E::make(&a, p),
            opts,
            continuity(prime(LONG, B), AlignmentPolicy::None),
            &input,
            &blocks,
            vec![(PF, E::make(&b, p))],
        );
        // Independent simulation of the documented catch-up schedule.
        let first = blocks
            .iter()
            .scan(0, |at, n| {
                let s = *at;
                *at += n;
                Some(s)
            })
            .position(|s| s == PF)
            .unwrap();
        let mut lag = LONG;
        let mut at = PF;
        let mut index = first;
        let mut delayed = 0;
        let start = loop {
            let chunk = B.min(lag);
            let status = run.statuses[index].1;
            assert_eq!(run.statuses[index].0, at);
            assert_eq!(status.primed_frames, chunk, "bounded priming at {at}");
            lag -= chunk;
            if lag == 0 {
                assert_eq!(status.started, Some(1));
                assert!(!status.priming);
                break at;
            }
            assert!(status.priming && status.started.is_none());
            lag += blocks[index];
            at += blocks[index];
            index += 1;
            delayed += 1;
        };
        let largest = *blocks.iter().max().unwrap();
        let bound = (LONG - B).div_ceil(B - largest);
        assert!(delayed <= bound, "{delayed} > {bound}");
        for (_, status) in &run.statuses {
            assert!(status.primed_frames <= B);
        }
        let expected = model::<E>(&old, &suffix, start, fade, 0, 0);
        assert_same(&run.output, &expected, E::bits, "delayed fade");
        let error = max_error::<E>(
            &run.output[..],
            &model::<E>(&old, &steady, start, fade, 0, 0),
            0,
            PTOTAL,
        );
        println!(
            "W264 delayed priming: fade start {start} ({delayed} delayed blocks, bound {bound}), max error vs never-switched {error:.3e}"
        );
        assert!(error < 1e-12);
    }
}

#[test]
fn mid_fade_policies_and_reset_apply_while_priming() {
    type E = ConvolutionBank64;
    let p = P::ZeroLatency { head: 16 };
    let (a, b, c) = (response(1, 100), response(2, 120), response(3, 140));
    let input = noise::<E>(4, 8192);
    let delayed = continuity(prime(1024, 100), AlignmentPolicy::None);
    let mut out = vec![vec![0.; MAX]; 2];
    let block = |audio: &mut ConvolutionTransition<E>, out: &mut Vec<Vec<f64>>, at: usize| {
        let (l, r) = out.split_at_mut(1);
        audio
            .process(
                &[&input[0][at..at + MAX], &input[1][at..at + MAX]],
                &mut [&mut l[0][..], &mut r[0][..]],
                MAX,
            )
            .unwrap()
    };
    // Refuse: a proposal offered during a delayed fade is returned unused.
    let opts = options(100, CrossfadeLaw::EqualGain, MidFadePolicy::Refuse);
    let (mut control, mut audio) =
        convolution_transition_with_continuity(E::make(&a, p), MAX, opts, delayed).unwrap();
    for at in (0..2048).step_by(MAX) {
        block(&mut audio, &mut out, at);
    }
    control.offer(E::make(&b, p)).unwrap();
    let status = block(&mut audio, &mut out, 2048);
    assert!(status.priming);
    assert_eq!(audio.priming_sequence(), Some(1));
    assert_eq!(audio.priming_lag(), Some(1024 - 100 + MAX));
    control.offer(E::make(&c, p)).unwrap();
    block(&mut audio, &mut out, 2048 + MAX);
    let returned = control.take_returned().unwrap();
    assert_eq!(
        (returned.reason, returned.sequence),
        (ReturnReason::Refused, 2)
    );
    assert_eq!(audio.current_sequence(), 0);
    // Reset while priming: history is forgotten and the fade starts next block.
    audio.reset();
    assert_eq!(audio.priming_lag(), Some(0));
    let status = block(&mut audio, &mut out, 2048 + 2 * MAX);
    assert_eq!(status.started, Some(1));
    assert_eq!(status.primed_frames, 0);
    drop(audio);
    // QueueOne: the second proposal waits in the handoff until after the fade.
    let opts = options(100, CrossfadeLaw::EqualGain, MidFadePolicy::QueueOne);
    let (mut control, mut audio) =
        convolution_transition_with_continuity(E::make(&a, p), MAX, opts, delayed).unwrap();
    for at in (0..2048).step_by(MAX) {
        block(&mut audio, &mut out, at);
    }
    control.offer(E::make(&b, p)).unwrap();
    block(&mut audio, &mut out, 2048);
    control.offer(E::make(&c, p)).unwrap();
    assert_eq!(
        control.offer(E::make(&c, p)).unwrap_err().reason,
        TransitionOfferFailure::Full
    );
    let mut starts = Vec::new();
    for at in (2048 + MAX..8192).step_by(MAX) {
        if let Some(s) = block(&mut audio, &mut out, at).started {
            starts.push(s);
        }
    }
    assert_eq!(starts, vec![1, 2]);
}

#[test]
fn priming_completion_with_a_cut_starts_one_proposal_per_boundary() {
    type E = ConvolutionBank64;
    let p = P::ZeroLatency { head: 16 };
    let input = noise::<E>(8, 4096);
    let opts = options(0, CrossfadeLaw::EqualGain, MidFadePolicy::QueueOne);
    let policies = continuity(prime(512, 100), AlignmentPolicy::None);
    let (mut control, mut audio) =
        convolution_transition_with_continuity(E::make(&response(1, 50), p), MAX, opts, policies)
            .unwrap();
    let mut out = vec![vec![0.; MAX]; 2];
    let mut starts = Vec::new();
    for (index, at) in (0..4096).step_by(MAX).enumerate() {
        if index == 10 {
            control.offer(E::make(&response(2, 50), p)).unwrap();
        }
        if index == 11 {
            control.offer(E::make(&response(3, 50), p)).unwrap();
        }
        let (l, r) = out.split_at_mut(1);
        let status = audio
            .process(
                &[&input[0][at..at + MAX], &input[1][at..at + MAX]],
                &mut [&mut l[0][..], &mut r[0][..]],
                MAX,
            )
            .unwrap();
        if let Some(sequence) = status.started {
            assert_eq!(status.completed, Some(sequence));
            starts.push((index, sequence));
        }
        if index == 12 {
            // Forget the history: 1 completes priming at the next boundary, and 2
            // (queued) could then prime instantly from an empty ring.
            assert!(audio.priming_sequence() == Some(1));
            audio.reset();
        }
    }
    // One start per boundary: 1 cuts in at block 13, 2 at block 14.
    assert_eq!(starts, vec![(13, 1), (14, 2)]);
    assert_eq!(control.reclaim(), 2);
}

// ---- Latency alignment ----

const AF: usize = 1536;
const AL: usize = 300;
const ATOTAL: usize = 3072;

fn alignment_case<E: Kind>() -> (f64, f64) {
    let a = response(41, 200);
    let b = response(42, 333);
    let input = noise::<E>(43, ATOTAL);
    let zero = P::ZeroLatency { head: 16 };
    let d32 = P::UniformDelayed { partition: 32 };
    let d64 = P::UniformDelayed { partition: 64 };
    let mut unaligned_error = 0f64;
    let mut aligned_error = 0f64;
    for (pa, pb, la, lb) in [
        (zero, d32, 0usize, 32usize),
        (d64, zero, 64, 0),
        (d32, d64, 32, 64),
        (d64, d32, 64, 32),
    ] {
        for law in [CrossfadeLaw::EqualGain, CrossfadeLaw::EqualPower] {
            for frames in [AL, 0] {
                let fade = CrossfadePolicy { frames, law };
                let opts = options(frames, law, MidFadePolicy::Refuse);
                let old = solo::<E>(&a, pa, &input, 0);
                let (d_old, d_new) = if lb >= la { (lb - la, 0) } else { (0, la - lb) };
                let (d_old, d_new) = if frames == 0 { (0, 0) } else { (d_old, d_new) };
                let change_frame = if lb > la || frames == 0 {
                    AF
                } else {
                    AF + frames
                };
                for primed in [false, true] {
                    let (policies, new_start) = if primed {
                        (continuity(prime(1024, 1024), align(64)), AF - 1024)
                    } else {
                        (continuity(PrimingPolicy::None, align(64)), AF)
                    };
                    let new = solo::<E>(&b, pb, &input, new_start);
                    let expected = model::<E>(&old, &new, AF, fade, d_old, d_new);
                    for blocks in segmentations(AF, ATOTAL) {
                        let run = drive(
                            E::make(&a, pa),
                            opts,
                            policies,
                            &input,
                            &blocks,
                            vec![(AF, E::make(&b, pb))],
                        );
                        assert_same(&run.output, &expected, E::bits, "aligned");
                        aligned_error =
                            aligned_error.max(max_error::<E>(&run.output, &expected, 0, ATOTAL));
                        assert_eq!(run.output_latency, Some(lb));
                        let changes: Vec<_> = run
                            .reports
                            .iter()
                            .filter_map(|r| match r.event {
                                TransitionEvent::OutputLatencyChanged {
                                    previous_latency,
                                    latency,
                                } => Some((r.frame, previous_latency, latency)),
                                _ => None,
                            })
                            .collect();
                        assert_eq!(changes, vec![(change_frame as u64, la, lb)]);
                        let flagged: Vec<_> = run
                            .statuses
                            .iter()
                            .filter(|(_, s)| s.output_latency_change.is_some())
                            .map(|(at, s)| (*at, s.output_latency_change.unwrap()))
                            .collect();
                        assert_eq!(flagged.len(), 1);
                        let (at, change) = flagged[0];
                        assert_eq!(change, (la, lb));
                        let n = blocks[run.statuses.iter().position(|s| s.0 == at).unwrap()];
                        assert!(at <= change_frame && change_frame <= at + n);
                    }
                    if frames > 0 && !primed {
                        // The default policy mixes misaligned paths during the fade.
                        let run = drive(
                            E::make(&a, pa),
                            opts,
                            TransitionContinuity::NONE,
                            &input,
                            &uniform(ATOTAL, MAX),
                            vec![(AF, E::make(&b, pb))],
                        );
                        let error = max_error::<E>(&run.output, &expected, AF, AF + frames);
                        unaligned_error = unaligned_error.max(error / peak::<E>(&expected));
                    }
                }
            }
        }
    }
    (aligned_error, unaligned_error)
}

#[test]
fn alignment_matches_independent_reference_sample_exactly_f64() {
    let (aligned, unaligned) = alignment_case::<ConvolutionBank64>();
    println!(
        "W264 alignment f64: aligned max error {aligned:e}; unaligned default max error {unaligned:.3} of peak during the fade"
    );
    assert_eq!(aligned, 0.);
    assert!(unaligned > 0.1);
}
#[test]
fn alignment_matches_independent_reference_sample_exactly_f32() {
    let (aligned, unaligned) = alignment_case::<ConvolutionBank32>();
    println!(
        "W264 alignment f32: aligned max error {aligned:e}; unaligned default max error {unaligned:.3} of peak during the fade"
    );
    assert_eq!(aligned, 0.);
    assert!(unaligned > 0.1);
}

#[test]
fn reported_output_latency_matches_measured_impulse_onset() {
    type E = ConvolutionBank64;
    const TOTAL: usize = 4096;
    let unit = [1.0];
    let zero = P::ZeroLatency { head: 16 };
    let d16 = P::UniformDelayed { partition: 16 };
    let d64 = P::UniformDelayed { partition: 64 };
    // Impulses every 97 frames (longer than the largest latency, coprime with blocks).
    let impulses: Vec<usize> = (5..TOTAL - 128).step_by(97).collect();
    let mut input = vec![vec![0.; TOTAL]; 2];
    for &q in &impulses {
        input[0][q] = 1.;
        input[1][q] = 1.;
    }
    let offers = |_: ()| {
        vec![
            (1024, E::make(&unit, d64)),  // 0 -> 64 (old delayed)
            (2048, E::make(&unit, d16)),  // 64 -> 16 (new delayed)
            (3072, E::make(&unit, zero)), // 16 -> 0
        ]
    };
    let mut measured = 0;
    let mut worst = 0usize;
    for blocks in [
        uniform(TOTAL, MAX),
        random_blocks(7, &[1024, 2048, 3072], TOTAL),
    ] {
        for law in [CrossfadeLaw::EqualGain, CrossfadeLaw::EqualPower] {
            let opts = options(400, law, MidFadePolicy::Refuse);
            let run = drive(
                E::make(&unit, zero),
                opts,
                // Priming lets the new path see impulses just before its start.
                continuity(prime(256, 256), align(64)),
                &input,
                &blocks,
                offers(()),
            );
            let mut changes = vec![(0u64, 0usize)];
            for r in &run.reports {
                if let TransitionEvent::OutputLatencyChanged { latency, .. } = r.event {
                    changes.push((r.frame, latency));
                }
            }
            assert_eq!(
                changes,
                vec![(0, 0), (1024, 64), (2448, 16), (3472, 0)],
                "{law:?}"
            );
            let latency_at = |o: usize| {
                changes
                    .iter()
                    .rev()
                    .find(|(f, _)| *f as usize <= o)
                    .unwrap()
                    .1
            };
            for &q in &impulses {
                // Skip impulses within the largest delay of a latency change, whose
                // response is repeated or skipped by the documented jump.
                if changes
                    .iter()
                    .skip(1)
                    .any(|(f, _)| (*f as usize).abs_diff(q) <= 64)
                {
                    continue;
                }
                let onsets: Vec<usize> = (q..q + 97)
                    .filter(|&o| run.output[0][o].abs() > 0.25)
                    .collect();
                assert_eq!(onsets.len(), 1, "one aligned onset for impulse {q}");
                let o = onsets[0];
                assert_eq!(o - q, latency_at(o), "impulse {q}");
                // Both aligned paths sum to the full impulse under equal gain.
                if law == CrossfadeLaw::EqualGain {
                    assert!(
                        (run.output[0][o] - 1.).abs() < 1e-12,
                        "impulse {q} onset {o} value {}",
                        run.output[0][o]
                    );
                }
                worst = worst.max((o - q).abs_diff(latency_at(o)));
                measured += 1;
            }
        }
    }
    // Contrast: the default policy shows two onsets for impulses during a fade.
    let opts = options(400, CrossfadeLaw::EqualGain, MidFadePolicy::Refuse);
    let run = drive(
        E::make(&unit, zero),
        opts,
        TransitionContinuity::NONE,
        &input,
        &uniform(TOTAL, MAX),
        offers(()),
    );
    let doubled = impulses
        .iter()
        .filter(|&&q| (1024..1024 + 400 - 64).contains(&q))
        .filter(|&&q| {
            (q..q + 97)
                .filter(|&o| run.output[0][o].abs() > 0.05)
                .count()
                == 2
        })
        .count();
    println!(
        "W264 impulse onsets: {measured} measured, worst |onset - reported latency| = {worst}; default policy doubled onsets during the first fade: {doubled}"
    );
    assert!(measured > 100);
    assert_eq!(worst, 0);
    assert!(doubled >= 2);
}

// ---- Refusals ----

fn untouched(engine: &ConvolutionBank64) -> bool {
    (0..engine.channels()).all(|c| {
        engine
            .engine(c)
            .unwrap()
            .stage_work()
            .iter()
            .flatten()
            .all(|w| w.completed_blocks == 0 && w.pending_source_frames == 0)
    })
}

#[test]
fn setup_refusals_return_the_identical_engine_without_allocating() {
    type E = ConvolutionBank64;
    let p = P::ZeroLatency { head: 16 };
    let opts = options(100, CrossfadeLaw::EqualGain, MidFadePolicy::Refuse);
    for (policies, reason) in [
        (
            continuity(prime(0, 10), AlignmentPolicy::None),
            TransitionSetupFailure::Priming,
        ),
        (
            continuity(
                prime(TRANSITION_MAXIMUM_PRIMING_FRAMES + 1, 1 << 21),
                AlignmentPolicy::None,
            ),
            TransitionSetupFailure::Priming,
        ),
        (
            continuity(prime(100, 0), AlignmentPolicy::None),
            TransitionSetupFailure::Priming,
        ),
        // Below the history and not above the maximum block: could never catch up.
        (
            continuity(prime(1000, MAX), AlignmentPolicy::None),
            TransitionSetupFailure::Priming,
        ),
        (
            continuity(
                PrimingPolicy::None,
                align(TRANSITION_MAXIMUM_ALIGNMENT_FRAMES + 1),
            ),
            TransitionSetupFailure::Alignment,
        ),
    ] {
        let engine = E::make(&response(1, 50), p);
        let address = &*engine as *const E;
        let mut error = None;
        let info = allocation_counter::measure(|| {
            error = Some(
                convolution_transition_with_continuity(engine, MAX, opts, policies).unwrap_err(),
            );
        });
        let error = error.unwrap();
        assert_eq!(error.reason, reason);
        assert_eq!(&*error.engine as *const E, address);
        assert!(untouched(&error.engine));
        assert_eq!(info.count_total, 0, "{reason:?} refused before allocating");
    }
    // Storage cap: 64 channels x (2^20 history + 2 x (2^20 + 64)) f64 samples.
    let wide: Vec<Vec<f64>> = (0..64).map(|c| response(c, 8)).collect();
    let refs: Vec<&[f64]> = wide.iter().map(|v| v.as_slice()).collect();
    let engine = Box::new(ConvolutionBank64::new(&refs, p, MAX, BUDGET).unwrap());
    let address = &*engine as *const E;
    let policies = continuity(
        prime(
            TRANSITION_MAXIMUM_PRIMING_FRAMES,
            TRANSITION_MAXIMUM_PRIMING_FRAMES,
        ),
        align(TRANSITION_MAXIMUM_ALIGNMENT_FRAMES),
    );
    let mut error = None;
    let info = allocation_counter::measure(|| {
        error =
            Some(convolution_transition_with_continuity(engine, MAX, opts, policies).unwrap_err());
    });
    let error = error.unwrap();
    assert_eq!(error.reason, TransitionSetupFailure::Storage);
    assert_eq!(&*error.engine as *const E, address);
    assert_eq!(info.count_total, 0);
}

#[test]
fn alignment_refusal_returns_the_engine_unprocessed_and_leaves_state_unchanged() {
    type E = ConvolutionBank64;
    let (a, b) = (response(1, 150), response(2, 170));
    let zero = P::ZeroLatency { head: 16 };
    let d32 = P::UniformDelayed { partition: 32 };
    let input = noise::<E>(4, 2048);
    let opts = options(200, CrossfadeLaw::EqualPower, MidFadePolicy::QueueOne);
    let policies = continuity(prime(256, 256), align(16));
    let (mut control, mut audio) =
        convolution_transition_with_continuity(E::make(&a, zero), MAX, opts, policies).unwrap();
    let never = solo::<E>(&a, zero, &input, 0);
    let mut out = vec![vec![0.; 2048]; 2];
    let mut block = |audio: &mut ConvolutionTransition<E>, at: usize| {
        let (l, r) = out.split_at_mut(1);
        audio
            .process(
                &[&input[0][at..at + MAX], &input[1][at..at + MAX]],
                &mut [&mut l[0][at..at + MAX], &mut r[0][at..at + MAX]],
                MAX,
            )
            .unwrap()
    };
    for at in (0..512).step_by(MAX) {
        block(&mut audio, at);
    }
    // Difference 32 > 16: refused at the start boundary through a reserved slot.
    let candidate = E::make(&b, d32);
    let address = &*candidate as *const E;
    let sequence = control.offer(candidate).unwrap();
    let status = block(&mut audio, 512);
    assert_eq!(status.alignment_refused, Some(sequence));
    assert_eq!(status.started, None);
    assert_eq!(status.primed_frames, 0);
    let returned = control.take_returned().unwrap();
    assert_eq!(returned.reason, ReturnReason::Refused);
    assert_eq!(returned.sequence, sequence);
    assert_eq!(&*returned.engine as *const E, address);
    assert!(untouched(&returned.engine));
    assert_eq!(
        control.take_report().unwrap().event,
        TransitionEvent::AlignmentRefused {
            previous_latency: 0,
            latency: 32
        }
    );
    assert_eq!(audio.current_sequence(), 0);
    assert_eq!(audio.output_latency_samples(), Some(0));
    assert_eq!(audio.priming_sequence(), None);
    // Backpressure: with the returned queue full, the refusal waits in place and
    // nothing starts or changes.
    let mut seq = sequence;
    let mut filled = 0;
    while filled < 4 {
        seq = control.offer(E::make(&b, d32)).unwrap();
        let at = 576 + filled * MAX;
        let status = block(&mut audio, at);
        assert_eq!(status.alignment_refused, Some(seq));
        filled += 1;
    }
    let blocked = control.offer(E::make(&b, d32)).unwrap();
    for at in (832..1024).step_by(MAX) {
        let status = block(&mut audio, at);
        assert!(status.retirement_blocked);
        assert_eq!(status.alignment_refused, None);
        assert!(control.has_pending());
        assert_eq!(audio.current_sequence(), 0);
    }
    assert_eq!(control.reclaim(), 4);
    let status = block(&mut audio, 1024);
    assert_eq!(status.alignment_refused, Some(blocked));
    assert!(untouched(&control.take_returned().unwrap().engine));
    assert!(seq > sequence);
    // A refused process call takes nothing and changes no history or delay state.
    let short = vec![0.; 3];
    let (mut o, mut o2) = (vec![0.; MAX], vec![0.; MAX]);
    control.offer(E::make(&b, d32)).unwrap();
    assert!(
        audio
            .process(&[&short, &short], &mut [&mut o, &mut o2], MAX)
            .is_err()
    );
    assert!(control.has_pending());
    assert_eq!(control.take_returned().map(|r| r.sequence), None);
    for at in (1088..2048).step_by(MAX) {
        block(&mut audio, at);
    }
    // Output identical to a never-switched engine throughout.
    assert_same(&out, &never, E::bits, "refusals leave audio unchanged");
}

// ---- Allocation-free audio ----

#[test]
fn continuity_audio_side_has_zero_allocations() {
    type E = ConvolutionBank32;
    let schedules = [
        P::ZeroLatency { head: 16 },
        P::UniformDelayed { partition: 16 },
        P::UniformDelayed { partition: 32 },
        P::UniformDelayed { partition: 128 },
    ];
    let input = noise::<E>(6, 96 * MAX);
    let mut out = vec![vec![0f32; MAX]; 2];
    let mut audio_total = allocation_counter::AllocationInfo::default();
    let mut reclaim_total = allocation_counter::AllocationInfo::default();
    let mut refused = 0;
    let mut changes = 0;
    for mid in [
        MidFadePolicy::Refuse,
        MidFadePolicy::QueueOne,
        MidFadePolicy::LatestWins,
    ] {
        for (frames, priming) in [
            (0, prime(512, 512)),
            (100, prime(512, 100)),
            (100, prime(64, 64)),
        ] {
            let opts = options(frames, CrossfadeLaw::EqualPower, mid);
            let policies = continuity(priming, align(64));
            let (mut control, mut audio): (ConvolutionTransitionControl<E>, _) =
                convolution_transition_with_continuity(
                    E::make(&response(1, 300), schedules[0]),
                    MAX,
                    opts,
                    policies,
                )
                .unwrap();
            let mut rng = Rng(77);
            for at in (0..96 * MAX).step_by(MAX) {
                if !control.has_pending() {
                    let schedule = schedules[rng.below(4)];
                    let _ = control.offer(E::make(&response(rng.next(), 300), schedule));
                }
                let mut status = TransitionStatus::default();
                let mut empty = TransitionStatus::default();
                let (l, r) = out.split_at_mut(1);
                audio_total += allocation_counter::measure(|| {
                    // A zero-frame call handles the boundary (priming, refusal).
                    empty = audio
                        .process(
                            &[&input[0][..0], &input[1][..0]],
                            &mut [&mut l[0][..0], &mut r[0][..0]],
                            0,
                        )
                        .unwrap();
                    status = audio
                        .process(
                            &[&input[0][at..at + MAX], &input[1][at..at + MAX]],
                            &mut [&mut l[0][..], &mut r[0][..]],
                            MAX,
                        )
                        .unwrap();
                    if at == 40 * MAX {
                        audio.reset();
                    }
                });
                for s in [empty, status] {
                    refused += usize::from(s.alignment_refused.is_some());
                    changes += usize::from(s.output_latency_change.is_some());
                }
                if at >= 32 * MAX {
                    reclaim_total += allocation_counter::measure(|| {
                        control.reclaim();
                    });
                }
                while control.take_report().is_some() {}
            }
            drop(audio);
            drop(control);
        }
    }
    assert!(refused > 0 && changes > 0, "{refused} {changes}");
    assert_eq!(audio_total.count_total, 0);
    assert_eq!(audio_total.count_current, 0);
    assert_eq!(audio_total.bytes_total, 0);
    assert!(reclaim_total.count_current < 0);
    println!(
        "W264 allocation audio {audio_total:?} reclaim {reclaim_total:?} alignment refusals {refused} latency changes {changes}"
    );
}

// ---- Concurrent stress ----

#[test]
fn concurrent_priming_and_alignment_stress_with_deterministic_seeds() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    type E = ConvolutionBank32;
    const ACCEPTED: u64 = 32;
    let schedules = [
        (P::ZeroLatency { head: 16 }, 0usize),
        (P::UniformDelayed { partition: 16 }, 16),
        (P::UniformDelayed { partition: 32 }, 32),
        (P::UniformDelayed { partition: 128 }, 128),
    ];
    for seed in [0x5eed_0101u64, 0x5eed_0102, 0x5eed_0103] {
        for mid in [
            MidFadePolicy::Refuse,
            MidFadePolicy::QueueOne,
            MidFadePolicy::LatestWins,
        ] {
            let priming = if seed % 2 == 0 {
                prime(512, 512)
            } else {
                prime(512, 100)
            };
            let opts = options(517, CrossfadeLaw::EqualPower, mid);
            let policies = continuity(priming, align(64));
            let initial = E::make(&response(seed, 64), schedules[0].0);
            let (mut control, mut audio) =
                convolution_transition_with_continuity(initial, MAX, opts, policies).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let audio_stop = stop.clone();
            let audio_thread = std::thread::spawn(move || {
                let mut rng = Rng(seed ^ 0xa0d10);
                let mut input = vec![vec![0f32; MAX]; 2];
                let mut out = vec![vec![0f32; MAX]; 2];
                let mut peak = 0f32;
                let mut counts = [0u64; 4]; // blocks, starts, refusals, latency changes
                let mut primed_max = 0;
                let info = allocation_counter::measure(|| {
                    while !audio_stop.load(Ordering::Acquire) {
                        let n = 1 + rng.below(MAX);
                        for channel in &mut input {
                            for x in &mut channel[..n] {
                                *x = (rng.unit() * 0.9) as f32;
                            }
                        }
                        let (l, r) = out.split_at_mut(1);
                        let status = audio
                            .process(
                                &[&input[0][..n], &input[1][..n]],
                                &mut [&mut l[0][..n], &mut r[0][..n]],
                                n,
                            )
                            .unwrap();
                        counts[1] += u64::from(status.started.is_some());
                        counts[2] += u64::from(status.alignment_refused.is_some());
                        counts[3] += u64::from(status.output_latency_change.is_some());
                        primed_max = primed_max.max(status.primed_frames);
                        for channel in &out {
                            for y in &channel[..n] {
                                assert!(y.is_finite());
                                peak = peak.max(y.abs());
                            }
                        }
                        counts[0] += 1;
                        if counts[0].is_multiple_of(64) {
                            std::thread::yield_now();
                        }
                    }
                });
                (audio, info, peak, counts, primed_max)
            });
            let mut rng = Rng(seed);
            let mut accepted = 0u64;
            let mut returned = 0u64;
            let (mut refused, mut superseded, mut retired) = (0, 0, 0);
            let mut sequences = std::collections::BTreeSet::new();
            let mut held: Option<Box<E>> = None;
            let mut max_l1 = response(seed, 64).iter().map(|x| x.abs()).sum::<f64>();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            while returned < ACCEPTED {
                assert!(
                    std::time::Instant::now() < deadline,
                    "stress deadline {seed:x} {mid:?}"
                );
                if accepted < ACCEPTED && held.is_none() {
                    let h = response(rng.next(), 32 + rng.below(400));
                    max_l1 = max_l1.max(h.iter().map(|x| x.abs()).sum::<f64>());
                    held = Some(E::make(&h, schedules[rng.below(4)].0));
                }
                if let Some(engine) = held.take() {
                    match control.offer(engine) {
                        Ok(_) => accepted += 1,
                        Err(e) => {
                            assert_eq!(e.reason, TransitionOfferFailure::Full);
                            held = Some(e.engine);
                        }
                    }
                }
                while let Some(r) = control.take_returned() {
                    assert!(sequences.insert(r.sequence), "returned once");
                    match r.reason {
                        ReturnReason::Retired => retired += 1,
                        ReturnReason::Refused => refused += 1,
                        ReturnReason::Superseded => superseded += 1,
                    }
                    returned += 1;
                }
                while control.take_report().is_some() {}
                std::thread::yield_now();
            }
            stop.store(true, Ordering::Release);
            let (audio, info, peak, counts, primed_max) = audio_thread.join().unwrap();
            let [blocks, starts, alignment_refused, changes] = counts;
            assert_eq!(returned, accepted);
            assert!(!sequences.contains(&audio.current_sequence()));
            assert_eq!(audio.fade_position(), None);
            assert_eq!(audio.priming_sequence(), None);
            assert_eq!(audio.waiting_sequence(), None);
            assert_eq!(retired, starts);
            assert!(alignment_refused <= refused);
            if mid == MidFadePolicy::QueueOne {
                // Only alignment refusals can occur under QueueOne.
                assert_eq!((refused, superseded), (alignment_refused, 0));
            }
            // Outside a fade the output latency equals the current engine latency.
            assert_eq!(
                audio.output_latency_samples(),
                Some(audio.latency_samples())
            );
            assert!(primed_max <= if seed % 2 == 0 { 512 } else { 100 });
            assert_eq!(info.count_total, 0, "audio allocations {seed:x} {mid:?}");
            assert!(
                f64::from(peak) <= 0.9 * max_l1 * 2f64.sqrt() * (1. + 1e-5),
                "{peak} {max_l1}"
            );
            println!(
                "W264 stress seed {seed:x} {mid:?}: blocks {blocks} starts {starts} retired {retired} refused {refused} (alignment {alignment_refused}) superseded {superseded} latency changes {changes} primed max {primed_max} peak {peak}"
            );
            drop(audio);
            drop(control);
        }
    }
}

// ---- Default-path preservation against the ADR 0272 (W259) build ----

/// Default-constructor scenarios whose digests were recorded from the public
/// W259 build (this module is compiled unchanged against both trees). Each
/// digest covers output sample bits, the W259 status fields, offer results,
/// reports and returned engines, under every mid-fade policy, crossfade laws, a
/// cut, three block segmentations, a mid-fade reset, differing latencies and
/// returned-storage backpressure.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod w259_golden {
    use ravet_graph::{
        CrossfadeLaw, CrossfadePolicy, LatencyPolicy, MidFadePolicy, TransitionEngine,
        TransitionOptions, convolution_transition,
    };
    use ravet_signal::{ConvolutionBank32, ConvolutionBank64, ConvolutionPartitioning as P};

    const MAX: usize = 64;
    const TOTAL: usize = 2048;
    const BUDGET: usize = 8 << 20;

    struct Fnv(u64);
    impl Fnv {
        fn bytes(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 ^= u64::from(b);
                self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
            }
        }
        fn word(&mut self, v: u64) {
            self.bytes(&v.to_le_bytes());
        }
        fn text(&mut self, s: &str) {
            self.word(s.len() as u64);
            self.bytes(s.as_bytes());
        }
    }

    fn xorshift(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }
    fn unit(state: &mut u64) -> f64 {
        (xorshift(state) >> 11) as f64 / (1u64 << 53) as f64 * 2. - 1.
    }
    fn response(seed: u64, taps: usize) -> Vec<f64> {
        let mut s = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        (0..taps)
            .map(|i| unit(&mut s) * (-(i as f64) / (taps as f64 / 4.)).exp() * 0.3)
            .collect()
    }

    pub trait Golden: TransitionEngine + Sized {
        fn build(h: &[f64], p: P) -> Box<Self>;
        fn sample(x: f64) -> Self::Sample;
        fn bits(x: Self::Sample) -> u64;
    }
    impl Golden for ConvolutionBank64 {
        fn build(h: &[f64], p: P) -> Box<Self> {
            let r: Vec<f64> = h.iter().rev().map(|x| -0.5 * x).collect();
            Box::new(ConvolutionBank64::new(&[h, &r[..]], p, MAX, BUDGET).unwrap())
        }
        fn sample(x: f64) -> f64 {
            x
        }
        fn bits(x: f64) -> u64 {
            x.to_bits()
        }
    }
    impl Golden for ConvolutionBank32 {
        fn build(h: &[f64], p: P) -> Box<Self> {
            let l: Vec<f32> = h.iter().map(|&x| x as f32).collect();
            let r: Vec<f32> = h.iter().rev().map(|&x| (-0.5 * x) as f32).collect();
            Box::new(ConvolutionBank32::new(&[&l, &r], p, MAX, BUDGET).unwrap())
        }
        fn sample(x: f64) -> f32 {
            x as f32
        }
        fn bits(x: f32) -> u64 {
            u64::from(x.to_bits())
        }
    }

    fn blocks(kind: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut at = 0;
        let mut s = 0x5eed_0259u64;
        while at < TOTAL {
            let n = match kind {
                0 => MAX,
                1 => 37,
                _ => 1 + (xorshift(&mut s) % MAX as u64) as usize,
            }
            .min(TOTAL - at);
            out.push(n);
            at += n;
        }
        out
    }

    fn run<E: Golden>(
        mid: MidFadePolicy,
        law: CrossfadeLaw,
        fade: usize,
        kind: usize,
        drain: bool,
        seen: &mut std::collections::BTreeSet<String>,
    ) -> u64 {
        let options = TransitionOptions {
            crossfade: CrossfadePolicy { frames: fade, law },
            mid_fade: mid,
            latency: LatencyPolicy::Report,
        };
        let mut s = 0x1234_5678u64;
        let input: Vec<Vec<E::Sample>> = (0..2)
            .map(|_| (0..TOTAL).map(|_| E::sample(unit(&mut s) * 0.5)).collect())
            .collect();
        let (mut control, mut audio) = convolution_transition(
            E::build(&response(1, 200), P::ZeroLatency { head: 16 }),
            MAX,
            options,
        )
        .unwrap();
        let mut offers = vec![
            (448, response(2, 333), P::UniformDelayed { partition: 32 }),
            (512, response(3, 150), P::UniformDelayed { partition: 64 }),
            (576, response(4, 260), P::ZeroLatency { head: 32 }),
            (1400, response(5, 100), P::UniformDelayed { partition: 32 }),
            (1440, response(6, 120), P::ZeroLatency { head: 16 }),
            (1600, response(7, 90), P::ZeroLatency { head: 16 }),
            (1664, response(8, 100), P::ZeroLatency { head: 16 }),
            (1728, response(9, 110), P::ZeroLatency { head: 16 }),
            (1792, response(10, 120), P::ZeroLatency { head: 16 }),
            (1856, response(11, 130), P::ZeroLatency { head: 16 }),
            (1920, response(12, 140), P::ZeroLatency { head: 16 }),
        ];
        offers.reverse();
        let mut reset_done = false;
        let mut h = Fnv(0xcbf2_9ce4_8422_2325);
        let mut out = vec![vec![E::Sample::default(); MAX]; 2];
        let mut at = 0;
        for n in blocks(kind) {
            while offers.last().is_some_and(|o| o.0 <= at) {
                let (_, taps, p) = offers.pop().unwrap();
                match control.offer(E::build(&taps, p)) {
                    Ok(sequence) => h.word(sequence),
                    Err(e) => {
                        seen.insert(format!("offer {:?}", e.reason));
                        h.text(&format!("{:?}", e.reason));
                    }
                }
            }
            if !reset_done && at >= 640 {
                audio.reset();
                reset_done = true;
                h.text("reset");
            }
            let (l, r) = out.split_at_mut(1);
            let status = audio
                .process(
                    &[&input[0][at..at + n], &input[1][at..at + n]],
                    &mut [&mut l[0][..n], &mut r[0][..n]],
                    n,
                )
                .unwrap();
            for channel in &out {
                for &x in &channel[..n] {
                    h.word(E::bits(x));
                }
            }
            h.text(&format!(
                "{:?} {:?} {} {}",
                status.started, status.completed, status.fading, status.retirement_blocked
            ));
            if status.retirement_blocked {
                seen.insert("retirement_blocked".into());
            }
            while let Some(report) = control.take_report() {
                let kind = format!("{:?}", report.event);
                seen.insert(format!(
                    "event {}",
                    kind.split([' ', '{']).next().unwrap_or("")
                ));
                h.text(&format!(
                    "{} {:?} {} {}",
                    report.sequence, report.event, report.frame, report.lost_before
                ));
            }
            if drain {
                while let Some(x) = control.take_returned() {
                    seen.insert(format!("returned {:?}", x.reason));
                    h.text(&format!("{:?} {}", x.reason, x.sequence));
                }
            }
            at += n;
        }
        while let Some(x) = control.take_returned() {
            seen.insert(format!("returned {:?}", x.reason));
            h.text(&format!("{:?} {}", x.reason, x.sequence));
        }
        h.word(control.reclaim() as u64);
        h.0
    }

    /// Every scenario name with its digest, and the behaviours the scenarios reached.
    pub fn digests() -> (Vec<(String, u64)>, std::collections::BTreeSet<String>) {
        let mut all = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for (precision, sixty_four) in [("f64", true), ("f32", false)] {
            for mid in [
                MidFadePolicy::Refuse,
                MidFadePolicy::QueueOne,
                MidFadePolicy::LatestWins,
            ] {
                for (law, fade) in [
                    (CrossfadeLaw::EqualPower, 300),
                    (CrossfadeLaw::EqualGain, 300),
                    (CrossfadeLaw::EqualGain, 0),
                ] {
                    for kind in 0..3 {
                        for drain in [true, false] {
                            let digest = if sixty_four {
                                run::<ConvolutionBank64>(mid, law, fade, kind, drain, &mut seen)
                            } else {
                                run::<ConvolutionBank32>(mid, law, fade, kind, drain, &mut seen)
                            };
                            all.push((
                                format!(
                                    "{precision} {mid:?} {law:?} {fade} blocks{kind} drain={drain}"
                                ),
                                digest,
                            ));
                        }
                    }
                }
            }
        }
        (all, seen)
    }
}

/// Digests printed by the same module compiled against the public ADR 0272
/// build (`cargo test --test w259_golden_probe` on an unmodified public copy) on
/// macOS arm64, in the debug profile. f64 equal-power gains use the platform
/// `sin`/`cos`, whose results differ between profiles and platforms, so the
/// digests are recorded per profile and the test runs only where they were
/// recorded.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const W259_DIGESTS_DEBUG: [(&str, u64); 108] = [
    (
        "f64 Refuse EqualPower 300 blocks0 drain=true",
        0x226edb3f3128c41f,
    ),
    (
        "f64 Refuse EqualPower 300 blocks0 drain=false",
        0x9dbc7f854b6ef404,
    ),
    (
        "f64 Refuse EqualPower 300 blocks1 drain=true",
        0x8f64b4b44987f804,
    ),
    (
        "f64 Refuse EqualPower 300 blocks1 drain=false",
        0x6cf90289cc91e218,
    ),
    (
        "f64 Refuse EqualPower 300 blocks2 drain=true",
        0x2e1c98372d5f2286,
    ),
    (
        "f64 Refuse EqualPower 300 blocks2 drain=false",
        0xba0d5a120b8f20db,
    ),
    (
        "f64 Refuse EqualGain 300 blocks0 drain=true",
        0x6522eb6b0a2b65c9,
    ),
    (
        "f64 Refuse EqualGain 300 blocks0 drain=false",
        0x26ed3ab7e66d3528,
    ),
    (
        "f64 Refuse EqualGain 300 blocks1 drain=true",
        0xde162d386943a4b9,
    ),
    (
        "f64 Refuse EqualGain 300 blocks1 drain=false",
        0x4280ced85e1a730e,
    ),
    (
        "f64 Refuse EqualGain 300 blocks2 drain=true",
        0x075dc5085552dc00,
    ),
    (
        "f64 Refuse EqualGain 300 blocks2 drain=false",
        0x2e4ae805477a3d11,
    ),
    (
        "f64 Refuse EqualGain 0 blocks0 drain=true",
        0x8a3d5e14f420fbc7,
    ),
    (
        "f64 Refuse EqualGain 0 blocks0 drain=false",
        0xc145ac6a18358422,
    ),
    (
        "f64 Refuse EqualGain 0 blocks1 drain=true",
        0x12ac2168f917cbc3,
    ),
    (
        "f64 Refuse EqualGain 0 blocks1 drain=false",
        0x2d2123a234a0ccd6,
    ),
    (
        "f64 Refuse EqualGain 0 blocks2 drain=true",
        0xde0bc3aba20b409b,
    ),
    (
        "f64 Refuse EqualGain 0 blocks2 drain=false",
        0xf48dd073b8de5c01,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks0 drain=true",
        0x624d56b35b7d3d12,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks0 drain=false",
        0xa9d87341b353b90e,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks1 drain=true",
        0xdf2593836bc8489c,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks1 drain=false",
        0xe4d5aecb373d5ef4,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks2 drain=true",
        0x8d47ec316636474f,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks2 drain=false",
        0xee5283faddba3e11,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks0 drain=true",
        0xc3df2ebd262b8a3f,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks0 drain=false",
        0x970660a4c12c4bb1,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks1 drain=true",
        0x4f076c576af15120,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks1 drain=false",
        0xac79016d39ab3650,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks2 drain=true",
        0x0ffcaebc9d1e9012,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks2 drain=false",
        0x952cb1f45658e63a,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks0 drain=true",
        0x8a3d5e14f420fbc7,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks0 drain=false",
        0xc145ac6a18358422,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks1 drain=true",
        0x12ac2168f917cbc3,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks1 drain=false",
        0x2d2123a234a0ccd6,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks2 drain=true",
        0xde0bc3aba20b409b,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks2 drain=false",
        0xf48dd073b8de5c01,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks0 drain=true",
        0x82c099e684eccccb,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks0 drain=false",
        0xc9955709d10139d3,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks1 drain=true",
        0xe36a5c60f983f685,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks1 drain=false",
        0x91866809a8b24808,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks2 drain=true",
        0x4bbca7258048d367,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks2 drain=false",
        0xe60bee3c95f770d9,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks0 drain=true",
        0xb872d65316b9f18b,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks0 drain=false",
        0xee1aa52b7bae97b9,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks1 drain=true",
        0x3680cb8f810c986f,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks1 drain=false",
        0x20513571c86ab48b,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks2 drain=true",
        0x77f783d72882f9c3,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks2 drain=false",
        0xd4aff30972acd866,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks0 drain=true",
        0x8a3d5e14f420fbc7,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks0 drain=false",
        0xc145ac6a18358422,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks1 drain=true",
        0x12ac2168f917cbc3,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks1 drain=false",
        0x2d2123a234a0ccd6,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks2 drain=true",
        0xde0bc3aba20b409b,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks2 drain=false",
        0xf48dd073b8de5c01,
    ),
    (
        "f32 Refuse EqualPower 300 blocks0 drain=true",
        0x0d19dd6454da0d72,
    ),
    (
        "f32 Refuse EqualPower 300 blocks0 drain=false",
        0xae19c09647ecf76d,
    ),
    (
        "f32 Refuse EqualPower 300 blocks1 drain=true",
        0x5ce93309777a329a,
    ),
    (
        "f32 Refuse EqualPower 300 blocks1 drain=false",
        0x8bb08325014b9c40,
    ),
    (
        "f32 Refuse EqualPower 300 blocks2 drain=true",
        0xcea52339a82b2620,
    ),
    (
        "f32 Refuse EqualPower 300 blocks2 drain=false",
        0x2e7a86c8a45b87f6,
    ),
    (
        "f32 Refuse EqualGain 300 blocks0 drain=true",
        0x5dd1bbe0582f9ef8,
    ),
    (
        "f32 Refuse EqualGain 300 blocks0 drain=false",
        0xb0a96a6b9d0c7b52,
    ),
    (
        "f32 Refuse EqualGain 300 blocks1 drain=true",
        0xdd6e689e5b85aceb,
    ),
    (
        "f32 Refuse EqualGain 300 blocks1 drain=false",
        0x48659675fe0df59a,
    ),
    (
        "f32 Refuse EqualGain 300 blocks2 drain=true",
        0xc02d89fd038dc3da,
    ),
    (
        "f32 Refuse EqualGain 300 blocks2 drain=false",
        0x43372ffe8ceabd85,
    ),
    (
        "f32 Refuse EqualGain 0 blocks0 drain=true",
        0x649d1da54a67a152,
    ),
    (
        "f32 Refuse EqualGain 0 blocks0 drain=false",
        0xf80f29c567914473,
    ),
    (
        "f32 Refuse EqualGain 0 blocks1 drain=true",
        0xbe183ac52564ff41,
    ),
    (
        "f32 Refuse EqualGain 0 blocks1 drain=false",
        0x8968ddb01c76b6ac,
    ),
    (
        "f32 Refuse EqualGain 0 blocks2 drain=true",
        0xcfbc58359a0acbb7,
    ),
    (
        "f32 Refuse EqualGain 0 blocks2 drain=false",
        0x4609c30e085be050,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks0 drain=true",
        0xb5f7f31564619bbd,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks0 drain=false",
        0x15ac334c439c4d13,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks1 drain=true",
        0x4f01bc13418112c2,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks1 drain=false",
        0x45cc48bbc8c30c16,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks2 drain=true",
        0xc1cdd125e48dca94,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks2 drain=false",
        0x0286d60bba7110b4,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks0 drain=true",
        0x4aa3119e65f16167,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks0 drain=false",
        0x071d0990c4996f81,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks1 drain=true",
        0xb5111d83d112e00f,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks1 drain=false",
        0x6384774a06835379,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks2 drain=true",
        0xb839283e549b83c5,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks2 drain=false",
        0x7c41df6f839c64eb,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks0 drain=true",
        0x649d1da54a67a152,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks0 drain=false",
        0xf80f29c567914473,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks1 drain=true",
        0xbe183ac52564ff41,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks1 drain=false",
        0x8968ddb01c76b6ac,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks2 drain=true",
        0xcfbc58359a0acbb7,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks2 drain=false",
        0x4609c30e085be050,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks0 drain=true",
        0x3a4b0bf53bad7a77,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks0 drain=false",
        0x1a0fcc22acdbc5d5,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks1 drain=true",
        0x177e9ab29f91e0b9,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks1 drain=false",
        0x73aa6cd51126abee,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks2 drain=true",
        0xeaba529ad16c3c98,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks2 drain=false",
        0x2bea72d5bc4ad65c,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks0 drain=true",
        0xeafd4d07de688c95,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks0 drain=false",
        0xc5da0fa902e12378,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks1 drain=true",
        0x3dad185ecc361e2e,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks1 drain=false",
        0xd18b1bbe90605200,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks2 drain=true",
        0x8bd36265b1e5001c,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks2 drain=false",
        0xfd0db98dd59029bf,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks0 drain=true",
        0x649d1da54a67a152,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks0 drain=false",
        0xf80f29c567914473,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks1 drain=true",
        0xbe183ac52564ff41,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks1 drain=false",
        0x8968ddb01c76b6ac,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks2 drain=true",
        0xcfbc58359a0acbb7,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks2 drain=false",
        0x4609c30e085be050,
    ),
];
/// As [`W259_DIGESTS_DEBUG`], recorded in the release profile.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const W259_DIGESTS_RELEASE: [(&str, u64); 108] = [
    (
        "f64 Refuse EqualPower 300 blocks0 drain=true",
        0x692fea4191785807,
    ),
    (
        "f64 Refuse EqualPower 300 blocks0 drain=false",
        0x1e3b7aa13728f371,
    ),
    (
        "f64 Refuse EqualPower 300 blocks1 drain=true",
        0x53735a3f06a7b516,
    ),
    (
        "f64 Refuse EqualPower 300 blocks1 drain=false",
        0x806bb76ca9b13bbc,
    ),
    (
        "f64 Refuse EqualPower 300 blocks2 drain=true",
        0x5e2a71091d15f719,
    ),
    (
        "f64 Refuse EqualPower 300 blocks2 drain=false",
        0xa2e94f0812a3d82c,
    ),
    (
        "f64 Refuse EqualGain 300 blocks0 drain=true",
        0x6522eb6b0a2b65c9,
    ),
    (
        "f64 Refuse EqualGain 300 blocks0 drain=false",
        0x26ed3ab7e66d3528,
    ),
    (
        "f64 Refuse EqualGain 300 blocks1 drain=true",
        0xde162d386943a4b9,
    ),
    (
        "f64 Refuse EqualGain 300 blocks1 drain=false",
        0x4280ced85e1a730e,
    ),
    (
        "f64 Refuse EqualGain 300 blocks2 drain=true",
        0x075dc5085552dc00,
    ),
    (
        "f64 Refuse EqualGain 300 blocks2 drain=false",
        0x2e4ae805477a3d11,
    ),
    (
        "f64 Refuse EqualGain 0 blocks0 drain=true",
        0x8a3d5e14f420fbc7,
    ),
    (
        "f64 Refuse EqualGain 0 blocks0 drain=false",
        0xc145ac6a18358422,
    ),
    (
        "f64 Refuse EqualGain 0 blocks1 drain=true",
        0x12ac2168f917cbc3,
    ),
    (
        "f64 Refuse EqualGain 0 blocks1 drain=false",
        0x2d2123a234a0ccd6,
    ),
    (
        "f64 Refuse EqualGain 0 blocks2 drain=true",
        0xde0bc3aba20b409b,
    ),
    (
        "f64 Refuse EqualGain 0 blocks2 drain=false",
        0xf48dd073b8de5c01,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks0 drain=true",
        0x8f9b7972c1690244,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks0 drain=false",
        0xa17318ad669739c0,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks1 drain=true",
        0x50c8018af9d8cd5c,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks1 drain=false",
        0x3492f6f813cb3c40,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks2 drain=true",
        0xae0d0453faac4f04,
    ),
    (
        "f64 QueueOne EqualPower 300 blocks2 drain=false",
        0xef1233eb9618b8b6,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks0 drain=true",
        0xc3df2ebd262b8a3f,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks0 drain=false",
        0x970660a4c12c4bb1,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks1 drain=true",
        0x4f076c576af15120,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks1 drain=false",
        0xac79016d39ab3650,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks2 drain=true",
        0x0ffcaebc9d1e9012,
    ),
    (
        "f64 QueueOne EqualGain 300 blocks2 drain=false",
        0x952cb1f45658e63a,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks0 drain=true",
        0x8a3d5e14f420fbc7,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks0 drain=false",
        0xc145ac6a18358422,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks1 drain=true",
        0x12ac2168f917cbc3,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks1 drain=false",
        0x2d2123a234a0ccd6,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks2 drain=true",
        0xde0bc3aba20b409b,
    ),
    (
        "f64 QueueOne EqualGain 0 blocks2 drain=false",
        0xf48dd073b8de5c01,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks0 drain=true",
        0xfdf3bb0ecfdb6b14,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks0 drain=false",
        0x5cd78a25a4018ce9,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks1 drain=true",
        0xa4869fd22357159b,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks1 drain=false",
        0x51aa1a2fa021696c,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks2 drain=true",
        0xbd2042145d32eb14,
    ),
    (
        "f64 LatestWins EqualPower 300 blocks2 drain=false",
        0x6638afb2e598546e,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks0 drain=true",
        0xb872d65316b9f18b,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks0 drain=false",
        0xee1aa52b7bae97b9,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks1 drain=true",
        0x3680cb8f810c986f,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks1 drain=false",
        0x20513571c86ab48b,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks2 drain=true",
        0x77f783d72882f9c3,
    ),
    (
        "f64 LatestWins EqualGain 300 blocks2 drain=false",
        0xd4aff30972acd866,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks0 drain=true",
        0x8a3d5e14f420fbc7,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks0 drain=false",
        0xc145ac6a18358422,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks1 drain=true",
        0x12ac2168f917cbc3,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks1 drain=false",
        0x2d2123a234a0ccd6,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks2 drain=true",
        0xde0bc3aba20b409b,
    ),
    (
        "f64 LatestWins EqualGain 0 blocks2 drain=false",
        0xf48dd073b8de5c01,
    ),
    (
        "f32 Refuse EqualPower 300 blocks0 drain=true",
        0x0d19dd6454da0d72,
    ),
    (
        "f32 Refuse EqualPower 300 blocks0 drain=false",
        0xae19c09647ecf76d,
    ),
    (
        "f32 Refuse EqualPower 300 blocks1 drain=true",
        0x5ce93309777a329a,
    ),
    (
        "f32 Refuse EqualPower 300 blocks1 drain=false",
        0x8bb08325014b9c40,
    ),
    (
        "f32 Refuse EqualPower 300 blocks2 drain=true",
        0xcea52339a82b2620,
    ),
    (
        "f32 Refuse EqualPower 300 blocks2 drain=false",
        0x2e7a86c8a45b87f6,
    ),
    (
        "f32 Refuse EqualGain 300 blocks0 drain=true",
        0x5dd1bbe0582f9ef8,
    ),
    (
        "f32 Refuse EqualGain 300 blocks0 drain=false",
        0xb0a96a6b9d0c7b52,
    ),
    (
        "f32 Refuse EqualGain 300 blocks1 drain=true",
        0xdd6e689e5b85aceb,
    ),
    (
        "f32 Refuse EqualGain 300 blocks1 drain=false",
        0x48659675fe0df59a,
    ),
    (
        "f32 Refuse EqualGain 300 blocks2 drain=true",
        0xc02d89fd038dc3da,
    ),
    (
        "f32 Refuse EqualGain 300 blocks2 drain=false",
        0x43372ffe8ceabd85,
    ),
    (
        "f32 Refuse EqualGain 0 blocks0 drain=true",
        0x649d1da54a67a152,
    ),
    (
        "f32 Refuse EqualGain 0 blocks0 drain=false",
        0xf80f29c567914473,
    ),
    (
        "f32 Refuse EqualGain 0 blocks1 drain=true",
        0xbe183ac52564ff41,
    ),
    (
        "f32 Refuse EqualGain 0 blocks1 drain=false",
        0x8968ddb01c76b6ac,
    ),
    (
        "f32 Refuse EqualGain 0 blocks2 drain=true",
        0xcfbc58359a0acbb7,
    ),
    (
        "f32 Refuse EqualGain 0 blocks2 drain=false",
        0x4609c30e085be050,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks0 drain=true",
        0xb5f7f31564619bbd,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks0 drain=false",
        0x15ac334c439c4d13,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks1 drain=true",
        0x4f01bc13418112c2,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks1 drain=false",
        0x45cc48bbc8c30c16,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks2 drain=true",
        0xc1cdd125e48dca94,
    ),
    (
        "f32 QueueOne EqualPower 300 blocks2 drain=false",
        0x0286d60bba7110b4,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks0 drain=true",
        0x4aa3119e65f16167,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks0 drain=false",
        0x071d0990c4996f81,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks1 drain=true",
        0xb5111d83d112e00f,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks1 drain=false",
        0x6384774a06835379,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks2 drain=true",
        0xb839283e549b83c5,
    ),
    (
        "f32 QueueOne EqualGain 300 blocks2 drain=false",
        0x7c41df6f839c64eb,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks0 drain=true",
        0x649d1da54a67a152,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks0 drain=false",
        0xf80f29c567914473,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks1 drain=true",
        0xbe183ac52564ff41,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks1 drain=false",
        0x8968ddb01c76b6ac,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks2 drain=true",
        0xcfbc58359a0acbb7,
    ),
    (
        "f32 QueueOne EqualGain 0 blocks2 drain=false",
        0x4609c30e085be050,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks0 drain=true",
        0x3a4b0bf53bad7a77,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks0 drain=false",
        0x1a0fcc22acdbc5d5,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks1 drain=true",
        0x177e9ab29f91e0b9,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks1 drain=false",
        0x73aa6cd51126abee,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks2 drain=true",
        0xeaba529ad16c3c98,
    ),
    (
        "f32 LatestWins EqualPower 300 blocks2 drain=false",
        0x2bea72d5bc4ad65c,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks0 drain=true",
        0xeafd4d07de688c95,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks0 drain=false",
        0xc5da0fa902e12378,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks1 drain=true",
        0x3dad185ecc361e2e,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks1 drain=false",
        0xd18b1bbe90605200,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks2 drain=true",
        0x8bd36265b1e5001c,
    ),
    (
        "f32 LatestWins EqualGain 300 blocks2 drain=false",
        0xfd0db98dd59029bf,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks0 drain=true",
        0x649d1da54a67a152,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks0 drain=false",
        0xf80f29c567914473,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks1 drain=true",
        0xbe183ac52564ff41,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks1 drain=false",
        0x8968ddb01c76b6ac,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks2 drain=true",
        0xcfbc58359a0acbb7,
    ),
    (
        "f32 LatestWins EqualGain 0 blocks2 drain=false",
        0x4609c30e085be050,
    ),
];

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn default_constructor_matches_digests_recorded_from_the_w259_build() {
    let recorded = if cfg!(debug_assertions) {
        W259_DIGESTS_DEBUG
    } else {
        W259_DIGESTS_RELEASE
    };
    let (digests, seen) = w259_golden::digests();
    assert_eq!(digests.len(), recorded.len());
    for ((name, digest), (expected_name, expected)) in digests.iter().zip(recorded) {
        assert_eq!(name, expected_name);
        assert_eq!(
            *digest, expected,
            "{name}: default path differs from the W259 build"
        );
    }
    // The scenarios reach every W259 event, return reason and backpressure state.
    let expected: Vec<&str> = vec![
        "event Completed",
        "event Refused",
        "event Started",
        "event Superseded",
        "offer Full",
        "retirement_blocked",
        "returned Refused",
        "returned Retired",
        "returned Superseded",
    ];
    assert_eq!(
        seen.iter().map(String::as_str).collect::<Vec<_>>(),
        expected
    );
}

// ---- R2 review additions ----

#[test]
fn latest_wins_proposals_during_priming_supersede_in_offer_order() {
    type E = ConvolutionBank64;
    let p = P::ZeroLatency { head: 16 };
    let input = noise::<E>(51, 16384);
    let opts = options(100, CrossfadeLaw::EqualGain, MidFadePolicy::LatestWins);
    let delayed = continuity(prime(1024, 100), AlignmentPolicy::None);
    let block = |audio: &mut ConvolutionTransition<E>, at: usize| {
        let mut out = vec![vec![0.; MAX]; 2];
        let (l, r) = out.split_at_mut(1);
        audio
            .process(
                &[&input[0][at..at + MAX], &input[1][at..at + MAX]],
                &mut [&mut l[0][..], &mut r[0][..]],
                MAX,
            )
            .unwrap()
    };
    // Several proposals while 1 is being primed: each newer one supersedes the
    // waiting one, and only the newest starts after 1's fade.
    let (mut control, mut audio) =
        convolution_transition_with_continuity(E::make(&response(1, 80), p), MAX, opts, delayed)
            .unwrap();
    let mut at = 0;
    while at < 2048 {
        block(&mut audio, at);
        at += MAX;
    }
    let mut returned = Vec::new();
    let mut starts = Vec::new();
    for (index, seed) in [2u64, 3, 4, 5].into_iter().enumerate() {
        control.offer(E::make(&response(seed, 80), p)).unwrap();
        let status = block(&mut audio, at);
        at += MAX;
        assert!(status.priming, "1 is still priming");
        assert_eq!(audio.priming_sequence(), Some(1));
        if index == 0 {
            assert_eq!(status.primed_frames, 100);
        }
        while let Some(x) = control.take_returned() {
            returned.push((x.reason, x.sequence));
        }
    }
    // 1 is priming; 2 and 3 were superseded in offer order; 4 waits.
    assert_eq!(
        returned,
        vec![(ReturnReason::Superseded, 2), (ReturnReason::Superseded, 3)]
    );
    while at < 16384 {
        let status = block(&mut audio, at);
        at += MAX;
        if let Some(s) = status.started {
            starts.push(s);
        }
        while let Some(x) = control.take_returned() {
            returned.push((x.reason, x.sequence));
        }
    }
    assert_eq!(starts, vec![1, 4]);
    assert_eq!(
        returned,
        vec![
            (ReturnReason::Superseded, 2),
            (ReturnReason::Superseded, 3),
            (ReturnReason::Retired, 0),
            (ReturnReason::Retired, 1),
        ]
    );
    assert_eq!(audio.current_sequence(), 4);
    drop(audio);

    // A proposal pending at the boundary where priming completes is handled by
    // the mid-fade policy at that boundary: LatestWins moves it to waiting and it
    // starts after the fade; Refuse refuses it at that boundary.
    for mid in [MidFadePolicy::LatestWins, MidFadePolicy::Refuse] {
        let opts = options(100, CrossfadeLaw::EqualGain, mid);
        let (mut control, mut audio) = convolution_transition_with_continuity(
            E::make(&response(1, 80), p),
            MAX,
            opts,
            delayed,
        )
        .unwrap();
        let mut at = 0;
        while at < 2048 {
            block(&mut audio, at);
            at += MAX;
        }
        control.offer(E::make(&response(2, 80), p)).unwrap();
        block(&mut audio, at);
        at += MAX;
        // Advance until the remaining lag fits one priming step.
        while audio.priming_lag().is_some_and(|lag| lag > 100) {
            assert!(block(&mut audio, at).priming);
            at += MAX;
        }
        control.offer(E::make(&response(3, 80), p)).unwrap();
        let status = block(&mut audio, at);
        at += MAX;
        assert_eq!(status.started, Some(1), "priming completes and 1 starts");
        let refused = control.take_returned().map(|x| (x.reason, x.sequence));
        let mut later = Vec::new();
        while at < 8192 {
            if let Some(s) = block(&mut audio, at).started {
                later.push(s);
            }
            at += MAX;
        }
        if mid == MidFadePolicy::Refuse {
            assert_eq!(refused, Some((ReturnReason::Refused, 2)));
            assert_eq!(later, Vec::<u64>::new());
        } else {
            assert_eq!(refused, None);
            assert_eq!(later, vec![2]);
        }
    }
}

#[test]
fn priming_with_history_shorter_than_requested_primes_only_recorded_frames() {
    type E = ConvolutionBank64;
    const TOTAL: usize = 8192;
    let a = response(61, 300);
    let b = long_response(62, LONG);
    let p = P::NonUniform {
        head: 16,
        tail: 256,
    };
    let input = noise::<E>(63, TOTAL);
    let fade = CrossfadePolicy {
        frames: PL,
        law: CrossfadeLaw::EqualPower,
    };
    let opts = options(PL, fade.law, MidFadePolicy::Refuse);
    let policies = continuity(prime(LONG, LONG), AlignmentPolicy::None);
    // Before the ring fills: 1024 of 4096 frames recorded.
    const EARLY: usize = 1024;
    let expected = model::<E>(
        &solo::<E>(&a, p, &input, 0),
        &solo::<E>(&b, p, &input, 0),
        EARLY,
        fade,
        0,
        0,
    );
    // After a reset at RESET: the old engine restarts there, and the history
    // holds only the 640 frames recorded since.
    const RESET: usize = 2048;
    const LATE: usize = RESET + 640;
    let old_before = solo::<E>(&a, p, &input, 0);
    let old_after = solo::<E>(&a, p, &input, RESET);
    let old: Vec<Vec<f64>> = (0..2)
        .map(|c| {
            (0..TOTAL)
                .map(|t| {
                    if t < RESET {
                        old_before[c][t]
                    } else {
                        old_after[c][t]
                    }
                })
                .collect()
        })
        .collect();
    let expected_late = model::<E>(&old, &solo::<E>(&b, p, &input, RESET), LATE, fade, 0, 0);
    for split in [MAX, 32, 7] {
        let blocks = uniform_split(EARLY, TOTAL, split);
        let run = drive(
            E::make(&a, p),
            opts,
            policies,
            &input,
            &blocks,
            vec![(EARLY, E::make(&b, p))],
        );
        assert_same(&run.output, &expected, E::bits, "short history");
        let started: Vec<_> = run
            .statuses
            .iter()
            .filter(|s| s.1.started.is_some())
            .collect();
        assert_eq!((started[0].0, started[0].1.primed_frames), (EARLY, EARLY));
        assert_eq!(
            run.reports[0].event,
            TransitionEvent::PrimingStarted {
                history_frames: EARLY
            }
        );
        let blocks = if split == 7 {
            random_blocks(64, &[RESET, LATE], TOTAL)
        } else {
            uniform(TOTAL, split)
        };
        let run = drive_with_resets(
            E::make(&a, p),
            opts,
            policies,
            &input,
            &blocks,
            vec![(LATE, E::make(&b, p))],
            &[RESET],
        );
        assert_same(&run.output, &expected_late, E::bits, "history after reset");
        let started: Vec<_> = run
            .statuses
            .iter()
            .filter(|s| s.1.started.is_some())
            .collect();
        assert_eq!((started[0].0, started[0].1.primed_frames), (LATE, 640));
    }
}

/// `pre` before `reset`; afterwards `post`, with a path delayed by `delay`
/// reading zero until it has `delay` frames recorded since the reset.
fn delayed_with_reset<S: Copy + Default>(
    pre: &[S],
    post: &[S],
    reset: usize,
    delay: usize,
    t: usize,
) -> S {
    if t < reset {
        if t >= delay {
            pre[t - delay]
        } else {
            S::default()
        }
    } else if t - reset >= delay {
        post[t - delay]
    } else {
        S::default()
    }
}

#[test]
fn reset_during_an_aligned_fade_matches_an_independent_reference() {
    type E = ConvolutionBank64;
    const START: usize = 1536;
    const RESET: usize = START + 128;
    const TOTAL: usize = 3072;
    let (a, b) = (response(71, 200), response(72, 333));
    let input = noise::<E>(73, TOTAL);
    let zero = P::ZeroLatency { head: 16 };
    let d32 = P::UniformDelayed { partition: 32 };
    let fade = CrossfadePolicy {
        frames: AL,
        law: CrossfadeLaw::EqualPower,
    };
    let opts = options(AL, fade.law, MidFadePolicy::Refuse);
    // (old, new): increase delays the old path; decrease delays the new path.
    for (pa, pb, old_delay, new_delay) in [(zero, d32, 32, 0), (d32, zero, 0, 32)] {
        let old_pre = solo::<E>(&a, pa, &input, 0);
        let old_post = solo::<E>(&a, pa, &input, RESET);
        let new_pre = solo::<E>(&b, pb, &input, START);
        let new_post = solo::<E>(&b, pb, &input, RESET);
        // Fade contributions computed independently; model() then blends them.
        let mut old = vec![vec![0.; TOTAL]; 2];
        let mut new = vec![vec![0.; TOTAL]; 2];
        for c in 0..2 {
            for t in 0..TOTAL {
                let fading = (START..START + AL).contains(&t);
                old[c][t] = if fading {
                    delayed_with_reset(&old_pre[c], &old_post[c], RESET, old_delay, t)
                } else {
                    delayed_with_reset(&old_pre[c], &old_post[c], RESET, 0, t)
                };
                new[c][t] = if fading {
                    delayed_with_reset(&new_pre[c], &new_post[c], RESET, new_delay, t)
                } else {
                    delayed_with_reset(&new_pre[c], &new_post[c], RESET, 0, t)
                };
            }
        }
        let expected = model::<E>(&old, &new, START, fade, 0, 0);
        for blocks in [
            uniform(TOTAL, MAX),
            uniform(TOTAL, 16),
            random_blocks(74, &[START, RESET], TOTAL),
        ] {
            let run = drive_with_resets(
                E::make(&a, pa),
                opts,
                continuity(PrimingPolicy::None, align(64)),
                &input,
                &blocks,
                vec![(START, E::make(&b, pb))],
                &[RESET],
            );
            assert_same(&run.output, &expected, E::bits, "reset during aligned fade");
            assert_eq!(run.output_latency, Some(if new_delay > 0 { 0 } else { 32 }));
            let completed: Vec<_> = run
                .statuses
                .iter()
                .zip(&blocks)
                .filter(|(s, _)| s.1.completed == Some(1))
                .map(|(s, n)| (s.0, *n))
                .collect();
            assert_eq!(completed.len(), 1, "the fade position survives reset");
            let (at, n) = completed[0];
            assert!(at < START + AL && START + AL <= at + n);
        }
    }
}
