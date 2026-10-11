use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor,
    Sample, Transport,
};
use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
use ravet_signal::{
    FirConvolver, TruePeakBank64 as Bank, TruePeakDesign as Design, TruePeakFactor as Factor,
    TruePeakMeter64 as Meter,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Sender},
};
const TAPS: [f64; 5] = [0.1, 0.4, -0.2, 0.3, 0.2];
trait Native: Sample + ravet_graph::GraphSample + Send + 'static {
    fn wide(self) -> f64;
    fn narrow(x: f64) -> Self;
    fn observe(bank: &mut Bank, inputs: &[&[Self]], n: usize);
    fn observe_mut(bank: &mut Bank, inputs: &[&mut [Self]], n: usize);
}
impl Native for f64 {
    fn wide(self) -> f64 {
        self
    }
    fn narrow(x: f64) -> Self {
        x
    }
    fn observe(bank: &mut Bank, inputs: &[&[Self]], n: usize) {
        bank.observe_planar(inputs, n).unwrap();
    }
    fn observe_mut(bank: &mut Bank, inputs: &[&mut [Self]], n: usize) {
        bank.observe_in_place(inputs, n).unwrap();
    }
}
impl Native for f32 {
    fn wide(self) -> f64 {
        self as f64
    }
    fn narrow(x: f64) -> Self {
        x as f32
    }
    fn observe(bank: &mut Bank, inputs: &[&[Self]], n: usize) {
        bank.observe_planar_f32(inputs, n).unwrap();
    }
    fn observe_mut(bank: &mut Bank, inputs: &[&mut [Self]], n: usize) {
        bank.observe_in_place_f32(inputs, n).unwrap();
    }
}
struct MeterNode<S: Native> {
    design: Design,
    bank: Option<Bank>,
    scratch: [Vec<S>; 3],
    config: Option<ProcessConfig>,
    inplace: bool,
    returned: Sender<Bank>,
    frames: Arc<AtomicUsize>,
}
impl<S: Native> Processor<S> for MeterNode<S> {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        if c.layout() != AudioLayout::new(3, 3).unwrap() {
            return Err(PrepareError::UnsupportedLayout);
        }
        if c.sample_rate() != self.design.sample_rate() as f64 {
            return Err(PrepareError::InvalidSampleRate);
        }
        if c.maximum_frames() > 67 {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        self.bank = Some(Bank::new(&self.design, 3, 67).expect("test CONTROL allocation"));
        self.config = Some(c);
        self.reset();
        Ok(())
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        if let Err(e) = self.config.unwrap().validate_block(b) {
            b.clear_outputs();
            return ProcessStatus::Fault(e);
        }
        let n = b.audio.frames();
        for ch in 0..3 {
            self.scratch[ch][..n].copy_from_slice(&b.audio.input(ch).unwrap()[..n]);
        }
        if self.inplace {
            let [a, c, d] = &mut self.scratch;
            let refs = [&mut a[..n], &mut c[..n], &mut d[..n]];
            S::observe_mut(self.bank.as_mut().unwrap(), &refs, n);
        } else {
            let refs = self.scratch.each_ref().map(|s| &s[..n]);
            S::observe(self.bank.as_mut().unwrap(), &refs, n);
        }
        for ch in 0..3 {
            b.audio.output_mut(ch).unwrap()[..n].copy_from_slice(&self.scratch[ch][..n]);
        }
        self.frames.fetch_add(n, Ordering::Relaxed);
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        if let Some(bank) = self.bank.as_mut() {
            bank.reset();
        }
        self.frames.store(0, Ordering::Relaxed);
    }
    fn deactivate(&mut self) {
        self.config = None;
        if let Some(bank) = self.bank.take() {
            // Test-only CONTROL ownership transfer. No sender operation occurs in AUDIO.
            self.returned.send(bank).unwrap();
        }
    }
}
struct FirNode<S: Native> {
    fir: [FirConvolver; 3],
    config: Option<ProcessConfig>,
    _sample: std::marker::PhantomData<S>,
}
impl<S: Native> Processor<S> for FirNode<S> {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(c);
        self.reset();
        Ok(())
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        if let Err(e) = self.config.unwrap().validate_block(b) {
            b.clear_outputs();
            return ProcessStatus::Fault(e);
        }
        for ch in 0..3 {
            for i in 0..b.audio.frames() {
                let sample = b.audio.input(ch).unwrap()[i];
                let output = self.fir[ch].process_sample(sample.wide());
                b.audio.output_mut(ch).unwrap()[i] = S::narrow(output);
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        for fir in &mut self.fir {
            fir.reset();
        }
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
}
fn direct_peak(d: &Design, input: &[f64]) -> f64 {
    (0..input.len() + 2 * d.half_width())
        .flat_map(|t| (0..d.factor().factor()).map(move |p| (t, p)))
        .map(|(t, p)| {
            d.phase_coefficients(p)
                .unwrap()
                .iter()
                .enumerate()
                .filter_map(|(j, h)| t.checked_sub(j).and_then(|i| input.get(i)).map(|x| h * x))
                .sum::<f64>()
                .abs()
        })
        .fold(0_f64, f64::max)
}
// Independent zero-history convolution of the (widened) delivered input with
// TAPS. NaN inputs propagate through the same arithmetic as the FIR node does.
fn fir_reference<S: Native>(input: &[S]) -> Vec<S> {
    let wide: Vec<f64> = input.iter().map(|x| x.wide()).collect();
    (0..wide.len())
        .map(|n| {
            S::narrow(
                TAPS.iter()
                    .enumerate()
                    .filter_map(|(j, h)| n.checked_sub(j).map(|i| h * wide[i]))
                    .sum(),
            )
        })
        .collect()
}
fn close<S: Native>(actual: S, expected: S, epsilon: f64) -> bool {
    let (a, e) = (actual.wide(), expected.wide());
    (a.is_nan() && e.is_nan()) || (a - e).abs() < epsilon
}
// One Graph::process call over a whole block, through a separate or an actual
// host-aliased in-place AudioBuffer. Allocation free (usable inside measure).
fn push<S: Native>(
    graph: &mut Graph<S>,
    input: &[Vec<S>; 3],
    output: &mut [Vec<S>; 3],
    host_inplace: bool,
) {
    let frames = input[0].len();
    let inputs = input.each_ref().map(|s| s.as_slice());
    let [a, b, c] = output;
    let mut outputs = [a.as_mut_slice(), b.as_mut_slice(), c.as_mut_slice()];
    let audio = if host_inplace {
        for (view, source) in outputs.iter_mut().zip(input) {
            view.copy_from_slice(source);
        }
        AudioBuffer::in_place(&mut outputs, frames).unwrap()
    } else {
        AudioBuffer::separate(&inputs, &mut outputs, frames).unwrap()
    };
    assert_eq!(
        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
        GraphStatus::Continue
    );
}
fn run<S: Native>(epsilon: f64) {
    for (factor, half, beta) in [
        (Factor::Four, 32, 0.),
        (Factor::Eight, 32, 12.),
        (Factor::Sixteen, 128, 16.),
    ] {
        for host_inplace in [false, true] {
            for inplace in [false, true] {
                let design = Design::new(48000, half, factor, beta).unwrap();
                let (framework_return, control_receiver) = mpsc::channel();
                let frames = Arc::new(AtomicUsize::new(0));
                let meter = MeterNode::<S> {
                    design: design.clone(),
                    bank: None,
                    scratch: std::array::from_fn(|_| vec![S::default(); 67]),
                    config: None,
                    inplace,
                    returned: framework_return,
                    frames: frames.clone(),
                };
                let fir = FirNode::<S> {
                    fir: std::array::from_fn(|_| FirConvolver::new(&TAPS).unwrap()),
                    config: None,
                    _sample: std::marker::PhantomData,
                };
                let routes: Vec<_> = (0..3)
                    .flat_map(|ch| {
                        [
                            Connection {
                                source: Source::Input(ch),
                                destination: Destination::Node {
                                    id: NodeId(0),
                                    channel: ch,
                                },
                                gain: S::narrow(1.),
                            },
                            Connection {
                                source: Source::Node {
                                    id: NodeId(0),
                                    channel: ch,
                                },
                                destination: Destination::Node {
                                    id: NodeId(1),
                                    channel: ch,
                                },
                                gain: S::narrow(1.),
                            },
                            Connection {
                                source: Source::Node {
                                    id: NodeId(1),
                                    channel: ch,
                                },
                                destination: Destination::Output(ch),
                                gain: S::narrow(1.),
                            },
                        ]
                    })
                    .collect();
                let layout = AudioLayout::new(3, 3).unwrap();
                let mut graph = Graph::new(
                    layout,
                    vec![
                        GraphNode {
                            id: NodeId(0),
                            layout,
                            processor: Box::new(meter),
                        },
                        GraphNode {
                            id: NodeId(1),
                            layout,
                            processor: Box::new(fir),
                        },
                    ],
                    &routes,
                )
                .unwrap();
                let input: [Vec<S>; 3] = std::array::from_fn(|ch| {
                    (0usize..769)
                        .map(|n| {
                            S::narrow(
                                0.5 * (n as f64 * (0.173 + ch as f64 * 0.037) + ch as f64 * 0.9)
                                    .sin(),
                            )
                        })
                        .collect()
                });
                let before: [Vec<f64>; 3] = input
                    .each_ref()
                    .map(|s| s.iter().map(|x| x.wide()).collect());
                let peak = before.each_ref().map(|x| direct_peak(&design, x));
                let reference: [Vec<S>; 3] = std::array::from_fn(|ch| {
                    (0usize..769)
                        .map(|n| {
                            S::narrow(
                                TAPS.iter()
                                    .enumerate()
                                    .filter_map(|(j, h)| {
                                        n.checked_sub(j).map(|i| h * before[ch][i])
                                    })
                                    .sum(),
                            )
                        })
                        .collect()
                });
                // The supplied test adapter (not the library) checks rate equality:
                // a mismatched Graph rate is refused by the node's prepare and
                // leaves the graph deactivated with no bank returned.
                assert!(matches!(
                    graph.prepare(ProcessConfig::new(44100., 67, 0, layout).unwrap()),
                    Err(PrepareError::InvalidSampleRate)
                ));
                assert!(control_receiver.try_recv().is_err());
                for _cycle in 0..2 {
                    graph
                        .prepare(ProcessConfig::new(48000., 67, 0, layout).unwrap())
                        .unwrap();
                    let mut output: [Vec<S>; 3] =
                        std::array::from_fn(|_| vec![S::narrow(99.); 772]);
                    if host_inplace {
                        for ch in 0..3 {
                            output[ch][..769].copy_from_slice(&input[ch]);
                        }
                    }
                    let mut done = 0;
                    let mut turn = 0;
                    let info = allocation_counter::measure(|| {
                        while done < 769 {
                            let n = [1, 67, 0, 2, 31, 5][turn % 6].min(769 - done);
                            turn += 1;
                            let inputs = input.each_ref().map(|s| &s[done..]);
                            let [a, b, c] = &mut output;
                            let mut outputs = [&mut a[done..], &mut b[done..], &mut c[done..]];
                            let audio = if host_inplace {
                                AudioBuffer::in_place(&mut outputs, n).unwrap()
                            } else {
                                AudioBuffer::separate(&inputs, &mut outputs, n).unwrap()
                            };
                            assert_eq!(
                                graph.process(&mut ProcessBlock::new(
                                    audio,
                                    &[],
                                    Transport::default()
                                )),
                                GraphStatus::Continue
                            );
                            done += n;
                        }
                    });
                    assert_eq!(info.count_total, 0);
                    assert_eq!(info.count_current, 0);
                    assert_eq!(info.bytes_total, 0);
                    assert_eq!(frames.load(Ordering::Relaxed), 769);
                    for ch in 0..3 {
                        assert!(output[ch][769..].iter().all(|x| x.wide() == 99.));
                        for (&actual, expected) in output[ch][..769].iter().zip(&reference[ch]) {
                            assert!((actual.wide() - expected.wide()).abs() < epsilon);
                        }
                    }
                    assert_eq!(
                        input
                            .each_ref()
                            .map(|s| s.iter().map(|x| x.wide()).collect::<Vec<_>>()),
                        before
                    );
                    graph.deactivate();
                    let mut owned = control_receiver.try_recv().unwrap();
                    assert_eq!(owned.source_frames(), 769);
                    let info = allocation_counter::measure(|| {
                        owned.finish();
                    });
                    assert_eq!(info.count_total, 0);
                    assert_eq!(info.count_current, 0);
                    for (ch, &expected) in peak.iter().enumerate() {
                        let r = owned.reading(ch).unwrap();
                        assert!((r.interpolated_peak - expected).abs() < 3e-15);
                        assert_eq!(r.source_frames, 769);
                        assert_eq!(r.tail_frames, (2 * half) as u64);
                        assert!(!r.nonfinite_input);
                    }
                }
                // Poison block: NaN on channel 0 only (frame 1), finite nonzero
                // neighbours. FIR taps are 5 long, so ch0 output is NaN for frames
                // 1..=3 while frame 0 is still finite.
                let poison: [Vec<S>; 3] = [
                    [0.25, f64::NAN, 0.5, -0.125]
                        .iter()
                        .map(|&x| S::narrow(x))
                        .collect(),
                    [0.6, -0.4, 0.2, 0.1]
                        .iter()
                        .map(|&x| S::narrow(x))
                        .collect(),
                    [-0.7, 0.3, 0.9, -0.2]
                        .iter()
                        .map(|&x| S::narrow(x))
                        .collect(),
                ];
                let poison_reference: [Vec<S>; 3] = poison.each_ref().map(|s| fir_reference(s));
                let poison_wide: [Vec<f64>; 3] = poison
                    .each_ref()
                    .map(|s| s.iter().map(|x| x.wide()).collect());
                // Episode 1 (A2/A3): the NaN-poisoned bank is deactivated and
                // reclaimed BEFORE any reset can erase its flags.
                graph
                    .prepare(ProcessConfig::new(48000., 67, 0, layout).unwrap())
                    .unwrap();
                let mut out: [Vec<S>; 3] = std::array::from_fn(|_| vec![S::narrow(99.); 4]);
                let info = allocation_counter::measure(|| {
                    push(&mut graph, &poison, &mut out, host_inplace);
                });
                assert_eq!(info.count_total, 0);
                assert_eq!(info.count_current, 0);
                assert_eq!(info.bytes_total, 0);
                assert_eq!(frames.load(Ordering::Relaxed), 4);
                // Audio propagation: the NaN passes untouched through the meter
                // into the FIR, whose ring then carries it for its five taps.
                assert!(out[0][0].wide().is_finite());
                assert!(out[0][1..].iter().all(|x| x.wide().is_nan()));
                for ch in 0..3 {
                    for (&actual, &expected) in out[ch].iter().zip(&poison_reference[ch]) {
                        assert!(close(actual, expected, epsilon), "ch{ch}");
                    }
                }
                graph.deactivate();
                let mut owned = control_receiver.try_recv().unwrap();
                assert!(control_receiver.try_recv().is_err(), "exactly one bank");
                assert_eq!(owned.source_frames(), 4);
                for ch in 0..3 {
                    let r = owned.reading(ch).unwrap();
                    assert_eq!(r.nonfinite_input, ch == 0, "live flag ch{ch}");
                    assert!(!r.amplitude_overflow);
                }
                assert_eq!(owned.reading(0).unwrap().sample_peak, 0.5);
                owned.finish();
                for (ch, poison_channel) in poison_wide.iter().enumerate() {
                    let r = owned.reading(ch).unwrap();
                    assert_eq!(r.nonfinite_input, ch == 0, "finished flag ch{ch}");
                    assert!(!r.amplitude_overflow);
                    assert_eq!(r.source_frames, 4);
                    assert_eq!(r.tail_frames, (2 * half) as u64);
                    // Isolation: each channel equals an independent meter fed the
                    // same delivered samples (NaN included for channel 0).
                    let mut meter = Meter::new(&design, 4).unwrap();
                    meter.observe(poison_channel).unwrap();
                    meter.finish();
                    assert_eq!(r, meter.reading(), "meter ch{ch}");
                    // NaN is measurement silence: channel 0 equals the direct peak
                    // of the stream with the NaN replaced by zero.
                    let mut measured = poison_channel.clone();
                    measured.iter_mut().for_each(|x| {
                        if x.is_nan() {
                            *x = 0.;
                        }
                    });
                    assert!(
                        (r.interpolated_peak - direct_peak(&design, &measured)).abs() < 3e-15,
                        "direct ch{ch}"
                    );
                }
                // Episode 2 (A3): separate reset episode after re-preparing. Poison,
                // reset through the Graph, then compare ALL post-reset FIR outputs
                // with direct convolution from zero history.
                graph
                    .prepare(ProcessConfig::new(48000., 67, 0, layout).unwrap())
                    .unwrap();
                let mut out: [Vec<S>; 3] = std::array::from_fn(|_| vec![S::narrow(99.); 4]);
                push(&mut graph, &poison, &mut out, host_inplace);
                assert!(out[0][1..].iter().all(|x| x.wide().is_nan()));
                assert_eq!(frames.load(Ordering::Relaxed), 4);
                let info = allocation_counter::measure(|| {
                    graph.reset();
                });
                assert_eq!(info.count_total, 0);
                assert_eq!(info.count_current, 0);
                assert_eq!(frames.load(Ordering::Relaxed), 0);
                let input: [Vec<S>; 3] = std::array::from_fn(|ch| {
                    (0..12)
                        .map(|n| S::narrow(0.8 * (n as f64 * 0.61 + ch as f64 * 1.3 + 0.2).sin()))
                        .collect()
                });
                let reference: [Vec<S>; 3] = input.each_ref().map(|s| fir_reference(s));
                let mut output: [Vec<S>; 3] = std::array::from_fn(|_| vec![S::default(); 12]);
                let info = allocation_counter::measure(|| {
                    push(&mut graph, &input, &mut output, host_inplace);
                });
                assert_eq!(info.count_total, 0);
                assert_eq!(info.count_current, 0);
                assert_eq!(info.bytes_total, 0);
                assert_eq!(frames.load(Ordering::Relaxed), 12);
                for ch in 0..3 {
                    for (n, (&actual, &expected)) in
                        output[ch].iter().zip(&reference[ch]).enumerate()
                    {
                        assert!(actual.wide().is_finite());
                        assert!(
                            close(actual, expected, epsilon),
                            "post-reset FIR ch{ch} n{n}: {} vs {}",
                            actual.wide(),
                            expected.wide()
                        );
                    }
                }
                graph.deactivate();
                let mut owned = control_receiver.try_recv().unwrap();
                assert!(control_receiver.try_recv().is_err(), "exactly one bank");
                assert_eq!(owned.source_frames(), 12);
                assert!(
                    (0..3).all(|ch| !owned.reading(ch).unwrap().nonfinite_input),
                    "reset cleared the latched flag"
                );
                owned.finish();
                for (ch, input_channel) in input.iter().enumerate() {
                    let r = owned.reading(ch).unwrap();
                    assert!(!r.nonfinite_input && !r.amplitude_overflow);
                    assert_eq!(r.source_frames, 12);
                    assert_eq!(r.tail_frames, (2 * half) as u64);
                    assert_eq!(
                        r.phase_observations,
                        (12 + 2 * half) as u64 * factor.factor() as u64
                    );
                    let source: Vec<f64> = input_channel.iter().map(|x| x.wide()).collect();
                    assert!((r.interpolated_peak - direct_peak(&design, &source)).abs() < 3e-15);
                }
                println!(
                    "GRAPH true-peak native{} factor={} half={half} beta={beta} meter_inplace={inplace} graph_inplace={host_inplace};769 source frames, exact stopped owner+tail, two lifecycle cycles, independent direct peak/FIR; separate NaN episode (flags read after deactivate, before reset, isolated channel peaks, NaN propagated) and separate reset episode (all 36 post-reset FIR values vs zero-history convolution); no callback heap",
                    size_of::<S>() * 8,
                    factor.factor()
                );
            }
        }
    }
}
#[test]
fn actual_native64_foundation_graph_meter_to_fir_and_stopped_tail() {
    run::<f64>(3e-15);
}
#[test]
fn actual_native32_foundation_graph_meter_to_fir_and_stopped_tail() {
    run::<f32>(3e-7);
}
