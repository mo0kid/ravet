#[path = "support/convolution_processor.rs"]
mod convolution_processor;
use convolution_processor::{ConvolutionProcessor32, ConvolutionProcessor64};
use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Sample, Tail, Transport,
};
use ravet_graph::{
    Connection, Destination, Graph, GraphNode, GraphSample, GraphStatus, NodeId, Source,
};
use ravet_signal::{
    ConvolutionBank32, ConvolutionBank64, ConvolutionPartitioning as Mode, FirConvolver,
};
use std::sync::mpsc::{self, Sender};
const BUDGET: usize = 256 * 1024 * 1024;
const FIR: [f64; 5] = [0.1, 0.4, -0.2, 0.3, 0.2];
trait Native: Sample + GraphSample + Send + 'static {
    type Child: Processor<Self> + Send;
    fn narrow(x: f64) -> Self;
    fn wide(self) -> f64;
    fn child(h: &[Vec<Self>; 3], mode: Mode) -> Self::Child;
}
impl Native for f32 {
    type Child = ConvolutionProcessor32;
    fn narrow(x: f64) -> Self {
        x as f32
    }
    fn wide(self) -> f64 {
        self as f64
    }
    fn child(h: &[Vec<Self>; 3], mode: Mode) -> Self::Child {
        let bank = ConvolutionBank32::new(&h.each_ref().map(|x| &x[..]), mode, 67, BUDGET).unwrap();
        ConvolutionProcessor32::new(bank, 48000.)
            .unwrap_or_else(|_| panic!("finite target profile"))
    }
}
impl Native for f64 {
    type Child = ConvolutionProcessor64;
    fn narrow(x: f64) -> Self {
        x
    }
    fn wide(self) -> f64 {
        self
    }
    fn child(h: &[Vec<Self>; 3], mode: Mode) -> Self::Child {
        let bank = ConvolutionBank64::new(&h.each_ref().map(|x| &x[..]), mode, 67, BUDGET).unwrap();
        ConvolutionProcessor64::new(bank, 48000.)
            .unwrap_or_else(|_| panic!("finite target profile"))
    }
}
struct Counted<S: Native> {
    child: S::Child,
    prepared: bool,
    committed: usize,
    returned: Sender<usize>,
}
impl<S: Native> Processor<S> for Counted<S> {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        self.prepared = false;
        self.child.prepare(c)?;
        self.prepared = true;
        self.committed = 0;
        Ok(())
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        self.child.start_audio()
    }
    fn stop_audio(&mut self) {
        self.child.stop_audio();
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        let n = b.audio.frames();
        let status = self.child.process(b);
        if status == ProcessStatus::Continue {
            self.committed += n;
        }
        status
    }
    fn reset(&mut self) {
        self.child.reset();
        self.committed = 0;
    }
    fn deactivate(&mut self) {
        self.child.deactivate();
        if self.prepared {
            self.prepared = false;
            self.returned.send(self.committed).unwrap();
        }
    }
    fn latency_samples(&self) -> u32 {
        self.child.latency_samples()
    }
    fn tail(&self) -> Tail {
        self.child.tail()
    }
}
struct Fir<S: Native> {
    fir: [FirConvolver; 3],
    config: Option<ProcessConfig>,
    marker: core::marker::PhantomData<S>,
}
impl<S: Native> Processor<S> for Fir<S> {
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
                let x = b.audio.input(ch).unwrap()[i].wide();
                let y = self.fir[ch].process_sample(x);
                b.audio.output_mut(ch).unwrap()[i] = S::narrow(y);
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        for f in &mut self.fir {
            f.reset();
        }
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
}
fn graph<S: Native>(tolerance: f64) {
    let layout = AudioLayout::new(3, 3).unwrap();
    let config = ProcessConfig::new(48000., 67, 0, layout).unwrap();
    let h: [Vec<S>; 3] = std::array::from_fn(|ch| {
        (0..[17, 129, 397][ch])
            .map(|i| S::narrow((i as f64 * 0.31 + ch as f64).cos() * 0.3 / (1. + i as f64)))
            .collect()
    });
    let raw: [Vec<S>; 3] = std::array::from_fn(|ch| {
        (0..1703)
            .map(|i| S::narrow((i as f64 * (0.073 + 0.13 * ch as f64)).sin() * 0.3))
            .collect()
    });
    let mut maximum = 0f64;
    for mode in [
        Mode::UniformDelayed { partition: 64 },
        Mode::ZeroLatency { head: 16 },
        Mode::NonUniform {
            head: 16,
            tail: 128,
        },
    ] {
        let latency = if let Mode::UniformDelayed { partition } = mode {
            partition
        } else {
            0
        };
        let n = 1703 + 396 + latency + 4;
        let input: [Vec<S>; 3] = raw.each_ref().map(|x| {
            (0..n)
                .map(|i| x.get(i).copied().unwrap_or_default())
                .collect()
        });
        // Independent finite source coordinate/convolution. The native engine
        // rounding is qualified against this analytic reference, not reproduced.
        let wet: [Vec<S>; 3] = std::array::from_fn(|ch| {
            (0..n)
                .map(|i| {
                    S::narrow(i.checked_sub(latency).map_or(0., |at| {
                        h[ch]
                            .iter()
                            .enumerate()
                            .filter_map(|(k, h)| {
                                at.checked_sub(k)
                                    .and_then(|j| raw[ch].get(j))
                                    .map(|x| h.wide() * x.wide())
                            })
                            .sum()
                    }))
                })
                .collect()
        });
        let dry_gain = S::narrow(0.2);
        let expected: [Vec<S>; 3] = std::array::from_fn(|ch| {
            (0..n)
                .map(|i| {
                    let filtered = S::narrow(
                        FIR.iter()
                            .enumerate()
                            .filter_map(|(k, h)| i.checked_sub(k).map(|j| *h * wet[ch][j].wide()))
                            .sum(),
                    );
                    let dry = S::narrow(
                        i.checked_sub(latency)
                            .and_then(|j| raw[ch].get(j))
                            .map_or(0., |x| x.wide() * dry_gain.wide()),
                    );
                    S::narrow(filtered.wide() + dry.wide())
                })
                .collect()
        });
        for inplace in [false, true] {
            let (tx, rx) = mpsc::channel();
            let child = Counted::<S> {
                child: S::child(&h, mode),
                prepared: false,
                committed: 0,
                returned: tx,
            };
            let fir = Fir::<S> {
                fir: std::array::from_fn(|_| FirConvolver::new(&FIR).unwrap()),
                config: None,
                marker: core::marker::PhantomData,
            };
            let routes: Vec<_> = (0..3)
                .flat_map(|channel| {
                    [
                        Connection {
                            source: Source::Input(channel),
                            destination: Destination::Node {
                                id: NodeId(0),
                                channel,
                            },
                            gain: S::narrow(1.),
                        },
                        Connection {
                            source: Source::Node {
                                id: NodeId(0),
                                channel,
                            },
                            destination: Destination::Node {
                                id: NodeId(1),
                                channel,
                            },
                            gain: S::narrow(1.),
                        },
                        Connection {
                            source: Source::Node {
                                id: NodeId(1),
                                channel,
                            },
                            destination: Destination::Output(channel),
                            gain: S::narrow(1.),
                        },
                        Connection {
                            source: Source::Input(channel),
                            destination: Destination::Output(channel),
                            gain: dry_gain,
                        },
                    ]
                })
                .collect();
            let mut graph = Graph::new(
                layout,
                vec![
                    GraphNode {
                        id: NodeId(0),
                        layout,
                        processor: Box::new(child),
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
            for _cycle in 0..2 {
                graph.prepare(config).unwrap();
                assert_eq!(graph.latency_samples(), latency as u32);
                let mut output: [Vec<S>; 3] = std::array::from_fn(|_| vec![S::narrow(19.); n + 3]);
                let mut session = graph.start_audio().unwrap();
                let counter = allocation_counter::measure(|| {
                    let (mut done, mut turn) = (0, 0);
                    while done < n {
                        let count = [0, 1, 67, 2, 31, 5][turn % 6].min(n - done);
                        turn += 1;
                        let inputs = input.each_ref().map(|x| &x[done..]);
                        let [a, b, c] = &mut output;
                        let mut outputs = [&mut a[done..], &mut b[done..], &mut c[done..]];
                        let audio = if inplace {
                            for ch in 0..3 {
                                outputs[ch][..count].copy_from_slice(&inputs[ch][..count]);
                            }
                            AudioBuffer::in_place(&mut outputs, count).unwrap()
                        } else {
                            AudioBuffer::separate(&inputs, &mut outputs, count).unwrap()
                        };
                        assert_eq!(
                            session.process(&mut ProcessBlock::new(
                                audio,
                                &[],
                                Transport::default()
                            )),
                            GraphStatus::Continue
                        );
                        done += count;
                    }
                });
                assert_eq!(counter.count_total, 0);
                assert_eq!(counter.count_current, 0);
                assert_eq!(counter.bytes_total, 0);
                drop(session);
                for ch in 0..3 {
                    for (actual, expected) in output[ch][..n].iter().zip(&expected[ch]) {
                        let error = (actual.wide() - expected.wide()).abs();
                        maximum = maximum.max(error);
                        assert!(
                            error <= tolerance * (1. + expected.wide().abs()),
                            "{mode:?} native{} inplace={inplace} actual={} expected={} error={error:e}",
                            size_of::<S>() * 8,
                            actual.wide(),
                            expected.wide()
                        );
                    }
                    assert!(output[ch][n..].iter().all(|x| x.wide() == 19.));
                }
                graph.deactivate();
                assert_eq!(rx.try_recv().unwrap(), n);
            }
        }
    }
    println!(
        "native{} actual convolution→FIR Graph + latency-compensated dry branch: three modes/both external views/two sessions, independent three IRs/tail/coordinates; maxabsolute={maximum:e}",
        size_of::<S>() * 8
    );
}
#[test]
fn actual_native64_graph_modes_latency_tail_and_inplace() {
    graph::<f64>(5e-12);
}
#[test]
fn actual_native32_graph_modes_latency_tail_and_inplace() {
    graph::<f32>(2e-6);
}
