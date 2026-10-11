//! W257 review coverage: block-partition identity, stage-boundary impulses,
//! empty intervals, zero/subnormal responses, independent frequency response,
//! bank bounds/refusals and adapter re-preparation. Every numerical oracle is
//! an independent f64 direct convolution or analytic DTFT, never the engine.
use ravet_signal::{
    ConvolutionBank32, ConvolutionBank64, ConvolutionPartitioning as Mode,
    ConvolutionPreparationError as Prepare, ConvolutionProcessError as Process, Convolver32,
    Convolver64,
};
const BUDGET: usize = 256 * 1024 * 1024;
const B: usize = 16;
const T: usize = 128;
fn modes() -> [Mode; 4] {
    [
        Mode::UniformDelayed { partition: B },
        Mode::ZeroLatency { head: B },
        Mode::NonUniform { head: B, tail: T },
        Mode::NonUniform { head: 32, tail: 32 },
    ]
}
fn direct(h: &[f64], x: &[f64], latency: usize, n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            i.checked_sub(latency).map_or(0., |at| {
                h.iter()
                    .enumerate()
                    .filter_map(|(k, h)| at.checked_sub(k).and_then(|j| x.get(j)).map(|x| h * x))
                    .sum()
            })
        })
        .collect()
}
fn no_heap<R>(f: impl FnOnce() -> R) -> R {
    let mut out = None;
    let counter = allocation_counter::measure(|| out = Some(f()));
    assert_eq!(counter.count_total, 0, "heap allocation in prepared path");
    assert_eq!(counter.count_current, 0);
    out.unwrap()
}
macro_rules! review {
    ($identity:ident, $impulses:ident, $edges:ident, $spectrum:ident, $bank_test:ident,
     $engine:ident, $bank:ident, $s:ty, $tol:expr) => {
        #[test]
        fn $identity() {
            let maximum = 257;
            let taps = 700;
            let h: Vec<$s> = (0..taps)
                .map(|i| ((i as f64 * 0.917).sin() * 0.6 / (1. + 0.01 * i as f64)) as $s)
                .collect();
            let source: Vec<$s> = (0..1500)
                .map(|i| ((i as f64 * 0.21).cos() * 0.4 - (i as f64 * 1.3).sin() * 0.2) as $s)
                .collect();
            let wide: Vec<f64> = h.iter().map(|x| *x as f64).collect();
            let xs: Vec<f64> = source.iter().map(|x| *x as f64).collect();
            let mut worst = 0f64;
            for mode in modes() {
                let mut reference = $engine::new(&h, mode, maximum, BUDGET).unwrap();
                let info = reference.info();
                let n = source.len() + info.tail_samples + 3;
                let input: Vec<$s> = (0..n).map(|i| source.get(i).copied().unwrap_or(0.)).collect();
                // Sample-at-a-time execution is the identity reference.
                let single: Vec<$s> = input.iter().map(|x| reference.process_sample(*x)).collect();
                let oracle = direct(&wide, &xs, info.latency_samples, n);
                for (a, b) in single.iter().zip(&oracle) {
                    let e = (*a as f64 - b).abs();
                    worst = worst.max(e);
                    assert!(e <= $tol * (1. + b.abs()), "{mode:?} e={e:e}");
                }
                let schedules: [&[usize]; 10] = [
                    &[1], &[2], &[3], &[7], &[13], &[16], &[17], &[127], &[257],
                    &[0, 1, 5, 257, 11, 64, 3, 0, 129],
                ];
                for schedule in schedules {
                    for inplace in [false, true] {
                        let mut engine = $engine::new(&h, mode, maximum, BUDGET).unwrap();
                        let mut rendered = vec![0 as $s; n];
                        let (mut at, mut turn) = (0, 0);
                        while at < n {
                            let count = schedule[turn % schedule.len()].min(n - at);
                            turn += 1;
                            if inplace {
                                rendered[at..at + count].copy_from_slice(&input[at..at + count]);
                                no_heap(|| engine.process_in_place(&mut rendered[at..], count)).unwrap();
                            } else {
                                no_heap(|| engine.process(&input[at..], &mut rendered[at..], count)).unwrap();
                            }
                            at += count;
                        }
                        assert!(
                            rendered.iter().zip(&single).all(|(a, b)| a.to_bits() == b.to_bits()),
                            "{mode:?} schedule={schedule:?} inplace={inplace} not bit-identical"
                        );
                    }
                }
                // Reset at every stage boundary phase, then a full rerun matches a fresh engine.
                let mut boundaries = vec![B - 1, B, B + 1, T - 1, T, T + 1, taps - 1, taps];
                boundaries.push(taps + 1);
                for k in boundaries {
                    let mut engine = $engine::new(&h, mode, maximum, BUDGET).unwrap();
                    for x in &input[..k] {
                        engine.process_sample(*x);
                    }
                    no_heap(|| engine.reset());
                    assert!(engine.stage_work().into_iter().flatten().all(|w| w.completed_blocks == 0 && w.pending_source_frames == 0));
                    let again: Vec<$s> = input.iter().map(|x| engine.process_sample(*x)).collect();
                    assert!(again.iter().zip(&single).all(|(a, b)| a.to_bits() == b.to_bits()), "{mode:?} reset at {k}");
                }
            }
            println!("native{} block-partition identity: 4 modes x 10 schedules x 2 views bit-identical to per-sample; oracle maxabs={worst:e}", size_of::<$s>() * 8);
        }
        #[test]
        fn $impulses() {
            let taps = 300;
            let mut worst = 0f64;
            for mode in modes() {
                for position in [0, B - 1, B, B + 1, 31, 32, 33, T - 1, T, T + 1, taps - 1] {
                    let mut h = vec![0 as $s; taps];
                    h[position] = -0.75;
                    let mut engine = $engine::new(&h, mode, 64, BUDGET).unwrap();
                    let info = engine.info();
                    assert_eq!(info.tail_samples, taps - 1 + info.latency_samples);
                    for onset in [0usize, 5] {
                        engine.reset();
                        let n = onset + info.tail_samples + 1 + 40;
                        for i in 0..n {
                            let y = engine.process_sample(if i == onset { 1. } else { 0. }) as f64;
                            let expected = if i == onset + info.latency_samples + position { -0.75 } else { 0. };
                            let e = (y - expected).abs();
                            worst = worst.max(e);
                            assert!(e <= $tol, "{mode:?} tap={position} onset={onset} i={i} y={y:e}");
                        }
                    }
                }
            }
            println!("native{} stage-boundary impulses (B±1, T±1, end, onsets 0/5): maxabs={worst:e}", size_of::<$s>() * 8);
        }
        #[test]
        fn $edges() {
            // Empty intervals are omitted; no fabricated coefficients.
            let ones = |n: usize| vec![0.5 as $s; n];
            let z = $engine::new(&ones(B), Mode::ZeroLatency { head: B }, 8, BUDGET).unwrap().info();
            assert_eq!((z.direct_taps, z.stages), (B, [None, None]));
            let z = $engine::new(&ones(1), Mode::ZeroLatency { head: B }, 8, BUDGET).unwrap().info();
            assert_eq!((z.direct_taps, z.stages, z.tail_samples), (1, [None, None], 0));
            let z = $engine::new(&ones(B + 1), Mode::ZeroLatency { head: B }, 8, BUDGET).unwrap().info();
            let s = z.stages[0].unwrap();
            assert_eq!((s.original_offset, s.response_taps, s.partitions, s.fft_size), (B, 1, 1, 2 * B));
            let n = $engine::new(&ones(T), Mode::NonUniform { head: B, tail: T }, 8, BUDGET).unwrap().info();
            assert_eq!(n.stages[0].unwrap().response_taps, T - B);
            assert!(n.stages[1].is_none());
            let n = $engine::new(&ones(T + 1), Mode::NonUniform { head: B, tail: T }, 8, BUDGET).unwrap().info();
            assert_eq!((n.stages[1].unwrap().original_offset, n.stages[1].unwrap().response_taps), (T, 1));
            let n = $engine::new(&ones(200), Mode::NonUniform { head: 64, tail: 64 }, 8, BUDGET).unwrap().info();
            assert!(n.stages[0].is_none());
            assert_eq!(n.stages[1].unwrap().original_offset, 64);
            let u = $engine::new(&ones(1), Mode::UniformDelayed { partition: B }, 8, BUDGET).unwrap().info();
            assert_eq!((u.direct_taps, u.latency_samples, u.tail_samples), (0, B, B));
            assert_eq!(u.stages[0].unwrap().partitions, 1);
            // N=1 response through every mode reproduces the scaled, delayed source.
            for mode in modes() {
                let mut e = $engine::new(&[0.25 as $s], mode, 8, BUDGET).unwrap();
                let l = e.info().latency_samples;
                for i in 0..80usize {
                    let y = e.process_sample(((i % 7) as f64 - 3.) as $s) as f64;
                    let x = i.checked_sub(l).map_or(0., |j| (j % 7) as f64 - 3.);
                    assert!((y - 0.25 * x).abs() <= $tol, "{mode:?} N=1 i={i}");
                }
            }
            // All-zero response: exactly zero output for finite input.
            let source: Vec<$s> = (0..900).map(|i| ((i as f64 * 0.7).sin()) as $s).collect();
            for mode in modes() {
                let mut e = $engine::new(&vec![0 as $s; 400], mode, 900, BUDGET).unwrap();
                let mut out = vec![1 as $s; 900];
                e.process(&source, &mut out, 900).unwrap();
                assert!(out.iter().all(|y| *y == 0.), "{mode:?} zero response");
            }
            // All-subnormal response: no sanitization/flush policy; outputs stay
            // finite and are compared with the independent oracle in absolute
            // units of the coefficient magnitude (recorded, not hidden).
            let tiny = <$s>::MIN_POSITIVE / 4.;
            assert!(tiny.is_subnormal());
            let h = vec![tiny; 400];
            let wide: Vec<f64> = h.iter().map(|x| *x as f64).collect();
            let xs: Vec<f64> = source.iter().map(|x| *x as f64).collect();
            let mut worst = 0f64;
            for mode in modes() {
                let mut e = $engine::new(&h, mode, 900, BUDGET).unwrap();
                let mut out = vec![0 as $s; 900];
                e.process(&source, &mut out, 900).unwrap();
                let oracle = direct(&wide, &xs, e.info().latency_samples, 900);
                for (y, o) in out.iter().zip(&oracle) {
                    assert!(y.is_finite());
                    worst = worst.max((*y as f64 - o).abs() / tiny as f64);
                }
            }
            println!("native{} all-subnormal response: maxabs error = {worst:e} x coefficient ({tiny:e})", size_of::<$s>() * 8);
            assert!(worst <= 400., "subnormal error exceeds total coefficient mass");
        }
        #[test]
        fn $spectrum() {
            // Hann-windowed sinc low-pass, 257 taps, cutoff 0.15 cycles/sample.
            let taps = 257;
            let m = (taps - 1) as f64 / 2.;
            let h: Vec<$s> = (0..taps)
                .map(|k| {
                    let t = k as f64 - m;
                    let sinc = if t == 0. { 0.3 } else { (std::f64::consts::TAU * 0.15 * t).sin() / (std::f64::consts::PI * t) };
                    let w = 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / (taps - 1) as f64).cos();
                    (sinc * w) as $s
                })
                .collect();
            let mut worst = 0f64;
            for mode in modes() {
                for f in [0.0, 0.03, 0.1, 0.149, 0.2, 0.31, 0.45] {
                    let w = std::f64::consts::TAU * f;
                    // Independent analytic DTFT of the stored (quantized) response.
                    let (re, im) = h.iter().enumerate().fold((0., 0.), |(re, im), (k, h)| {
                        (re + *h as f64 * (w * k as f64).cos(), im - *h as f64 * (w * k as f64).sin())
                    });
                    let (gain, phase) = (re.hypot(im), im.atan2(re));
                    let mut e = $engine::new(&h, mode, 512, BUDGET).unwrap();
                    let l = e.info().latency_samples;
                    let n = taps + l + 1200;
                    let input: Vec<$s> = (0..n).map(|i| (w * i as f64).cos() as $s).collect();
                    let mut out = vec![0 as $s; n];
                    let mut at = 0;
                    while at < n {
                        let c = 512.min(n - at);
                        e.process(&input[at..], &mut out[at..], c).unwrap();
                        at += c;
                    }
                    for i in taps - 1 + l..n {
                        let expected = gain * (w * (i - l) as f64 + phase).cos();
                        let err = (out[i] as f64 - expected).abs();
                        worst = worst.max(err);
                        assert!(err <= $tol * 4., "{mode:?} f={f} i={i} err={err:e}");
                    }
                }
            }
            println!("native{} windowed-sinc steady-state vs analytic DTFT (7 freqs x 4 modes): maxabs={worst:e}", size_of::<$s>() * 8);
        }
        #[test]
        fn $bank_test() {
            let mode = Mode::ZeroLatency { head: B };
            let make = |len: usize, seed: f64| -> Vec<$s> {
                (0..len).map(|k| ((k as f64 * 0.37 + seed).sin() * 0.4 / (1. + 0.02 * k as f64)) as $s).collect()
            };
            // Refusals before allocation; late channel 63 is meaningful.
            let good = make(40, 0.);
            assert!(matches!($bank::new(&[], mode, 8, BUDGET), Err(Prepare::InvalidChannels)));
            assert!(matches!($bank::new(&[&good[..]; 65], mode, 8, BUDGET), Err(Prepare::InvalidChannels)));
            assert!(matches!($bank::new(&[&good[..]; 2], Mode::ZeroLatency { head: 24 }, 8, BUDGET), Err(Prepare::InvalidPartitioning)));
            for maximum in [0, 65537] {
                assert!(matches!($bank::new(&[&good[..]; 2], mode, maximum, BUDGET), Err(Prepare::InvalidMaximum)));
            }
            let too_long = vec![0 as $s; 262_145];
            let nan = [<$s>::NAN];
            for bad in [&[][..], &nan[..], &too_long[..]] {
                let mut responses = [&good[..]; 64];
                responses[63] = bad;
                let counter = allocation_counter::measure(|| {
                    assert!(matches!($bank::new(&responses, mode, 8, BUDGET), Err(Prepare::InvalidResponse)));
                });
                assert_eq!(counter.count_total, 0);
            }
            // Exact aggregate budget endpoint and endpoint-1.
            let full = [&good[..]; 64];
            let declared = $bank::new(&full, mode, 8, BUDGET).unwrap().declared_numeric_bytes();
            let at_limit = $bank::new(&full, mode, 8, declared).unwrap();
            assert_eq!(at_limit.actual_numeric_bytes(), declared);
            for budget in [0, declared - 1, BUDGET + 1] {
                let counter = allocation_counter::measure(|| {
                    assert!(matches!($bank::new(&full, mode, 8, budget), Err(Prepare::Capacity)));
                });
                assert_eq!(counter.count_total, 0);
            }
            // Mono, stereo and 64 channels with differing IR lengths: independent oracle.
            let mut worst = 0f64;
            for channels in [1usize, 2, 64] {
                let irs: Vec<Vec<$s>> = (0..channels).map(|ch| make(1 + ch * 13, ch as f64)).collect();
                let views: Vec<&[$s]> = irs.iter().map(|x| &x[..]).collect();
                let mut bank = $bank::new(&views, mode, 37, BUDGET).unwrap();
                assert_eq!(bank.channels(), channels);
                let n = 1100;
                let src: Vec<Vec<$s>> = (0..channels)
                    .map(|ch| (0..n).map(|i| ((i as f64 * (0.05 + 0.011 * ch as f64)).sin() * 0.5) as $s).collect())
                    .collect();
                let mut out: Vec<Vec<$s>> = vec![vec![19 as $s; n]; channels];
                let (mut at, mut turn) = (0, 0);
                while at < n {
                    let count = [37, 1, 0, 23, 5][turn % 5].min(n - at);
                    turn += 1;
                    let inputs: Vec<&[$s]> = src.iter().map(|x| &x[at..]).collect();
                    let mut outputs: Vec<&mut [$s]> = out.iter_mut().map(|x| &mut x[at..]).collect();
                    no_heap(|| bank.process_planar(&inputs, &mut outputs, count)).unwrap();
                    at += count;
                }
                for ch in 0..channels {
                    let h: Vec<f64> = irs[ch].iter().map(|x| *x as f64).collect();
                    let x: Vec<f64> = src[ch].iter().map(|x| *x as f64).collect();
                    for (y, o) in out[ch].iter().zip(direct(&h, &x, 0, n)) {
                        let e = (*y as f64 - o).abs();
                        worst = worst.max(e);
                        assert!(e <= $tol * (1. + o.abs()), "channels={channels} ch={ch}");
                    }
                }
                // Whole-call refusals leave outputs and every history untouched.
                let before = out.clone();
                let probe: Vec<Vec<$s>> = (0..channels).map(|ch| vec![(ch + 1) as $s; 40]).collect();
                {
                    let inputs: Vec<&[$s]> = probe.iter().map(|x| &x[..]).collect();
                    let mut outputs: Vec<&mut [$s]> = out.iter_mut().map(|x| &mut x[..]).collect();
                    assert_eq!(bank.process_planar(&inputs, &mut outputs, 38), Err(Process::TooManyFrames));
                    assert_eq!(bank.process_planar(&inputs[..channels - 1], &mut outputs, 4), Err(Process::ChannelMismatch));
                    assert_eq!(bank.process_planar(&inputs, &mut outputs[1..], 4), Err(Process::ChannelMismatch));
                    let mut short_inputs = inputs.clone();
                    short_inputs[channels - 1] = &probe[channels - 1][..3];
                    assert_eq!(bank.process_planar(&short_inputs, &mut outputs, 4), Err(Process::PrefixTooShort));
                    assert_eq!(bank.process_in_place(&mut outputs[..channels - 1], 4), Err(Process::ChannelMismatch));
                }
                {
                    let mut short: Vec<&mut [$s]> = out.iter_mut().map(|x| &mut x[..]).collect();
                    let last = short.pop().unwrap();
                    short.push(&mut last[..2]);
                    assert_eq!(bank.process_in_place(&mut short, 4), Err(Process::PrefixTooShort));
                }
                assert_eq!(out, before);
                assert_eq!(bank.reset_channel(channels), Err(Process::InvalidChannel));
                // Continue both from identical histories (bank already consumed n frames).
                let mut replay = $bank::new(&views, mode, 37, BUDGET).unwrap();
                let mut sink: Vec<Vec<$s>> = vec![vec![0 as $s; n]; channels];
                {
                    let inputs: Vec<&[$s]> = src.iter().map(|x| &x[..]).collect();
                    let mut outputs: Vec<&mut [$s]> = sink.iter_mut().map(|x| &mut x[..]).collect();
                    for at in (0..n).step_by(37) {
                        let c = 37.min(n - at);
                        let i: Vec<&[$s]> = inputs.iter().map(|x| &x[at..]).collect();
                        let mut o: Vec<&mut [$s]> = outputs.iter_mut().map(|x| &mut x[at..]).collect();
                        replay.process_planar(&i, &mut o, c).unwrap();
                    }
                }
                // reset_channel affects only the named channel.
                let reset_ch = channels / 2;
                bank.reset_channel(reset_ch).unwrap();
                let mut fresh = $bank::new(&views, mode, 37, BUDGET).unwrap();
                let mut a: Vec<Vec<$s>> = probe.clone();
                let mut b: Vec<Vec<$s>> = probe.clone();
                let mut c: Vec<Vec<$s>> = probe.clone();
                {
                    let mut va: Vec<&mut [$s]> = a.iter_mut().map(|x| &mut x[..]).collect();
                    bank.process_in_place(&mut va, 37).unwrap();
                    let mut vb: Vec<&mut [$s]> = b.iter_mut().map(|x| &mut x[..]).collect();
                    replay.process_in_place(&mut vb, 37).unwrap();
                    let mut vc: Vec<&mut [$s]> = c.iter_mut().map(|x| &mut x[..]).collect();
                    fresh.process_in_place(&mut vc, 37).unwrap();
                }
                for ch in 0..channels {
                    let expected = if ch == reset_ch { &c[ch] } else { &b[ch] };
                    assert!(a[ch].iter().zip(expected).all(|(x, y)| x.to_bits() == y.to_bits()), "reset_channel isolation ch={ch}");
                }
            }
            println!("native{} banks 1/2/64 channels, distinct IR lengths, refusals atomic: maxabs={worst:e}", size_of::<$s>() * 8);
        }
    };
}
review!(
    native64_block_partition_identity_and_reset_boundaries,
    native64_stage_boundary_impulses,
    native64_empty_intervals_zero_and_subnormal_responses,
    native64_frequency_response_against_analytic_dtft,
    native64_bank_bounds_refusals_and_channel_isolation,
    Convolver64,
    ConvolutionBank64,
    f64,
    3e-11
);
review!(
    native32_block_partition_identity_and_reset_boundaries,
    native32_stage_boundary_impulses,
    native32_empty_intervals_zero_and_subnormal_responses,
    native32_frequency_response_against_analytic_dtft,
    native32_bank_bounds_refusals_and_channel_isolation,
    Convolver32,
    ConvolutionBank32,
    f32,
    2e-5
);
#[path = "support/convolution_processor.rs"]
mod convolution_processor;
mod adapter {
    use super::convolution_processor::{ConvolutionProcessor32, ConvolutionProcessor64};
    use super::*;
    use ravet_foundation::{
        AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
        ProcessStatus, Processor, Tail, Transport,
    };
    macro_rules! adapter {
        ($test:ident, $processor:ident, $bank:ident, $s:ty) => {
            #[test]
            fn $test() {
                let h = [[0.5 as $s, 0.25], [-0.5 as $s, 0.125]];
                let mode = Mode::UniformDelayed { partition: B };
                let bank = || $bank::new(&[&h[0][..], &h[1][..]], mode, 64, BUDGET).unwrap();
                let refused = match $processor::new(bank(), f64::NAN) {
                    Err(f) => f,
                    Ok(_) => panic!("nonfinite rate accepted"),
                };
                assert_eq!(refused.error(), PrepareError::InvalidSampleRate);
                assert_eq!(refused.bank().channels(), 2);
                let mut p = $processor::new(refused.into_parts().0, 48000.)
                    .unwrap_or_else(|_| panic!("finite rate"));
                assert_eq!(p.latency_samples(), B as u32);
                assert_eq!(p.tail(), Tail::Samples((1 + B) as u64));
                let stereo = AudioLayout::new(2, 2).unwrap();
                let good = ProcessConfig::new(48000., 64, 0, stereo).unwrap();
                let bad: [(ProcessConfig, PrepareError); 4] = [
                    (
                        ProcessConfig::new(44100., 64, 0, stereo).unwrap(),
                        PrepareError::InvalidSampleRate,
                    ),
                    (
                        ProcessConfig::new(48000., 64, 0, AudioLayout::new(1, 2).unwrap()).unwrap(),
                        PrepareError::UnsupportedLayout,
                    ),
                    (
                        ProcessConfig::new(48000., 65, 0, stereo).unwrap(),
                        PrepareError::InvalidMaximumFrames,
                    ),
                    (
                        good.with_auxiliary_buses(&[stereo]).unwrap(),
                        PrepareError::TooManyAuxiliaryBuses,
                    ),
                ];
                let run = |p: &mut $processor, x: $s| -> (ProcessStatus, [[$s; 4]; 2]) {
                    let input = [[x, 0., 0., 0.], [x, 0., 0., 0.]];
                    let mut output = [[7 as $s; 4]; 2];
                    let ins = [&input[0][..], &input[1][..]];
                    let [a, b] = &mut output;
                    let mut outs = [&mut a[..], &mut b[..]];
                    let status = p.process(&mut ProcessBlock::new(
                        AudioBuffer::separate(&ins, &mut outs, 4).unwrap(),
                        &[],
                        Transport::default(),
                    ));
                    (status, output)
                };
                for (config, error) in bad {
                    p.prepare(good).unwrap();
                    assert_eq!(p.prepare(config), Err(error));
                    // A refused re-preparation discards the old configuration.
                    assert!(p.config().is_none());
                    let (status, output) = run(&mut p, 1.);
                    assert_eq!(status, ProcessStatus::Fault(ProcessError::NotPrepared));
                    assert_eq!(output, [[0 as $s; 4]; 2]);
                }
                p.prepare(good).unwrap();
                p.start_audio().unwrap();
                assert_eq!(p.prepare(good), Err(PrepareError::AudioSessionActive));
                assert_eq!(p.config(), Some(good));
                let mut collected = [[0 as $s; 2]; 2];
                for block in 0..6 {
                    let (status, output) = run(&mut p, if block == 0 { 1. } else { 0. });
                    assert_eq!(status, ProcessStatus::Continue);
                    for ch in 0..2 {
                        for (i, y) in output[ch].iter().enumerate() {
                            let t = block * 4 + i;
                            if (B..B + 2).contains(&t) {
                                collected[ch][t - B] = *y;
                            } else {
                                assert!(y.abs() <= 1e-6, "t={t}");
                            }
                        }
                    }
                }
                for ch in 0..2 {
                    for k in 0..2 {
                        assert!((collected[ch][k] - h[ch][k]).abs() <= 1e-6);
                    }
                }
                p.stop_audio();
                let p = match p.into_bank() {
                    Err(p) => *p,
                    Ok(_) => panic!("prepared adapter released its bank"),
                };
                let mut p = p;
                // Reprepare at the same rate clears history and stays exact.
                p.prepare(good).unwrap();
                let (_, output) = run(&mut p, 0.);
                assert_eq!(output, [[0 as $s; 4]; 2]);
                p.deactivate();
                let bank = p.into_bank().unwrap_or_else(|_| panic!("inactive adapter"));
                assert_eq!(bank.channels(), 2);
            }
        };
    }
    adapter!(
        native32_adapter_reprepare_refusals_and_extraction,
        ConvolutionProcessor32,
        ConvolutionBank32,
        f32
    );
    adapter!(
        native64_adapter_reprepare_refusals_and_extraction,
        ConvolutionProcessor64,
        ConvolutionBank64,
        f64
    );
}
