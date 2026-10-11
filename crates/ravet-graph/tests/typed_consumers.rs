use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor,
    Transport,
};
use ravet_graph::{GraphStatus, MonoDuplicator, SharedMonoKernel, TypedChain};
use ravet_signal::{
    FirConvolver, FirstOrderTptDesign, FirstOrderTptDesign32, FirstOrderTptFilter,
    FirstOrderTptFilter32, LadderDesign, LadderDesign32, LadderFilter, LadderFilter32, LadderMode,
};
const TAPS: [f64; 5] = [0.2, 0.4, 0.2, -0.1, 0.3];
macro_rules! kernels {
    ($s:ty,$td:ty,$tf:ty,$ld:ty,$lf:ty,$tk:ident,$lk:ident,$fir:ident) => {
        struct $tk {
            filter: $tf,
            calls: usize,
        }
        impl SharedMonoKernel<$s, $td> for $tk {
            fn prepare(&mut self, rate: f64, _: usize, c: &$td) -> Result<(), PrepareError> {
                if c.rate() != rate {
                    return Err(PrepareError::InvalidSampleRate);
                }
                self.filter.replace_design(*c);
                Ok(())
            }
            fn process_sample(&mut self, x: $s, c: &$td) -> $s {
                self.calls += 1;
                self.filter.set_design(*c);
                self.filter.process_sample(x).low_pass
            }
            fn reset(&mut self, _: &$td) {
                self.filter.reset();
            }
            fn deactivate(&mut self) {}
        }
        struct $lk {
            filter: $lf,
            calls: usize,
        }
        impl SharedMonoKernel<$s, $ld> for $lk {
            fn prepare(&mut self, rate: f64, _: usize, c: &$ld) -> Result<(), PrepareError> {
                if c.rate() != rate {
                    return Err(PrepareError::InvalidSampleRate);
                }
                self.filter.replace_design(*c);
                Ok(())
            }
            fn process_sample(&mut self, x: $s, c: &$ld) -> $s {
                self.calls += 1;
                self.filter.set_design(*c);
                self.filter.process_sample(x).select(LadderMode::LowPass24)
            }
            fn reset(&mut self, _: &$ld) {
                self.filter.reset();
            }
            fn deactivate(&mut self) {}
        }
        struct $fir {
            filters: Vec<FirConvolver>,
            config: Option<ProcessConfig>,
            frames: usize,
        }
        impl Processor<$s> for $fir {
            fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
                self.filters.iter_mut().for_each(FirConvolver::reset);
                self.config = Some(c);
                Ok(())
            }
            fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, $s>) -> ProcessStatus {
                self.config.unwrap().validate_block(b).unwrap();
                self.frames += b.audio.frames();
                for ch in 0..self.filters.len() {
                    for f in 0..b.audio.frames() {
                        let x = b.audio.input(ch).unwrap()[f];
                        b.audio.output_mut(ch).unwrap()[f] =
                            self.filters[ch].process_sample(x as f64) as $s;
                    }
                }
                ProcessStatus::Continue
            }
            fn reset(&mut self) {
                self.filters.iter_mut().for_each(FirConvolver::reset);
            }
            fn deactivate(&mut self) {
                self.config = None;
            }
        }
    };
}
kernels!(
    f64,
    FirstOrderTptDesign,
    FirstOrderTptFilter,
    LadderDesign,
    LadderFilter,
    Tpt64,
    Ladder64,
    Fir64
);
kernels!(
    f32,
    FirstOrderTptDesign32,
    FirstOrderTptFilter32,
    LadderDesign32,
    LadderFilter32,
    Tpt32,
    Ladder32,
    Fir32
);
// Independent implicit nodal Newton oracle from the framework W245 test packet.
// Solves four RC equations directly, never calls scalar reduction/production solve.
fn linear_solve(mut a: [[f64; 4]; 4], mut b: [f64; 4]) -> [f64; 4] {
    for i in 0..4 {
        let p = (i..4)
            .max_by(|&j, &k| a[j][i].abs().total_cmp(&a[k][i].abs()))
            .unwrap();
        a.swap(i, p);
        b.swap(i, p);
        assert!(a[i][i].abs() > 1e-20);
        let z = a[i][i];
        for value in &mut a[i][i..] {
            *value /= z;
        }
        b[i] /= z;
        for j in 0..4 {
            if j != i {
                let z = a[j][i];
                let pivot_row = a[i];
                for (value, &pivot) in a[j][i..].iter_mut().zip(&pivot_row[i..]) {
                    *value -= z * pivot;
                }
                b[j] -= z * b[i];
            }
        }
    }
    b
}
fn nodal(state: &mut [f64; 4], x: f64, gain: f64, k: f64, d: f64, nonlinear: bool) -> [f64; 6] {
    let g = gain / (1. - gain);
    let mut y = *state;
    for _ in 0..80 {
        let v = d * x - k * y[3];
        let u = if nonlinear { v.tanh() } else { v };
        let derivative = if nonlinear { 1. - u * u } else { 1. };
        let residual = std::array::from_fn(|i| {
            (1. + g) * y[i] - state[i] - g * if i == 0 { u } else { y[i - 1] }
        });
        let mut jac = [[0.; 4]; 4];
        for i in 0..4 {
            jac[i][i] = 1. + g;
            if i > 0 {
                jac[i][i - 1] = -g;
            }
        }
        jac[0][3] += g * k * derivative;
        let delta = linear_solve(jac, residual);
        let before = y;
        for i in 0..4 {
            y[i] -= delta[i];
        }
        if delta.iter().map(|v| v.abs()).fold(0., f64::max)
            < 2e-15 * (1. + y.iter().map(|v| v.abs()).fold(0., f64::max))
        {
            break;
        }
        if before == y {
            break;
        }
    }
    let u = if nonlinear {
        (d * x - k * y[3]).tanh()
    } else {
        d * x - k * y[3]
    };
    for i in 0..4 {
        state[i] = 2. * y[i] - state[i];
    }
    let [a, b, c, e] = y;
    [
        b,
        e,
        u - 2. * a + b,
        u - 4. * a + 6. * b - 4. * c + e,
        a - b,
        b - 2. * c + e,
    ]
}
trait Rate {
    fn rate(&self) -> f64;
}
impl Rate for FirstOrderTptDesign {
    fn rate(&self) -> f64 {
        self.sample_rate()
    }
}
impl Rate for FirstOrderTptDesign32 {
    fn rate(&self) -> f64 {
        self.sample_rate()
    }
}
impl Rate for LadderDesign {
    fn rate(&self) -> f64 {
        self.sample_rate()
    }
}
impl Rate for LadderDesign32 {
    fn rate(&self) -> f64 {
        self.prototype().sample_rate()
    }
}
macro_rules! consumer {
    ($name:ident,$s:ty,$td:ident,$tf:ident,$ld:ident,$lf:ident,$tk:ident,$lk:ident,$fir:ident,$convert:expr,$tol:expr) => {
        #[test]
        fn $name() {
            let mut maximum_error = 0f64;
            for (cutoff, resonance, drive) in [
                (500., 0., 1.),
                (2700., 0.2, 2.),
                (11000., 0.8, 7.),
                (300., 0.95, 1.),
                (14000., 0.3, 16.),
                (5000., 1., 3.),
            ] {
                let (tpt, ladder) = ($convert)(
                    FirstOrderTptDesign::new(48000., cutoff).unwrap(),
                    LadderDesign::new(48000., cutoff * 0.75, resonance, drive).unwrap(),
                );
                let d1 = MonoDuplicator::<$s, _, _>::new(tpt, 3, 67, |_, c| {
                    Ok::<_, ()>($tk {
                        filter: $tf::new(*c),
                        calls: 0,
                    })
                })
                .ok()
                .unwrap();
                let d2 = MonoDuplicator::<$s, _, _>::new(ladder, 3, 67, |_, c| {
                    Ok::<_, ()>($lk {
                        filter: $lf::new(*c),
                        calls: 0,
                    })
                })
                .ok()
                .unwrap();
                let fir = $fir {
                    filters: (0..3).map(|_| FirConvolver::new(&TAPS).unwrap()).collect(),
                    config: None,
                    frames: 0,
                };
                let mut chain = TypedChain::<$s, _>::new((d1, d2, fir), 3, 67, 0)
                    .ok()
                    .unwrap();
                let input: [Vec<$s>; 3] = std::array::from_fn(|ch| {
                    (0..769)
                        .map(|n| {
                            (0.17 * ((n * (ch + 1)) as f64 * 0.043).sin()
                                + if n == ch { 0.3 } else { 0. }) as $s
                        })
                        .collect()
                });
                // Distinct difference-equation TPT and implicit 4-D nodal ladder;
                // FIR is an explicit convolution sum, not the production circular history.
                let reference: [Vec<f64>; 3] = std::array::from_fn(|ch| {
                    let mut prev_x = 0.;
                    let mut prev_y = 0.;
                    let mut state = [0.; 4];
                    let mut wet = Vec::new();
                    let g = tpt.gain() as f64;
                    for &x in &input[ch] {
                        let x = x as f64;
                        let y = g * x + g * prev_x + (1. - 2. * g) * prev_y;
                        prev_x = x;
                        prev_y = y;
                        wet.push(
                            nodal(
                                &mut state,
                                y,
                                ladder.gain() as f64,
                                ladder.feedback() as f64,
                                ladder.input_drive() as f64,
                                true,
                            )[1],
                        );
                    }
                    (0..wet.len())
                        .map(|n| {
                            TAPS.iter()
                                .enumerate()
                                .filter(|(j, _)| *j <= n)
                                .map(|(j, t)| t * wet[n - j])
                                .sum()
                        })
                        .collect()
                });
                let cfg =
                    ProcessConfig::new(48000., 67, 0, AudioLayout::new(3, 3).unwrap()).unwrap();
                let mut output: [Vec<$s>; 3] =
                    std::array::from_fn(|_| vec![99. as $s; input[0].len() + 3]);
                // Preallocated outside every measured window.
                let mut scratch: [Vec<$s>; 3] = std::array::from_fn(|_| vec![0 as $s; 67]);
                for cycle in 0..2 {
                    chain.prepare(cfg).unwrap();
                    for _ in 0..2 {
                        chain.reset();
                        output.iter_mut().for_each(|v| v.fill(99. as $s));
                        let mut session = chain.start_audio().unwrap();
                        // Counters are taken before the rejected block so they can show it ran nothing.
                        let before_tpt: [usize; 3] = std::array::from_fn(|ch| {
                            session.stage::<0>().kernel(ch).unwrap().calls
                        });
                        let before_ladder: [usize; 3] = std::array::from_fn(|ch| {
                            session.stage::<1>().kernel(ch).unwrap().calls
                        });
                        let fir_frames = session.stage::<2>().frames;
                        // Actual late shape admission before either shared kernel executes.
                        let mut a = [9. as $s; 68];
                        let mut b = [9. as $s; 68];
                        let mut c = [9. as $s; 68];
                        let mut views = [&mut a[..], &mut b[..], &mut c[..]];
                        let audio = AudioBuffer::in_place(&mut views, 68).unwrap();
                        assert!(matches!(
                            session.process(&mut ProcessBlock::new(
                                audio,
                                &[],
                                Transport::default()
                            )),
                            GraphStatus::Fault { node: None, .. }
                        ));
                        for ch in 0..3 {
                            assert_eq!(
                                session.stage::<0>().kernel(ch).unwrap().calls,
                                before_tpt[ch]
                            );
                            assert_eq!(
                                session.stage::<1>().kernel(ch).unwrap().calls,
                                before_ladder[ch]
                            );
                        }
                        assert_eq!(session.stage::<2>().frames, fir_frames);
                        let mut consumed = 0;
                        let info = allocation_counter::measure(|| {
                            let mut turn = 0;
                            while consumed < input[0].len() {
                                let count =
                                    [1, 67, 0, 2, 31, 5][turn % 6].min(input[0].len() - consumed);
                                turn += 1;
                                let inputs = std::array::from_fn::<_, 3, _>(|ch| {
                                    &input[ch][consumed..consumed + count]
                                });
                                let [a, b, c] = &mut output;
                                let mut outputs = [
                                    &mut a[consumed..consumed + count],
                                    &mut b[consumed..consumed + count],
                                    &mut c[consumed..consumed + count],
                                ];
                                let audio =
                                    AudioBuffer::separate(&inputs, &mut outputs, count).unwrap();
                                assert_eq!(
                                    session.process(&mut ProcessBlock::new(
                                        audio,
                                        &[],
                                        Transport::default()
                                    )),
                                    GraphStatus::Continue
                                );
                                consumed += count;
                            }
                        });
                        assert_eq!(info.count_total, 0);
                        assert_eq!(info.count_current, 0);
                        for ch in 0..3 {
                            assert_eq!(
                                session.stage::<0>().kernel(ch).unwrap().calls - before_tpt[ch],
                                769
                            );
                            assert_eq!(
                                session.stage::<1>().kernel(ch).unwrap().calls - before_ladder[ch],
                                769
                            );
                        }
                        assert_eq!(session.stage::<2>().frames - fir_frames, 769);
                        // Bypass of the first shared stage: allocation-free, freezes its
                        // kernels, and the downstream stages keep running. Output values are
                        // not compared (the oracle above covers the unbypassed signal).
                        session.set_bypassed::<0>(true);
                        assert!(session.is_bypassed::<0>());
                        let frozen: [usize; 3] = std::array::from_fn(|ch| {
                            session.stage::<0>().kernel(ch).unwrap().calls
                        });
                        let downstream = session.stage::<1>().kernel(0).unwrap().calls;
                        let bypass_info = allocation_counter::measure(|| {
                            for count in [67usize, 0, 5] {
                                let inputs =
                                    std::array::from_fn::<_, 3, _>(|ch| &input[ch][..count]);
                                let [a, b, c] = &mut scratch;
                                let mut outputs =
                                    [&mut a[..count], &mut b[..count], &mut c[..count]];
                                let audio =
                                    AudioBuffer::separate(&inputs, &mut outputs, count).unwrap();
                                assert_eq!(
                                    session.process(&mut ProcessBlock::new(
                                        audio,
                                        &[],
                                        Transport::default()
                                    )),
                                    GraphStatus::Continue
                                );
                            }
                        });
                        assert_eq!(bypass_info.count_total, 0);
                        assert_eq!(bypass_info.count_current, 0);
                        for ch in 0..3 {
                            assert_eq!(session.stage::<0>().kernel(ch).unwrap().calls, frozen[ch]);
                        }
                        assert_eq!(
                            session.stage::<1>().kernel(0).unwrap().calls - downstream,
                            72
                        );
                        session.set_bypassed::<0>(false);
                        session.finish();
                        // The Processor facade and the typed getters allocate nothing either.
                        let facade_info = allocation_counter::measure(|| {
                            let inputs = std::array::from_fn::<_, 3, _>(|ch| &input[ch][..67]);
                            let [a, b, c] = &mut scratch;
                            let mut outputs = [&mut a[..], &mut b[..], &mut c[..]];
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 67).unwrap();
                            assert_eq!(
                                Processor::process(
                                    &mut chain,
                                    &mut ProcessBlock::new(audio, &[], Transport::default())
                                ),
                                ProcessStatus::Continue
                            );
                            assert!(!chain.is_bypassed::<0>());
                            assert!(chain.first_fault().is_none());
                            let _ = chain.latency_samples();
                            let _ = chain.tail();
                            let _ = chain.stage::<0>().kernel(0).unwrap().calls;
                        });
                        assert_eq!(facade_info.count_total, 0);
                        assert_eq!(facade_info.count_current, 0);
                        for ch in 0..3 {
                            assert_eq!(&output[ch][769..], &[99. as $s; 3]);
                            for (actual, expected) in output[ch][..769].iter().zip(&reference[ch]) {
                                let error = (*actual as f64 - expected).abs();
                                maximum_error = maximum_error.max(error);
                                assert!(
                                    error < $tol * (1. + expected.abs()),
                                    "cycle{cycle} error{error} actual{} expected{expected}",
                                    *actual
                                );
                            }
                        }
                    }
                    chain.deactivate();
                }
                let (first, second, third) = chain.into_stages();
                assert_eq!(first.channels(), 3);
                assert_eq!(second.channels(), 3);
                assert_eq!(third.filters.len(), 3);
                let (c, k) = first.into_parts();
                assert_eq!(c, tpt);
                assert_eq!(k.len(), 3);
            }
            eprintln!("selected composed max absolute error {maximum_error}");
        }
    };
}
consumer!(
    native64_shared_tpt_ladder_typed_graph_fir_independent_nodal_and_heap,
    f64,
    FirstOrderTptDesign,
    FirstOrderTptFilter,
    LadderDesign,
    LadderFilter,
    Tpt64,
    Ladder64,
    Fir64,
    |t, l| (t, l),
    2e-12
);
consumer!(
    native32_shared_tpt_ladder_typed_graph_fir_independent_nodal_and_heap,
    f32,
    FirstOrderTptDesign32,
    FirstOrderTptFilter32,
    LadderDesign32,
    LadderFilter32,
    Tpt32,
    Ladder32,
    Fir32,
    |t: FirstOrderTptDesign, l: LadderDesign| (
        t.quantize_f32().unwrap(),
        l.quantize_f32().unwrap()
    ),
    3e-6
);
