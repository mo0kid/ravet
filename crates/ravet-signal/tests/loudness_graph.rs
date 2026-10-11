use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor,
    Sample, Transport,
};
use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
use ravet_signal::{
    FirConvolver, KWeightingDesign as Design, LoudnessChannelRole as Role,
    LoudnessRecordCapacities as Capacities, LoudnessTailPolicy as Tail,
    PreparedLoudnessMeter64 as Bank,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Sender},
};
const ROLES: [Role; 3] = [Role::Front, Role::Surround, Role::Excluded];
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
        self.bank = Some(
            Bank::new(
                &self.design,
                &ROLES,
                67,
                Capacities {
                    integrated: 512,
                    lra: 256,
                },
                true,
            )
            .expect("test CONTROL allocation"),
        );
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
// Independent direct-form I histories and absolute source-coordinate windows.
fn records(d: Design, input: &[Vec<f64>; 3], grids: usize) -> Vec<f64> {
    let source: [Vec<f64>; 3] = std::array::from_fn(|ch| {
        let mut result = input[ch].clone();
        for stage in 0..2 {
            let (b, a) = d.section(stage).unwrap();
            let (mut x, mut y) = ([0.; 2], [0.; 2]);
            for sample in &mut result {
                let original = *sample;
                let out = b[0] * original + b[1] * x[0] + b[2] * x[1] - a[0] * y[0] - a[1] * y[1];
                x = [original, x[0]];
                y = [out, y[0]];
                *sample = out;
            }
        }
        result
    });
    let endpoint = |k: usize| k * d.sample_rate() as usize / 10;
    (grids..)
        .map_while(|k| {
            let end = endpoint(k);
            if end > input[0].len() {
                return None;
            }
            let start = endpoint(k - grids);
            let power: f64 = source
                .iter()
                .zip(ROLES)
                .map(|(x, role)| {
                    role.power_weight() * x[start..end].iter().map(|v| v * v).sum::<f64>()
                })
                .sum();
            Some(-0.691 + 10. * (power / (end - start) as f64).log10())
        })
        .collect()
}
fn compare(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 2e-10, "{a} vs {e}");
    }
}
fn gated(input: &[f64], inclusive: bool, offset: f64) -> (Vec<f64>, f64) {
    let absolute: Vec<_> = input
        .iter()
        .copied()
        .filter(|x| if inclusive { *x >= -70. } else { *x > -70. })
        .collect();
    let mean = absolute.iter().map(|x| 10_f64.powf(*x / 10.)).sum::<f64>() / absolute.len() as f64;
    let gate = 10. * mean.log10() - offset;
    (
        absolute
            .into_iter()
            .filter(|x| if inclusive { *x >= gate } else { *x > gate })
            .collect(),
        gate,
    )
}
fn run<S: Native>(epsilon: f64) {
    for rate in [8000, 8001, 48007] {
        let length = 3 * rate as usize + 67;
        for inplace in [false, true] {
            for external_inplace in [false, true] {
                let design = Design::new(rate).unwrap();
                let (framework_return, control_receiver) = mpsc::channel();
                let frames = Arc::new(AtomicUsize::new(0));
                let meter = MeterNode::<S> {
                    design,
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
                    (0usize..length)
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
                let expected_i = records(design, &before, 4);
                let mut padded = before.clone();
                let tail = (3 * rate as usize).div_ceil(2);
                for channel in &mut padded {
                    channel.resize(length + tail, 0.);
                }
                let expected_lra = records(design, &padded, 30);
                let reference: [Vec<S>; 3] = std::array::from_fn(|ch| {
                    (0usize..length)
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
                for _cycle in 0..2 {
                    graph
                        .prepare(ProcessConfig::new(rate as f64, 67, 0, layout).unwrap())
                        .unwrap();
                    let mut output: [Vec<S>; 3] =
                        std::array::from_fn(|_| vec![S::narrow(99.); length + 3]);
                    let mut done = 0;
                    let mut turn = 0;
                    let info = allocation_counter::measure(|| {
                        while done < length {
                            let n = [1, 67, 0, 2, 31, 5][turn % 6].min(length - done);
                            turn += 1;
                            let inputs = input.each_ref().map(|s| &s[done..]);
                            let [a, b, c] = &mut output;
                            let mut outputs = [&mut a[done..], &mut b[done..], &mut c[done..]];
                            let audio = if external_inplace {
                                for ch in 0..3 {
                                    outputs[ch][..n].copy_from_slice(&inputs[ch][..n]);
                                }
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
                    assert_eq!(frames.load(Ordering::Relaxed), length);
                    for ch in 0..3 {
                        assert!(output[ch][length..].iter().all(|x| x.wide() == 99.));
                        for (&actual, expected) in output[ch][..length].iter().zip(&reference[ch]) {
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
                    compare(owned.integrated_records(), &expected_i);
                    assert_eq!(owned.reading().source_frames, length as u64);
                    let info = allocation_counter::measure(|| {
                        owned.finish(Tail::LraSilence1500ms).unwrap();
                        owned.analyse_programme().unwrap();
                    });
                    assert_eq!(info.count_total, 0);
                    assert_eq!(info.count_current, 0);
                    compare(owned.lra_records(), &expected_lra);
                    let r = owned.reading();
                    assert_eq!(r.source_frames, length as u64);
                    assert_eq!(r.tail_frames, tail as u64);
                    assert!(!r.nonfinite_input);
                    assert!(r.numeric_fault.is_none());
                    let report = owned.analyse_programme().unwrap();
                    let (admitted, gate) = gated(&expected_i, false, 10.);
                    let integrated = 10.
                        * (admitted.iter().map(|x| 10_f64.powf(*x / 10.)).sum::<f64>()
                            / admitted.len() as f64)
                            .log10();
                    assert!((report.integrated_lufs.unwrap() - integrated).abs() < 2e-10);
                    assert!((report.integrated_relative_gate_lufs.unwrap() - gate).abs() < 2e-10);
                    let (mut admitted, gate) = gated(&expected_lra, true, 20.);
                    admitted.sort_by(f64::total_cmp);
                    let rank = |p: usize| ((admitted.len() - 1) * p + 50) / 100;
                    let range = admitted[rank(95)] - admitted[rank(10)];
                    assert!((report.loudness_range_lu.unwrap() - range).abs() < 2e-10);
                    assert!((report.lra_relative_gate_lufs.unwrap() - gate).abs() < 2e-10);
                    assert_eq!(report.source_frames, length as u64);
                    assert_eq!(report.lra_relative_count, admitted.len());
                }
                graph
                    .prepare(ProcessConfig::new(rate as f64, 67, 0, layout).unwrap())
                    .unwrap();
                let poison = [
                    vec![S::narrow(f64::NAN)],
                    vec![S::narrow(-0.3)],
                    vec![S::narrow(0.7)],
                ];
                let mut out: [Vec<S>; 3] = std::array::from_fn(|_| vec![S::default(); 1]);
                let inputs = poison.each_ref().map(|s| s.as_slice());
                let [a, b, c] = &mut out;
                let mut outputs = [a.as_mut_slice(), b.as_mut_slice(), c.as_mut_slice()];
                let audio = AudioBuffer::separate(&inputs, &mut outputs, 1).unwrap();
                assert_eq!(
                    graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                    GraphStatus::Continue
                );
                assert!(out[0][0].wide().is_nan());
                assert!(out[1][0].wide().is_finite());
                assert!(out[2][0].wide().is_finite());
                for ch in 1..3 {
                    assert!(
                        (out[ch][0].wide() - S::narrow(TAPS[0] * poison[ch][0].wide()).wide())
                            .abs()
                            < epsilon
                    );
                }
                // Reclaim before reset so the real Graph callback's poison flag
                // cannot disappear behind lifecycle cleanup. The public meter has
                // one aggregate flag, not per-channel flags.
                graph.deactivate();
                let mut poisoned = control_receiver.try_recv().unwrap();
                assert!(poisoned.reading().nonfinite_input);
                assert!(poisoned.reading().numeric_fault.is_none());
                assert_eq!(poisoned.reading().source_frames, 1);
                let window = (4 * rate as usize) / 10;
                let mut sanitized: [Vec<f64>; 3] = std::array::from_fn(|_| vec![0.; window]);
                sanitized[1][0] = poison[1][0].wide();
                sanitized[2][0] = poison[2][0].wide();
                let zeros = [vec![0.; 67], vec![0.; 67], vec![0.; 67]];
                let refs = zeros.each_ref().map(|s| s.as_slice());
                let mut remaining = window - 1;
                while remaining != 0 {
                    let n = remaining.min(67);
                    poisoned.observe_planar(&refs, n).unwrap();
                    remaining -= n;
                }
                // Direct-form I computes each neighbour's retained history;
                // only the nonfinite source sample is replaced with measurement zero.
                compare(
                    poisoned.integrated_records(),
                    &records(design, &sanitized, 4),
                );
                assert!(poisoned.reading().nonfinite_input);
                assert!(poisoned.reading().numeric_fault.is_none());
                assert_eq!(poisoned.reading().source_frames, window as u64);
                graph
                    .prepare(ProcessConfig::new(rate as f64, 67, 0, layout).unwrap())
                    .unwrap();
                // Poison the newly prepared instance again, then exercise actual
                // Graph reset before checking independent FIR output below.
                let inputs = poison.each_ref().map(|s| s.as_slice());
                let [a, b, c] = &mut out;
                let mut outputs = [a.as_mut_slice(), b.as_mut_slice(), c.as_mut_slice()];
                let audio = AudioBuffer::separate(&inputs, &mut outputs, 1).unwrap();
                assert_eq!(
                    graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                    GraphStatus::Continue
                );
                assert!(out[0][0].wide().is_nan());
                graph.reset();
                let input = [
                    vec![S::narrow(0.2); 12],
                    vec![S::narrow(-0.3); 12],
                    vec![S::narrow(0.7); 12],
                ];
                let mut output: [Vec<S>; 3] = std::array::from_fn(|_| vec![S::default(); 12]);
                let inputs = input.each_ref().map(|s| s.as_slice());
                let [a, b, c] = &mut output;
                let mut outputs = [a.as_mut_slice(), b.as_mut_slice(), c.as_mut_slice()];
                let audio = AudioBuffer::separate(&inputs, &mut outputs, 12).unwrap();
                let info = allocation_counter::measure(|| {
                    assert_eq!(
                        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        GraphStatus::Continue
                    );
                });
                assert_eq!(info.count_total, 0);
                assert_eq!(info.count_current, 0);
                assert!(output.iter().flatten().all(|x| x.wide().is_finite()));
                graph.deactivate();
                let mut owned = control_receiver.try_recv().unwrap();
                owned.finish(Tail::None).unwrap();
                let reading = owned.reading();
                assert_eq!(reading.source_frames, 12);
                assert!(!reading.nonfinite_input);
                assert!(reading.numeric_fault.is_none());
                assert!(reading.momentary_lufs.is_none());
                assert!(owned.analyse_programme().unwrap().integrated_lufs.is_none());
                for ch in 0..3 {
                    for (n, actual) in output[ch].iter().enumerate() {
                        let expected: f64 = TAPS
                            .iter()
                            .enumerate()
                            .filter_map(|(j, h)| n.checked_sub(j).map(|i| h * input[ch][i].wide()))
                            .sum();
                        assert!((actual.wide() - S::narrow(expected).wide()).abs() < epsilon);
                    }
                }
                println!(
                    "GRAPH loudness native{} rate={rate} meter_inplace={inplace} foundation_inplace={external_inplace}; {length} original/{tail} artificial frames, exact stopped owner, two cycles, independent direct-I absolute-grid records/gates/FIR, NaN raw-preserved, poisoned stopped owner and independent neighbour histories, reprepare/repoison then actual reset; no callback heap",
                    size_of::<S>() * 8
                );
            }
        }
    }
}
#[test]
fn actual_native64_foundation_graph_loudness_to_fir_and_stopped_analysis() {
    run::<f64>(3e-15);
}
#[test]
fn actual_native32_foundation_graph_loudness_to_fir_and_stopped_analysis() {
    run::<f32>(3e-7);
}

// ---------------------------------------------------------------------------------------
// Follow-up episodes. Each runs the full 24-combination matrix per native type: three
// rates, both meter views (`inplace`) and BOTH external buffer shapes (separate and
// in-place). The poison/reset episodes inside `run` above use separate external buffers
// only; these are the distinct external in-place cases. Heap measurement covers Graph
// callbacks and Graph::reset; CONTROL (prepare, deactivate, analysis) stays outside.
fn config(rate: u32) -> ProcessConfig {
    ProcessConfig::new(rate as f64, 67, 0, AudioLayout::new(3, 3).unwrap()).unwrap()
}
fn build<S: Native>(
    rate: u32,
    inplace: bool,
) -> (Graph<S>, mpsc::Receiver<Bank>, Arc<AtomicUsize>) {
    let (framework_return, control_receiver) = mpsc::channel();
    let frames = Arc::new(AtomicUsize::new(0));
    let meter = MeterNode::<S> {
        design: Design::new(rate).unwrap(),
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
    let graph = Graph::new(
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
    (graph, control_receiver, frames)
}
fn tone<S: Native>(length: usize) -> [Vec<S>; 3] {
    std::array::from_fn(|ch| {
        (0usize..length)
            .map(|n| {
                S::narrow(0.5 * (n as f64 * (0.173 + ch as f64 * 0.037) + ch as f64 * 0.9).sin())
            })
            .collect()
    })
}
fn widen<S: Native>(input: &[Vec<S>; 3]) -> [Vec<f64>; 3] {
    input
        .each_ref()
        .map(|channel| channel.iter().map(|x| x.wide()).collect())
}
// Independent FIR reference over finite widened input, narrowed like the node output.
fn fir_reference<S: Native>(source: &[Vec<f64>; 3]) -> [Vec<S>; 3] {
    source.each_ref().map(|channel| {
        (0..channel.len())
            .map(|n| {
                S::narrow(
                    TAPS.iter()
                        .enumerate()
                        .filter_map(|(j, h)| n.checked_sub(j).map(|i| h * channel[i]))
                        .sum(),
                )
            })
            .collect()
    })
}
fn near<S: Native>(actual: S, expected: S, epsilon: f64) -> bool {
    (actual.wide() - expected.wide()).abs() < epsilon
}
// Process `range` of `input` through the Graph in variable and zero-length callbacks.
// No allocation: all buffers are supplied by the caller, outside any measured closure.
fn drive<S: Native>(
    graph: &mut Graph<S>,
    input: &[Vec<S>; 3],
    output: &mut [Vec<S>; 3],
    range: std::ops::Range<usize>,
    external_inplace: bool,
) {
    let mut done = range.start;
    let mut turn = 0;
    while done < range.end {
        let n = [1, 67, 0, 2, 31, 5][turn % 6].min(range.end - done);
        turn += 1;
        let inputs = input.each_ref().map(|s| &s[done..]);
        let [a, b, c] = &mut *output;
        let mut outputs = [&mut a[done..], &mut b[done..], &mut c[done..]];
        let audio = if external_inplace {
            for (target, source) in outputs.iter_mut().zip(&inputs) {
                target[..n].copy_from_slice(&source[..n]);
            }
            AudioBuffer::in_place(&mut outputs, n).unwrap()
        } else {
            AudioBuffer::separate(&inputs, &mut outputs, n).unwrap()
        };
        assert_eq!(
            graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
            GraphStatus::Continue
        );
        done += n;
    }
}
fn reset_run<S: Native>(epsilon: f64) {
    for rate in [8000, 8001, 48007] {
        let design = Design::new(rate).unwrap();
        let window = 4 * rate as usize / 10;
        // 123 frames of a fifth block stay in the partial grid energy at reset time.
        let history = window + 123;
        // Nine whole blocks: record 5 covers blocks 5..9, wholly after the history.
        let total = 9 * rate as usize / 10;
        let mut input: [Vec<S>; 3] = tone(history);
        for channel in &mut input {
            channel.resize(total, S::default());
        }
        let before = widen(&input);
        let reference: [Vec<S>; 3] = fir_reference(&before);
        for inplace in [false, true] {
            for external_inplace in [false, true] {
                // Control: same history then zeros, never reset. Its finite ringing record
                // proves the Graph left nonzero K memory and nonzero FIR history behind.
                let (mut graph, receiver, _) = build::<S>(rate, inplace);
                graph.prepare(config(rate)).unwrap();
                let mut output: [Vec<S>; 3] = std::array::from_fn(|_| vec![S::narrow(99.); total]);
                drive(&mut graph, &input, &mut output, 0..total, external_inplace);
                for (actual, expected) in output.iter().zip(&reference) {
                    assert!(
                        actual
                            .iter()
                            .zip(expected)
                            .all(|(&a, &e)| near(a, e, epsilon))
                    );
                    assert!(
                        actual[history..history + 4].iter().any(|x| x.wide() != 0.),
                        "FIR history is nonzero after the audio ends"
                    );
                }
                graph.deactivate();
                let control = receiver.try_recv().unwrap();
                assert_eq!(control.reading().source_frames, total as u64);
                let ring = control.integrated_records();
                assert_eq!(ring.len(), 6);
                compare(&ring[..5], &records(design, &before, 4)[..5]);
                assert!(ring[5].is_finite(), "pure K ringing record {}", ring[5]);
                // Subject: same history, actual Graph reset, then exactly one zero window.
                let (mut graph, receiver, frames) = build::<S>(rate, inplace);
                graph.prepare(config(rate)).unwrap();
                let mut output: [Vec<S>; 3] = std::array::from_fn(|_| vec![S::narrow(99.); total]);
                drive(
                    &mut graph,
                    &input,
                    &mut output,
                    0..history,
                    external_inplace,
                );
                assert_eq!(frames.load(Ordering::Relaxed), history);
                graph.reset();
                assert_eq!(frames.load(Ordering::Relaxed), 0);
                drive(
                    &mut graph,
                    &input,
                    &mut output,
                    history..history + window,
                    external_inplace,
                );
                assert_eq!(frames.load(Ordering::Relaxed), window);
                for (actual, expected) in output.iter().zip(&reference) {
                    assert!(
                        actual[..history]
                            .iter()
                            .zip(expected)
                            .all(|(&a, &e)| near(a, e, epsilon))
                    );
                    assert!(
                        actual[history..history + window]
                            .iter()
                            .all(|x| x.wide() == 0.),
                        "stale FIR history after Graph::reset"
                    );
                    assert!(actual[history + window..].iter().all(|x| x.wide() == 99.));
                }
                graph.deactivate();
                let owned = receiver.try_recv().unwrap();
                let reading = owned.reading();
                assert_eq!(reading.source_frames, window as u64);
                assert!(!reading.nonfinite_input && reading.numeric_fault.is_none());
                assert_eq!(
                    owned.integrated_records(),
                    [f64::NEG_INFINITY],
                    "rate={rate} inplace={inplace} external_inplace={external_inplace}: \
                     stale K history or grid would give a finite or early record"
                );
                assert_eq!(reading.momentary_lufs, Some(f64::NEG_INFINITY));
                let mut fresh = Bank::new(
                    &design,
                    &ROLES,
                    67,
                    Capacities {
                        integrated: 512,
                        lra: 256,
                    },
                    true,
                )
                .unwrap();
                let zeros = [vec![0.; 67], vec![0.; 67], vec![0.; 67]];
                let refs = zeros.each_ref().map(|s| s.as_slice());
                let mut remaining = window;
                while remaining != 0 {
                    let n = remaining.min(67);
                    fresh.observe_planar(&refs, n).unwrap();
                    remaining -= n;
                }
                assert_eq!(reading, fresh.reading());
                assert_eq!(owned.lra_records(), fresh.lra_records());
            }
        }
    }
}
#[test]
fn actual_native64_graph_reset_after_nonzero_history_makes_zero_record_negative_infinity() {
    reset_run::<f64>(3e-15);
}
#[test]
fn actual_native32_graph_reset_after_nonzero_history_makes_zero_record_negative_infinity() {
    reset_run::<f32>(3e-7);
}
// Nonfinite positions in the poisoned channel: consecutive NaN, +inf, -inf and a later NaN.
const BAD: [(usize, f64); 5] = [
    (3, f64::NAN),
    (4, f64::NAN),
    (101, f64::INFINITY),
    (202, f64::NEG_INFINITY),
    (1500, f64::NAN),
];
fn poison_run<S: Native>(epsilon: f64, poisoned: usize) {
    for rate in [8000, 8001, 48007] {
        let design = Design::new(rate).unwrap();
        let window = 4 * rate as usize / 10;
        let length = window + 100;
        let clean = tone::<S>(length);
        let mut input = clean.clone();
        for (frame, value) in BAD {
            input[poisoned][frame] = S::narrow(value);
        }
        let bits = |a: &[Vec<S>; 3]| {
            a.each_ref()
                .map(|c| c.iter().map(|x| x.wide().to_bits()).collect::<Vec<_>>())
        };
        let input_bits = bits(&input);
        // Measurement silence: only the nonfinite samples become zero, other history stays.
        let sanitized = widen(&input).map(|channel| {
            channel
                .into_iter()
                .map(|x| if x.is_finite() { x } else { 0. })
                .collect::<Vec<_>>()
        });
        let reference: [Vec<S>; 3] = fir_reference(&widen(&clean));
        let clean_length = window + 5;
        let recovery = tone::<S>(clean_length);
        let recovery_reference: [Vec<S>; 3] = fir_reference(&widen(&recovery));
        for inplace in [false, true] {
            for external_inplace in [false, true] {
                let label = format!(
                    "rate={rate} channel={poisoned} inplace={inplace} external_inplace={external_inplace}"
                );
                let (mut graph, receiver, _) = build::<S>(rate, inplace);
                graph.prepare(config(rate)).unwrap();
                let mut output: [Vec<S>; 3] = std::array::from_fn(|_| vec![S::default(); length]);
                let info = allocation_counter::measure(|| {
                    drive(&mut graph, &input, &mut output, 0..length, external_inplace);
                });
                assert_eq!(info.count_total, 0, "{label}");
                assert_eq!(info.count_current, 0, "{label}");
                assert_eq!(info.bytes_total, 0, "{label}");
                assert_eq!(bits(&input), input_bits, "{label}: source audio untouched");
                for (ch, (actual, expected)) in output.iter().zip(&reference).enumerate() {
                    if ch == poisoned {
                        for (frame, _) in BAD {
                            assert!(!actual[frame].wide().is_finite(), "{label} frame {frame}");
                        }
                    } else {
                        // Every frame, including those after each poisoned frame.
                        for (frame, (&a, &e)) in actual.iter().zip(expected).enumerate() {
                            assert!(
                                a.wide().is_finite() && near(a, e, epsilon),
                                "{label}: channel {ch} frame {frame}"
                            );
                        }
                    }
                }
                graph.deactivate();
                let owned = receiver.try_recv().unwrap();
                let reading = owned.reading();
                assert!(reading.nonfinite_input, "{label}");
                assert!(reading.numeric_fault.is_none(), "{label}");
                assert_eq!(reading.source_frames, length as u64);
                compare(owned.integrated_records(), &records(design, &sanitized, 4));
                // Re-prepare, poison the first 70 frames, then measure actual Graph::reset
                // followed by clean audio. Buffers were all allocated above.
                graph.prepare(config(rate)).unwrap();
                let mut poison_output: [Vec<S>; 3] =
                    std::array::from_fn(|_| vec![S::default(); 70]);
                let mut recovery_output: [Vec<S>; 3] =
                    std::array::from_fn(|_| vec![S::narrow(99.); clean_length]);
                let info = allocation_counter::measure(|| {
                    drive(
                        &mut graph,
                        &input,
                        &mut poison_output,
                        0..70,
                        external_inplace,
                    );
                    graph.reset();
                    drive(
                        &mut graph,
                        &recovery,
                        &mut recovery_output,
                        0..clean_length,
                        external_inplace,
                    );
                });
                assert_eq!(info.count_total, 0, "{label}");
                assert_eq!(info.count_current, 0, "{label}");
                assert_eq!(info.bytes_total, 0, "{label}");
                assert!(!poison_output[poisoned][3].wide().is_finite(), "{label}");
                for (actual, expected) in recovery_output.iter().zip(&recovery_reference) {
                    assert!(
                        actual
                            .iter()
                            .zip(expected)
                            .all(|(&a, &e)| a.wide().is_finite() && near(a, e, epsilon)),
                        "{label}: FIR must recover after Graph::reset"
                    );
                }
                graph.deactivate();
                let owned = receiver.try_recv().unwrap();
                let reading = owned.reading();
                assert!(!reading.nonfinite_input, "{label}: flag cleared by reset");
                assert!(reading.numeric_fault.is_none(), "{label}");
                assert_eq!(reading.source_frames, clean_length as u64);
                compare(
                    owned.integrated_records(),
                    &records(design, &widen(&recovery), 4),
                );
            }
        }
    }
}
#[test]
fn actual_native64_graph_multiframe_nonfinite_poison_front_and_excluded_channels() {
    for channel in [0, 2] {
        poison_run::<f64>(3e-15, channel);
    }
}
#[test]
fn actual_native32_graph_multiframe_nonfinite_poison_front_and_excluded_channels() {
    for channel in [0, 2] {
        poison_run::<f32>(3e-7, channel);
    }
}
