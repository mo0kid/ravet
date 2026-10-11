#![cfg(feature = "convolution-transitions")]
//! Live convolution transitions: exact blends against independently computed
//! references, block-size and partition identity, mid-fade policies, latency
//! reports, refusals, allocation-free audio handoff and a concurrent stress run.
use ravet_graph::{
    ConvolutionTransition, ConvolutionTransitionControl, CrossfadeLaw, CrossfadePolicy,
    LatencyPolicy, MidFadePolicy, ReturnReason, TransitionEngine, TransitionEvent,
    TransitionOfferFailure, TransitionOptions, TransitionReport, TransitionSetupFailure,
    TransitionStatus, convolution_transition,
};
use ravet_signal::{
    ConvolutionBank32, ConvolutionBank64, ConvolutionPartitioning as P, ConvolutionProcessError,
};
use std::f64::consts::FRAC_PI_2;

const MAX: usize = 64;
const BUDGET: usize = 8 << 20;

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
fn noise<E: Kind>(seed: u64, frames: usize) -> Vec<Vec<E::Sample>> {
    let mut rng = Rng(seed);
    (0..2)
        .map(|_| (0..frames).map(|_| E::sample(rng.unit() * 0.5)).collect())
        .collect()
}
fn options(
    frames: usize,
    law: CrossfadeLaw,
    mid: MidFadePolicy,
    latency: LatencyPolicy,
) -> TransitionOptions {
    TransitionOptions {
        crossfade: CrossfadePolicy { frames, law },
        mid_fade: mid,
        latency,
    }
}

// Run a freshly prepared engine over input[start..], chunked at MAX.
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

// Independently computed blend: segment i starts at `start_i` with fresh history;
// fades do not overlap. t = (k + 1) / L.
fn reference<E: Kind>(
    input: &[Vec<E::Sample>],
    segments: &[(usize, &[f64], P)],
    fade: CrossfadePolicy,
) -> Vec<Vec<E::Sample>> {
    let outputs: Vec<_> = segments
        .iter()
        .map(|(s, h, p)| solo::<E>(h, *p, input, *s))
        .collect();
    let total = input[0].len();
    let mut out = vec![vec![E::Sample::default(); total]; 2];
    for t in 0..total {
        let i = segments.iter().rposition(|(s, _, _)| *s <= t).unwrap();
        for c in 0..2 {
            let k = t - segments[i].0;
            out[c][t] = if i > 0 && k < fade.frames {
                let x = (k + 1) as f64 / fade.frames as f64;
                let (g_old, g_new) = match fade.law {
                    CrossfadeLaw::EqualGain => (1. - x, x),
                    CrossfadeLaw::EqualPower => ((x * FRAC_PI_2).cos(), (x * FRAC_PI_2).sin()),
                };
                E::gain(g_old) * outputs[i - 1][c][t] + E::gain(g_new) * outputs[i][c][t]
            } else {
                outputs[i][c][t]
            };
        }
    }
    out
}

struct Run<S> {
    output: Vec<Vec<S>>,
    statuses: Vec<(usize, TransitionStatus)>,
    offers: Vec<Result<u64, TransitionOfferFailure>>,
    returned: Vec<(ReturnReason, u64)>,
    reports: Vec<TransitionReport>,
}
fn drive<E: Kind>(
    initial: Box<E>,
    opts: TransitionOptions,
    input: &[Vec<E::Sample>],
    blocks: &[usize],
    mut offers: Vec<(usize, Box<E>)>,
) -> Run<E::Sample> {
    let (mut control, mut audio) = convolution_transition(initial, MAX, opts).unwrap();
    let total = input[0].len();
    let mut run = Run {
        output: vec![vec![E::Sample::default(); total]; 2],
        statuses: Vec::new(),
        offers: Vec::new(),
        returned: Vec::new(),
        reports: Vec::new(),
    };
    offers.reverse();
    let mut at = 0;
    for &n in blocks {
        while offers.last().is_some_and(|(f, _)| *f <= at) {
            let (f, engine) = offers.pop().unwrap();
            assert_eq!(f, at, "offers must align with block starts");
            run.offers.push(control.offer(engine).map_err(|e| e.reason));
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
    drop(audio);
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
fn assert_same<S: Copy>(a: &[Vec<S>], b: &[Vec<S>], bits: fn(S) -> u64, what: &str) {
    for c in 0..2 {
        for (t, (x, y)) in a[c].iter().zip(&b[c]).enumerate() {
            assert_eq!(bits(*x), bits(*y), "{what}: channel {c} frame {t}");
        }
    }
}

const F: usize = 448;
const L: usize = 300;
const TOTAL: usize = 1600;

fn crossfade_case<E: Kind>() {
    let a = response(11, 200);
    let b = response(12, 333);
    let input = noise::<E>(5, TOTAL);
    let pairs = [
        (
            P::ZeroLatency { head: 16 },
            P::NonUniform { head: 16, tail: 64 },
        ),
        (
            P::NonUniform {
                head: 32,
                tail: 128,
            },
            P::ZeroLatency { head: 64 },
        ),
        (
            P::UniformDelayed { partition: 32 },
            P::UniformDelayed { partition: 32 },
        ),
    ];
    for (pa, pb) in pairs {
        for law in [CrossfadeLaw::EqualGain, CrossfadeLaw::EqualPower] {
            let opts = options(L, law, MidFadePolicy::Refuse, LatencyPolicy::RequireEqual);
            let expected = reference::<E>(&input, &[(0, &a, pa), (F, &b, pb)], opts.crossfade);
            let segmentations = [
                uniform(TOTAL, MAX),
                uniform(TOTAL, 1),
                uniform(TOTAL, 7),
                random_blocks(99, &[F], TOTAL),
                random_blocks(1234, &[F], TOTAL),
            ];
            for blocks in segmentations {
                let run = drive(
                    E::make(&a, pa),
                    opts,
                    &input,
                    &blocks,
                    vec![(F, E::make(&b, pb))],
                );
                assert_same(&run.output, &expected, E::bits, "blend");
                assert_eq!(run.offers, vec![Ok(1)]);
                assert_eq!(run.returned, vec![(ReturnReason::Retired, 0)]);
                let started: Vec<_> = run
                    .statuses
                    .iter()
                    .filter(|s| s.1.started.is_some())
                    .collect();
                assert_eq!(started.len(), 1);
                assert_eq!(started[0].0, F);
                let completed: Vec<_> = run
                    .statuses
                    .iter()
                    .filter(|s| s.1.completed.is_some())
                    .collect();
                assert_eq!(completed.len(), 1);
                let latency = E::make(&a, pa).transition_latency();
                assert_eq!(
                    run.reports,
                    vec![
                        TransitionReport {
                            sequence: 1,
                            event: TransitionEvent::Started {
                                previous_latency: latency,
                                latency,
                                fade_frames: L
                            },
                            frame: F as u64,
                            lost_before: 0
                        },
                        TransitionReport {
                            sequence: 1,
                            event: TransitionEvent::Completed,
                            frame: (F + L) as u64,
                            lost_before: 0
                        },
                    ]
                );
            }
        }
    }
}

#[test]
fn crossfade_matches_independent_blend_for_every_segmentation_f64() {
    crossfade_case::<ConvolutionBank64>();
}
#[test]
fn crossfade_matches_independent_blend_for_every_segmentation_f32() {
    crossfade_case::<ConvolutionBank32>();
}

#[test]
fn equal_gain_and_equal_power_laws_have_documented_endpoints() {
    for law in [CrossfadeLaw::EqualGain, CrossfadeLaw::EqualPower] {
        assert_eq!(
            law.gains(299, 300),
            if law == CrossfadeLaw::EqualGain {
                (0., 1.)
            } else {
                (FRAC_PI_2.cos(), 1.)
            }
        );
        let (o, n) = law.gains(0, 1);
        assert_eq!(n, 1.);
        assert!(o.abs() < 1e-16);
    }
    let (o, n) = CrossfadeLaw::EqualPower.gains(149, 300);
    assert!((o * o + n * n - 1.).abs() < 1e-15);
}

#[test]
fn cut_transition_switches_at_the_block_boundary() {
    type E = ConvolutionBank64;
    let (a, b) = (response(1, 100), response(2, 150));
    let p = P::ZeroLatency { head: 16 };
    let input = noise::<E>(3, 1000);
    let opts = options(
        0,
        CrossfadeLaw::EqualGain,
        MidFadePolicy::Refuse,
        LatencyPolicy::RequireEqual,
    );
    let expected = reference::<E>(&input, &[(0, &a, p), (320, &b, p)], opts.crossfade);
    let run = drive(
        E::make(&a, p),
        opts,
        &input,
        &uniform(1000, 32),
        vec![(320, E::make(&b, p))],
    );
    assert_same(&run.output, &expected, E::bits, "cut");
    let s = run.statuses.iter().find(|s| s.0 == 320).unwrap().1;
    assert_eq!(
        (s.started, s.completed, s.fading),
        (Some(1), Some(1), false)
    );
    assert_eq!(run.returned, vec![(ReturnReason::Retired, 0)]);
}

#[test]
fn latency_changes_are_reported_or_refused() {
    type E = ConvolutionBank64;
    let (a, b) = (response(1, 100), response(2, 150));
    let zero = P::ZeroLatency { head: 16 };
    let delayed = P::UniformDelayed { partition: 32 };
    let input = noise::<E>(3, 1280);
    // Refused: the caller gets the identical engine back and audio is unchanged.
    let opts = options(
        L,
        CrossfadeLaw::EqualPower,
        MidFadePolicy::Refuse,
        LatencyPolicy::RequireEqual,
    );
    let (mut control, mut audio) = convolution_transition(E::make(&a, zero), MAX, opts).unwrap();
    let candidate = E::make(&b, delayed);
    let address = &*candidate as *const E;
    let error = control.offer(candidate).unwrap_err();
    assert_eq!(error.reason, TransitionOfferFailure::Latency);
    assert_eq!(&*error.engine as *const E, address);
    assert!(!control.has_pending());
    let mut out = vec![vec![0.0; 1280]; 2];
    for at in (0..1280).step_by(MAX) {
        let (l, r) = out.split_at_mut(1);
        audio
            .process(
                &[&input[0][at..at + MAX], &input[1][at..at + MAX]],
                &mut [&mut l[0][at..at + MAX], &mut r[0][at..at + MAX]],
                MAX,
            )
            .unwrap();
    }
    assert_same(
        &out,
        &solo::<E>(&a, zero, &input, 0),
        E::bits,
        "refused latency",
    );
    // Reported: the new latency is visible at the start, both during the fade.
    let opts = options(
        L,
        CrossfadeLaw::EqualPower,
        MidFadePolicy::Refuse,
        LatencyPolicy::Report,
    );
    let expected = reference::<E>(&input, &[(0, &a, zero), (F, &b, delayed)], opts.crossfade);
    let run = drive(
        E::make(&a, zero),
        opts,
        &input,
        &uniform(1280, MAX),
        vec![(F, E::make(&b, delayed))],
    );
    assert_same(&run.output, &expected, E::bits, "reported latency");
    assert_eq!(
        run.reports[0].event,
        TransitionEvent::Started {
            previous_latency: 0,
            latency: 32,
            fade_frames: L
        }
    );
    let (_control, mut audio) = convolution_transition(E::make(&a, zero), MAX, opts).unwrap();
    let mut control = _control;
    control.offer(E::make(&b, delayed)).unwrap();
    let mut o = vec![vec![0.0; MAX]; 2];
    let (l, r) = o.split_at_mut(1);
    audio
        .process(
            &[&input[0][..MAX], &input[1][..MAX]],
            &mut [&mut l[0][..], &mut r[0][..]],
            MAX,
        )
        .unwrap();
    assert_eq!(audio.latency_samples(), 32);
    assert_eq!(audio.outgoing_latency_samples(), Some(0));
    assert_eq!(audio.fade_position(), Some(MAX));
    assert_eq!(audio.tail_samples(), 149 + 32);
}

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
fn mid_fade_policies_refuse_queue_one_and_latest_wins() {
    type E = ConvolutionBank64;
    let p = P::ZeroLatency { head: 16 };
    let (a, b, c, d) = (
        response(1, 120),
        response(2, 140),
        response(3, 160),
        response(4, 180),
    );
    let input = noise::<E>(8, TOTAL);
    let blocks = uniform(TOTAL, MAX);
    let fade = |mid| options(L, CrossfadeLaw::EqualGain, mid, LatencyPolicy::RequireEqual);
    // Fade from F=448 ends at 748 inside block [704, 768); next boundary is 768.
    let next = 768;

    // Refuse: the mid-fade proposal comes back unused; output matches one transition.
    let run = drive(
        E::make(&a, p),
        fade(MidFadePolicy::Refuse),
        &input,
        &blocks,
        vec![(F, E::make(&b, p)), (F + MAX, E::make(&c, p))],
    );
    let expected = reference::<E>(
        &input,
        &[(0, &a, p), (F, &b, p)],
        fade(MidFadePolicy::Refuse).crossfade,
    );
    assert_same(&run.output, &expected, E::bits, "refuse");
    assert_eq!(run.offers, vec![Ok(1), Ok(2)]);
    assert_eq!(
        run.returned,
        vec![(ReturnReason::Refused, 2), (ReturnReason::Retired, 0)]
    );
    assert!(run.reports.iter().any(|r| r.sequence == 2
        && r.event == TransitionEvent::Refused
        && r.frame == (F + MAX) as u64));

    // QueueOne: second proposal waits in the one-slot handoff; a third is Full.
    let run = drive(
        E::make(&a, p),
        fade(MidFadePolicy::QueueOne),
        &input,
        &blocks,
        vec![
            (F, E::make(&b, p)),
            (F + MAX, E::make(&c, p)),
            (F + 2 * MAX, E::make(&d, p)),
        ],
    );
    let expected = reference::<E>(
        &input,
        &[(0, &a, p), (F, &b, p), (next, &c, p)],
        fade(MidFadePolicy::QueueOne).crossfade,
    );
    assert_same(&run.output, &expected, E::bits, "queue one");
    assert_eq!(
        run.offers,
        vec![Ok(1), Ok(2), Err(TransitionOfferFailure::Full)]
    );
    assert_eq!(
        run.returned,
        vec![(ReturnReason::Retired, 0), (ReturnReason::Retired, 1)]
    );

    // LatestWins: the waiting proposal is superseded by the newest one.
    let run = drive(
        E::make(&a, p),
        fade(MidFadePolicy::LatestWins),
        &input,
        &blocks,
        vec![
            (F, E::make(&b, p)),
            (F + MAX, E::make(&c, p)),
            (F + 2 * MAX, E::make(&d, p)),
        ],
    );
    let expected = reference::<E>(
        &input,
        &[(0, &a, p), (F, &b, p), (next, &d, p)],
        fade(MidFadePolicy::LatestWins).crossfade,
    );
    assert_same(&run.output, &expected, E::bits, "latest wins");
    assert_eq!(run.offers, vec![Ok(1), Ok(2), Ok(3)]);
    assert_eq!(
        run.returned,
        vec![
            (ReturnReason::Superseded, 2),
            (ReturnReason::Retired, 0),
            (ReturnReason::Retired, 1)
        ]
    );
    assert!(
        run.reports
            .iter()
            .any(|r| r.sequence == 2 && r.event == TransitionEvent::Superseded)
    );
}

#[test]
fn refused_and_superseded_engines_return_unprocessed() {
    type E = ConvolutionBank64;
    let p = P::NonUniform { head: 16, tail: 32 };
    let input = noise::<E>(8, 512);
    for mid in [MidFadePolicy::Refuse, MidFadePolicy::LatestWins] {
        let opts = options(
            400,
            CrossfadeLaw::EqualGain,
            mid,
            LatencyPolicy::RequireEqual,
        );
        let (mut control, mut audio) =
            convolution_transition(E::make(&response(1, 200), p), MAX, opts).unwrap();
        let mut out = vec![vec![0.0; MAX]; 2];
        let mut block = |audio: &mut ConvolutionTransition<E>, at: usize| {
            let (l, r) = out.split_at_mut(1);
            audio
                .process(
                    &[&input[0][at..at + MAX], &input[1][at..at + MAX]],
                    &mut [&mut l[0][..], &mut r[0][..]],
                    MAX,
                )
                .unwrap()
        };
        control.offer(E::make(&response(2, 200), p)).unwrap();
        block(&mut audio, 0);
        control.offer(E::make(&response(3, 200), p)).unwrap();
        block(&mut audio, MAX);
        if mid == MidFadePolicy::LatestWins {
            assert_eq!(audio.waiting_sequence(), Some(2));
            control.offer(E::make(&response(4, 200), p)).unwrap();
            block(&mut audio, 2 * MAX);
            assert_eq!(audio.waiting_sequence(), Some(3));
        }
        let returned = control.take_returned().unwrap();
        assert_eq!(returned.sequence, 2);
        assert!(untouched(&returned.engine));
    }
}

#[test]
fn refused_calls_and_offers_leave_state_unchanged() {
    type E = ConvolutionBank64;
    let p = P::ZeroLatency { head: 16 };
    let (a, b) = (response(1, 150), response(2, 170));
    let input = noise::<E>(4, 1024);
    let opts = options(
        200,
        CrossfadeLaw::EqualPower,
        MidFadePolicy::QueueOne,
        LatencyPolicy::RequireEqual,
    );
    // Setup refusals return the identical initial engine.
    for (max, frames, reason) in [
        (0, 10, TransitionSetupFailure::MaximumFrames),
        (MAX + 1, 10, TransitionSetupFailure::MaximumFrames),
        (MAX, (1 << 20) + 1, TransitionSetupFailure::FadeFrames),
    ] {
        let engine = E::make(&a, p);
        let address = &*engine as *const E;
        let mut o = opts;
        o.crossfade.frames = frames;
        let error = convolution_transition(engine, max, o).unwrap_err();
        assert_eq!(error.reason, reason);
        assert_eq!(&*error.engine as *const E, address);
    }
    let expected = reference::<E>(&input, &[(0, &a, p), (256, &b, p)], opts.crossfade);
    let (mut control, mut audio) = convolution_transition(E::make(&a, p), MAX, opts).unwrap();
    // Offer refusals: channel count and maximum frames.
    let mono = Box::new(ConvolutionBank64::new(&[&b], p, MAX, BUDGET).unwrap());
    assert_eq!(
        control.offer(mono).unwrap_err().reason,
        TransitionOfferFailure::Channels
    );
    let (b0, b1) = stereo(&b);
    let short = Box::new(ConvolutionBank64::new(&[&b0, &b1], p, MAX - 1, BUDGET).unwrap());
    assert_eq!(
        control.offer(short).unwrap_err().reason,
        TransitionOfferFailure::MaximumFrames
    );
    let mut out = vec![vec![0.0; 1024]; 2];
    let mut at = 0;
    while at < 1024 {
        if at == 256 {
            control.offer(E::make(&b, p)).unwrap();
        }
        let refused_at = [0, 256, 320];
        if refused_at.contains(&at) {
            // Refused calls while idle, with a pending proposal and mid-fade.
            let before = (
                audio.fade_position(),
                audio.current_sequence(),
                control.has_pending(),
            );
            let mut scratch = vec![vec![7.0; MAX + 1]; 2];
            let (l, r) = scratch.split_at_mut(1);
            let too_many = audio.process(
                &[&input[0][..MAX + 1], &input[1][..MAX + 1]],
                &mut [&mut l[0][..], &mut r[0][..]],
                MAX + 1,
            );
            assert_eq!(
                too_many.unwrap_err(),
                ConvolutionProcessError::TooManyFrames
            );
            let mismatch = audio.process(&[&input[0][..MAX]], &mut [&mut l[0][..MAX]], MAX);
            assert_eq!(
                mismatch.unwrap_err(),
                ConvolutionProcessError::ChannelMismatch
            );
            let short = audio.process(
                &[&input[0][..MAX], &input[1][..MAX - 1]],
                &mut [&mut l[0][..MAX], &mut r[0][..MAX]],
                MAX,
            );
            assert_eq!(short.unwrap_err(), ConvolutionProcessError::PrefixTooShort);
            assert!(scratch.iter().flatten().all(|x| *x == 7.0));
            assert_eq!(
                before,
                (
                    audio.fade_position(),
                    audio.current_sequence(),
                    control.has_pending()
                )
            );
        }
        let (l, r) = out.split_at_mut(1);
        audio
            .process(
                &[&input[0][at..at + MAX], &input[1][at..at + MAX]],
                &mut [&mut l[0][at..at + MAX], &mut r[0][at..at + MAX]],
                MAX,
            )
            .unwrap();
        at += MAX;
    }
    assert_same(&out, &expected, E::bits, "refused calls");
}

#[test]
fn reset_clears_both_engines_and_keeps_the_fade_position() {
    type E = ConvolutionBank64;
    let p = P::ZeroLatency { head: 16 };
    let input = noise::<E>(4, MAX);
    let opts = options(
        200,
        CrossfadeLaw::EqualGain,
        MidFadePolicy::Refuse,
        LatencyPolicy::RequireEqual,
    );
    let (mut control, mut audio) =
        convolution_transition(E::make(&response(1, 90), p), MAX, opts).unwrap();
    control.offer(E::make(&response(2, 90), p)).unwrap();
    let mut out = vec![vec![0.0; MAX]; 2];
    let (l, r) = out.split_at_mut(1);
    audio
        .process(
            &[&input[0][..], &input[1][..]],
            &mut [&mut l[0][..], &mut r[0][..]],
            MAX,
        )
        .unwrap();
    audio.reset();
    assert_eq!(audio.fade_position(), Some(MAX));
    let zeros = [0.0; MAX];
    audio
        .process(&[&zeros, &zeros], &mut [&mut l[0][..], &mut r[0][..]], MAX)
        .unwrap();
    assert!(out.iter().flatten().all(|x| *x == 0.0));
}

fn block<E: Kind>(
    audio: &mut ConvolutionTransition<E>,
    input: &[Vec<E::Sample>],
    out: &mut [Vec<E::Sample>],
    at: usize,
    n: usize,
) -> TransitionStatus {
    let (l, r) = out.split_at_mut(1);
    audio
        .process(
            &[&input[0][at..at + n], &input[1][at..at + n]],
            &mut [&mut l[0][..n], &mut r[0][..n]],
            n,
        )
        .unwrap()
}

#[test]
fn audio_side_has_zero_allocations_through_handoff_fade_retirement_and_backpressure() {
    type E = ConvolutionBank32;
    let p = P::NonUniform { head: 16, tail: 64 };
    let input = noise::<E>(6, 64 * MAX);
    let mut out = vec![vec![0f32; MAX]; 2];
    let mut audio_total = allocation_counter::AllocationInfo::default();
    let mut reclaim_total = allocation_counter::AllocationInfo::default();
    for mid in [
        MidFadePolicy::Refuse,
        MidFadePolicy::QueueOne,
        MidFadePolicy::LatestWins,
    ] {
        for frames in [0, 100] {
            let opts = options(
                frames,
                CrossfadeLaw::EqualPower,
                mid,
                LatencyPolicy::RequireEqual,
            );
            let (mut control, mut audio): (ConvolutionTransitionControl<E>, _) =
                convolution_transition(E::make(&response(1, 300), p), MAX, opts).unwrap();
            let mut seed = 10;
            let mut blocked = 0;
            for at in (0..64 * MAX).step_by(MAX) {
                // Offer every block without reclaiming for a while to exercise
                // deferred retirement and blocked starts.
                if !control.has_pending() {
                    seed += 1;
                    let _ = control.offer(E::make(&response(seed, 300), p));
                }
                let mut status = TransitionStatus::default();
                audio_total += allocation_counter::measure(|| {
                    status = block(&mut audio, &input, &mut out, at, MAX);
                    if at == 20 * MAX {
                        audio.reset();
                    }
                });
                blocked += usize::from(status.retirement_blocked);
                if at >= 32 * MAX {
                    reclaim_total += allocation_counter::measure(|| {
                        control.reclaim();
                    });
                }
                while control.take_report().is_some() {}
            }
            assert!(blocked > 0, "{mid:?} {frames}: backpressure exercised");
            drop(audio);
            drop(control);
        }
    }
    assert_eq!(audio_total.count_total, 0);
    assert_eq!(audio_total.count_current, 0);
    assert_eq!(audio_total.bytes_total, 0);
    assert_eq!(audio_total.bytes_current, 0);
    // Disposal happens on the control side: reclaim frees engines.
    assert!(reclaim_total.count_current < 0);
    println!("W259 allocation audio {audio_total:?} reclaim {reclaim_total:?}");
}

#[test]
fn concurrent_worker_and_audio_stress_with_deterministic_seeds() {
    use ravet_signal::{
        ImpulseResponseRequest, ResponseChannelMap, ResponseDelay, ResponseNormalization,
        ResponseRateConversion, ResponseRateGain, ResponseTrim, prepare_impulse_response,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    type E = ConvolutionBank32;
    const ACCEPTED: u64 = 32;
    for seed in [0x5eed_0001u64, 0x5eed_0002, 0x5eed_0003] {
        for mid in [
            MidFadePolicy::Refuse,
            MidFadePolicy::QueueOne,
            MidFadePolicy::LatestWins,
        ] {
            let law = if seed % 2 == 0 {
                CrossfadeLaw::EqualGain
            } else {
                CrossfadeLaw::EqualPower
            };
            let opts = options(1031, law, mid, LatencyPolicy::RequireEqual);
            let initial = E::make(&response(seed, 64), P::ZeroLatency { head: 16 });
            let (mut control, mut audio) = convolution_transition(initial, MAX, opts).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let audio_stop = stop.clone();
            let audio_thread = std::thread::spawn(move || {
                let mut rng = Rng(seed ^ 0xa0d10);
                let mut input = vec![vec![0f32; MAX]; 2];
                let mut out = vec![vec![0f32; MAX]; 2];
                let mut peak = 0f32;
                let mut blocks = 0u64;
                let mut starts = 0u64;
                let info = allocation_counter::measure(|| {
                    while !audio_stop.load(Ordering::Acquire) {
                        let n = 1 + rng.below(MAX);
                        for channel in &mut input {
                            for x in &mut channel[..n] {
                                *x = (rng.unit() * 0.9) as f32;
                            }
                        }
                        let status = block(&mut audio, &input, &mut out, 0, n);
                        starts += u64::from(status.started.is_some());
                        for channel in &out {
                            for y in &channel[..n] {
                                assert!(y.is_finite());
                                peak = peak.max(y.abs());
                            }
                        }
                        blocks += 1;
                        if blocks.is_multiple_of(64) {
                            std::thread::yield_now();
                        }
                    }
                });
                (audio, info, peak, blocks, starts)
            });
            // Worker: background preparation, offer, retry and disposal.
            let mut rng = Rng(seed);
            let mut accepted = 0u64;
            let mut returned = 0u64;
            let (mut refused, mut superseded, mut retired) = (0, 0, 0);
            let mut sequences = std::collections::BTreeSet::new();
            let mut held: Option<Box<E>> = None;
            let mut lost = 0;
            let mut started_reports = 0;
            let initial_l1 = 1.5 * response(seed, 64).iter().map(|x| x.abs()).sum::<f64>();
            let mut max_l1 = initial_l1;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            while returned < ACCEPTED {
                assert!(
                    std::time::Instant::now() < deadline,
                    "stress deadline {seed:x} {mid:?}"
                );
                if accepted < ACCEPTED && held.is_none() {
                    let taps = 32 + rng.below(400);
                    let source: Vec<f64> = (0..taps)
                        .map(|i| rng.unit() * (-(i as f64) / 60.).exp())
                        .collect();
                    let request = ImpulseResponseRequest {
                        target_rate: 48_000,
                        conversion: ResponseRateConversion::Sinc {
                            taps: 32,
                            gain: ResponseRateGain::PreserveResponseGain,
                            delay: ResponseDelay::RemoveWholeTargetFrames,
                        },
                        channels: ResponseChannelMap::Select(&[0, 0]),
                        trim: ResponseTrim::default(),
                        normalization: ResponseNormalization::Peak { target: 0.25 },
                        maximum_taps: 4096,
                        maximum_working_bytes: 4 << 20,
                    };
                    let prepared = prepare_impulse_response(44_100, &[&source], &request).unwrap();
                    let schedule = [
                        P::ZeroLatency { head: 16 },
                        P::NonUniform { head: 16, tail: 64 },
                        P::ZeroLatency { head: 32 },
                    ][rng.below(3)];
                    let l1 = (0..2)
                        .map(|c| {
                            prepared
                                .channel(c)
                                .unwrap()
                                .iter()
                                .map(|x| x.abs())
                                .sum::<f64>()
                        })
                        .fold(0., f64::max);
                    max_l1 = max_l1.max(l1);
                    held = Some(Box::new(prepared.bank32(schedule, MAX, BUDGET).unwrap()));
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
                    drop(r);
                }
                while let Some(r) = control.take_report() {
                    lost += r.lost_before;
                    started_reports +=
                        u64::from(matches!(r.event, TransitionEvent::Started { .. }));
                }
                std::thread::yield_now();
            }
            stop.store(true, Ordering::Release);
            let (audio, info, peak, blocks, starts) = audio_thread.join().unwrap();
            // Every engine except the final current one came back exactly once.
            assert_eq!(returned, accepted);
            assert!((1..=ACCEPTED).contains(&audio.current_sequence()));
            assert!(!sequences.contains(&audio.current_sequence()));
            assert_eq!(audio.fade_position(), None);
            assert_eq!(audio.waiting_sequence(), None);
            assert_eq!(retired, starts);
            if mid == MidFadePolicy::QueueOne {
                assert_eq!((refused, superseded), (0, 0));
            }
            assert!(started_reports <= starts);
            assert_eq!(info.count_total, 0, "audio allocations {seed:x} {mid:?}");
            assert_eq!(info.count_current, 0);
            // |x| <= 0.9 and the two fade gains sum to at most sqrt(2).
            assert!(
                f64::from(peak) <= 0.9 * max_l1 * 2f64.sqrt() * (1. + 1e-5),
                "{peak} {max_l1}"
            );
            println!(
                "W259 stress seed {seed:x} {mid:?}: blocks {blocks} starts {starts} retired {retired} refused {refused} superseded {superseded} lost reports {lost} peak {peak}"
            );
            drop(audio);
            drop(control);
        }
    }
}

// ---- R3 review follow-ups ----

#[test]
fn latest_wins_newer_pending_supersedes_stale_waiting_after_the_fade() {
    type E = ConvolutionBank64;
    let p = P::ZeroLatency { head: 16 };
    let (a, b, c, d) = (
        response(1, 120),
        response(2, 140),
        response(3, 160),
        response(4, 180),
    );
    let input = noise::<E>(8, TOTAL);
    let opts = options(
        L,
        CrossfadeLaw::EqualGain,
        MidFadePolicy::LatestWins,
        LatencyPolicy::RequireEqual,
    );
    // C waits from boundary 512; the fade ends inside [704, 768). D is offered
    // after the last mid-fade boundary (704), so at the post-fade boundary 768
    // both C (waiting) and D (pending) exist: D must win and C is superseded.
    let run = drive(
        E::make(&a, p),
        opts,
        &input,
        &uniform(TOTAL, MAX),
        vec![
            (F, E::make(&b, p)),
            (F + MAX, E::make(&c, p)),
            (768, E::make(&d, p)),
        ],
    );
    let expected = reference::<E>(
        &input,
        &[(0, &a, p), (F, &b, p), (768, &d, p)],
        opts.crossfade,
    );
    assert_same(&run.output, &expected, E::bits, "latest wins after fade");
    assert_eq!(run.offers, vec![Ok(1), Ok(2), Ok(3)]);
    assert_eq!(
        run.returned,
        vec![
            (ReturnReason::Retired, 0),
            (ReturnReason::Superseded, 2),
            (ReturnReason::Retired, 1)
        ]
    );
    let started: Vec<_> = run
        .statuses
        .iter()
        .filter_map(|s| s.1.started.map(|q| (s.0, q)))
        .collect();
    assert_eq!(started, vec![(F, 1), (768, 3)]);
    assert!(
        run.reports
            .iter()
            .any(|r| r.sequence == 2 && r.event == TransitionEvent::Superseded && r.frame == 768)
    );
}

#[test]
fn refuse_under_backpressure_returns_the_proposal_instead_of_starting_it() {
    type E = ConvolutionBank64;
    let p = P::ZeroLatency { head: 16 };
    let input = noise::<E>(9, 40 * MAX);
    let opts = options(
        100,
        CrossfadeLaw::EqualPower,
        MidFadePolicy::Refuse,
        LatencyPolicy::RequireEqual,
    );
    let (mut control, mut audio) =
        convolution_transition(E::make(&response(0, 90), p), MAX, opts).unwrap();
    let mut out = vec![vec![0.0; MAX]; 2];
    let mut at = 0;
    let mut step = |audio: &mut ConvolutionTransition<E>| {
        let s = block(audio, &input, &mut out, at, MAX);
        at += MAX;
        s
    };
    // Four completed transitions fill the four-slot returned queue (no reclaim).
    for seed in 1..=4u64 {
        control.offer(E::make(&response(seed, 90), p)).unwrap();
        for _ in 0..3 {
            step(&mut audio);
        }
    }
    assert_eq!(audio.current_sequence(), 4);
    // Fifth starts (deferred slot free); sixth arrives mid-fade with no slot.
    control.offer(E::make(&response(5, 90), p)).unwrap();
    assert_eq!(step(&mut audio).started, Some(5));
    control.offer(E::make(&response(6, 90), p)).unwrap();
    let s = step(&mut audio);
    assert!(s.retirement_blocked && s.started.is_none());
    for _ in 0..6 {
        let s = step(&mut audio);
        assert!(s.started.is_none(), "refused proposal must never start");
    }
    assert_eq!(audio.current_sequence(), 5);
    assert!(control.has_pending());
    // Free the queue: the deferred retirement flushes and #6 is returned Refused.
    assert_eq!(control.reclaim(), 4);
    let s = step(&mut audio);
    assert!(s.started.is_none());
    assert_eq!(audio.current_sequence(), 5);
    let returned: Vec<_> = std::iter::from_fn(|| control.take_returned()).collect();
    let summary: Vec<_> = returned.iter().map(|r| (r.reason, r.sequence)).collect();
    assert_eq!(
        summary,
        vec![(ReturnReason::Retired, 4), (ReturnReason::Refused, 6)]
    );
    assert!(untouched(&returned[1].engine));
    assert!(!control.has_pending());
}

#[test]
fn zero_frame_calls_take_proposals_without_rendering() {
    type E = ConvolutionBank64;
    let p = P::NonUniform { head: 16, tail: 64 };
    let (a, b) = (response(1, 150), response(2, 170));
    let input = noise::<E>(10, TOTAL);
    for frames in [L, 0] {
        let opts = options(
            frames,
            CrossfadeLaw::EqualGain,
            MidFadePolicy::QueueOne,
            LatencyPolicy::RequireEqual,
        );
        let expected = reference::<E>(&input, &[(0, &a, p), (F, &b, p)], opts.crossfade);
        // A zero-length block at F takes the proposal; others are no-ops.
        let mut blocks = vec![0];
        blocks.extend(uniform(F, MAX));
        blocks.push(0);
        blocks.push(0);
        blocks.extend(uniform(TOTAL - F, MAX));
        let run = drive(
            E::make(&a, p),
            opts,
            &input,
            &blocks,
            vec![(F, E::make(&b, p))],
        );
        assert_same(&run.output, &expected, E::bits, "zero frames");
        let at_f: Vec<_> = run
            .statuses
            .iter()
            .filter(|s| s.0 == F)
            .map(|s| s.1)
            .collect();
        assert_eq!(at_f[0].started, Some(1));
        assert_eq!(at_f[0].completed, if frames == 0 { Some(1) } else { None });
        assert_eq!(at_f[0].fading, frames != 0);
        assert!(
            at_f[1..]
                .iter()
                .all(|s| s.started.is_none() && s.completed.is_none())
        );
    }
    // An empty call with empty slices is admitted and changes nothing.
    let opts = options(
        L,
        CrossfadeLaw::EqualGain,
        MidFadePolicy::QueueOne,
        LatencyPolicy::RequireEqual,
    );
    let (_control, mut audio) = convolution_transition(E::make(&a, p), MAX, opts).unwrap();
    let status = audio
        .process(&[&[], &[]], &mut [&mut [], &mut []], 0)
        .unwrap();
    assert_eq!(status, TransitionStatus::default());
    assert_eq!(audio.frames_processed(), 0);
}

#[test]
fn shutdown_with_engines_in_flight_frees_on_the_dropping_thread() {
    type E = ConvolutionBank32;
    let p = P::ZeroLatency { head: 16 };
    let input = noise::<E>(11, 64 * MAX);
    // Control dropped first, mid-fade, with a waiting and a pending proposal.
    let opts = options(
        200,
        CrossfadeLaw::EqualGain,
        MidFadePolicy::LatestWins,
        LatencyPolicy::RequireEqual,
    );
    let (mut control, mut audio) =
        convolution_transition(E::make(&response(1, 100), p), MAX, opts).unwrap();
    let mut out = vec![vec![0f32; MAX]; 2];
    control.offer(E::make(&response(2, 100), p)).unwrap();
    block(&mut audio, &input, &mut out, 0, MAX);
    control.offer(E::make(&response(3, 100), p)).unwrap();
    block(&mut audio, &input, &mut out, MAX, MAX);
    control.offer(E::make(&response(4, 100), p)).unwrap();
    drop(control);
    // Audio keeps running on its own thread: transitions complete, retired and
    // superseded engines move into the abandoned queue/deferred slot, and the
    // audio thread neither allocates nor frees.
    let thread_input = input.clone();
    let (audio, info) = std::thread::spawn(move || {
        let input = thread_input;
        let mut out = vec![vec![0f32; MAX]; 2];
        let info = allocation_counter::measure(|| {
            for i in 2..64 {
                block(&mut audio, &input, &mut out, i * MAX, MAX);
            }
        });
        (audio, info)
    })
    .join()
    .unwrap();
    assert_eq!(
        (info.count_total, info.count_current, info.bytes_current),
        (0, 0, 0)
    );
    // Offers were sequences 1, 2, 3: 2 was superseded mid-fade by 3, which started.
    assert_eq!(audio.current_sequence(), 3);
    assert_eq!(audio.fade_position(), None);
    // The last endpoint dropped (here audio, on this non-audio thread) frees the
    // current engine, the rings and every engine still queued in them.
    let freed = allocation_counter::measure(|| drop(audio));
    assert!(freed.count_current < 0 && freed.count_total == 0);

    // Audio dropped first (off audio): control keeps returned engines, offers
    // report AudioGone with the engine, and dropping control frees the rest.
    let (mut control, mut audio) =
        convolution_transition(E::make(&response(5, 100), p), MAX, opts).unwrap();
    control.offer(E::make(&response(6, 100), p)).unwrap();
    let mut out = vec![vec![0f32; MAX]; 2];
    for i in 0..8 {
        block(&mut audio, &input, &mut out, i * MAX, MAX);
    }
    control.offer(E::make(&response(7, 100), p)).unwrap();
    drop(audio);
    let error = control.offer(E::make(&response(8, 100), p)).unwrap_err();
    assert_eq!(error.reason, TransitionOfferFailure::AudioGone);
    // Sequence 1 (response 6) replaced the initial engine (sequence 0).
    assert_eq!(
        control.take_returned().map(|r| (r.reason, r.sequence)),
        Some((ReturnReason::Retired, 0))
    );
    let freed = allocation_counter::measure(|| drop(control));
    assert!(freed.count_current < 0 && freed.count_total == 0);
}
