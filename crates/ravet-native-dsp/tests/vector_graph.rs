use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor,
    Sample, Transport,
};
use ravet_graph::{
    Connection, Destination, Graph, GraphNode, GraphSample, GraphStatus, NodeId, Source,
};
use ravet_native_dsp::{PreparedVectorOps32, PreparedVectorOps64, VectorBackend as Backend};
use ravet_signal::FirConvolver;
use std::sync::mpsc::{self, Sender};
const TAPS: [f64; 5] = [0.1, 0.4, -0.2, 0.3, 0.2];
trait Native: Sample + GraphSample + Send + 'static {
    type Ops: Copy + Send;
    fn widen(self) -> f64;
    fn narrow(v: f64) -> Self;
    fn ops(b: Backend) -> Self::Ops;
    fn gain(ops: Self::Ops, input: &[Self], gain: Self, out: &mut [Self], n: usize);
    fn add(ops: Self::Ops, a: &[Self], b: &[Self], out: &mut [Self], n: usize);
}
impl Native for f32 {
    type Ops = PreparedVectorOps32;
    fn widen(self) -> f64 {
        self as f64
    }
    fn narrow(v: f64) -> Self {
        v as f32
    }
    fn ops(b: Backend) -> Self::Ops {
        PreparedVectorOps32::new(b, 67).unwrap()
    }
    fn gain(ops: Self::Ops, input: &[Self], gain: Self, out: &mut [Self], n: usize) {
        ops.gain(input, gain, out, n).unwrap();
    }
    fn add(ops: Self::Ops, a: &[Self], b: &[Self], out: &mut [Self], n: usize) {
        ops.add(a, b, out, n).unwrap();
    }
}
impl Native for f64 {
    type Ops = PreparedVectorOps64;
    fn widen(self) -> f64 {
        self
    }
    fn narrow(v: f64) -> Self {
        v
    }
    fn ops(b: Backend) -> Self::Ops {
        PreparedVectorOps64::new(b, 67).unwrap()
    }
    fn gain(ops: Self::Ops, input: &[Self], gain: Self, out: &mut [Self], n: usize) {
        ops.gain(input, gain, out, n).unwrap();
    }
    fn add(ops: Self::Ops, a: &[Self], b: &[Self], out: &mut [Self], n: usize) {
        ops.add(a, b, out, n).unwrap();
    }
}
struct Mixer<S: Native> {
    ops: S::Ops,
    gains: [S; 3],
    mixes: [S; 3],
    copied: [Vec<S>; 3],
    first: Vec<S>,
    second: Vec<S>,
    config: Option<ProcessConfig>,
    committed: usize,
    returned: Sender<usize>,
}
impl<S: Native> Processor<S> for Mixer<S> {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        if c.layout() != AudioLayout::new(3, 3).unwrap() {
            return Err(PrepareError::UnsupportedLayout);
        }
        if c.maximum_frames() > 67 {
            return Err(PrepareError::InvalidMaximumFrames);
        }
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
            self.copied[ch][..n].copy_from_slice(&b.audio.input(ch).unwrap()[..n]);
        }
        for ch in 0..3 {
            S::gain(
                self.ops,
                &self.copied[ch],
                self.gains[ch],
                &mut self.first,
                n,
            );
            S::gain(
                self.ops,
                &self.copied[(ch + 1) % 3],
                self.mixes[ch],
                &mut self.second,
                n,
            );
            S::add(
                self.ops,
                &self.first,
                &self.second,
                b.audio.output_mut(ch).unwrap(),
                n,
            );
        }
        self.committed += n;
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.committed = 0;
    }
    fn deactivate(&mut self) {
        if self.config.take().is_some() {
            self.returned.send(self.committed).unwrap();
        }
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
                let x = b.audio.input(ch).unwrap()[i];
                let y = self.fir[ch].process_sample(x.widen());
                b.audio.output_mut(ch).unwrap()[i] = S::narrow(y);
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
fn run<S: Native>(tolerance: f64) {
    let layout = AudioLayout::new(3, 3).unwrap();
    let config = ProcessConfig::new(48000., 67, 0, layout).unwrap();
    let gains = [0.37, -0.81, 1.2].map(S::narrow);
    let mixes = [-0.31, 0.2, 0.07].map(S::narrow);
    let input: [Vec<S>; 3] = std::array::from_fn(|ch| {
        (0..1703)
            .map(|n| {
                S::narrow(0.4 * (n as f64 * (0.037 + 0.11 * ch as f64) + 0.7 * ch as f64).sin())
            })
            .collect()
    });
    // Independent per-frame scalar schedule explicitly rounds both products
    // and their sum to the source sample precision before finite convolution.
    let mixed: [Vec<S>; 3] = std::array::from_fn(|ch| {
        (0..1703)
            .map(|n| {
                let a = S::narrow(input[ch][n].widen() * gains[ch].widen());
                let b = S::narrow(input[(ch + 1) % 3][n].widen() * mixes[ch].widen());
                S::narrow(a.widen() + b.widen())
            })
            .collect()
    });
    let expected: [Vec<S>; 3] = std::array::from_fn(|ch| {
        (0usize..1703)
            .map(|n| {
                S::narrow(
                    TAPS.iter()
                        .enumerate()
                        .filter_map(|(j, h)| n.checked_sub(j).map(|i| h * mixed[ch][i].widen()))
                        .sum(),
                )
            })
            .collect()
    });
    for backend in [Backend::Scalar, Backend::native().unwrap()] {
        for inplace in [false, true] {
            let (sender, receiver) = mpsc::channel();
            let mixer = Mixer::<S> {
                ops: S::ops(backend),
                gains,
                mixes,
                copied: std::array::from_fn(|_| vec![S::default(); 67]),
                first: vec![S::default(); 67],
                second: vec![S::default(); 67],
                config: None,
                committed: 0,
                returned: sender,
            };
            let fir = Fir::<S> {
                fir: std::array::from_fn(|_| FirConvolver::new(&TAPS).unwrap()),
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
                    ]
                })
                .collect();
            let mut graph = Graph::new(
                layout,
                vec![
                    GraphNode {
                        id: NodeId(0),
                        layout,
                        processor: Box::new(mixer),
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
                let mut output: [Vec<S>; 3] = std::array::from_fn(|_| vec![S::narrow(99.); 1706]);
                let info = allocation_counter::measure(|| {
                    let (mut done, mut turn) = (0, 0);
                    while done < 1703 {
                        let n = [0, 1, 67, 2, 31, 5][turn % 6].min(1703 - done);
                        turn += 1;
                        let inputs = input.each_ref().map(|v| &v[done..]);
                        let [a, b, c] = &mut output;
                        let mut outputs = [&mut a[done..], &mut b[done..], &mut c[done..]];
                        let audio = if inplace {
                            for ch in 0..3 {
                                outputs[ch][..n].copy_from_slice(&inputs[ch][..n]);
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
                });
                assert_eq!(info.count_total, 0);
                assert_eq!(info.count_current, 0);
                assert_eq!(info.bytes_total, 0);
                for ch in 0..3 {
                    for (a, b) in output[ch][..1703].iter().zip(&expected[ch]) {
                        assert!(
                            (a.widen() - b.widen()).abs() < tolerance,
                            "{backend:?}: {} vs {}",
                            a.widen(),
                            b.widen()
                        );
                    }
                    assert!(output[ch][1703..].iter().all(|x| x.widen() == 99.));
                }
                graph.deactivate();
                assert_eq!(receiver.try_recv().unwrap(), 1703);
            }
            println!(
                "packed Graph native{} selected={backend:?} external_inplace={inplace}: three independent gain/mix channels → actual FIR, independent rounded scalar schedule and convolution, variable/zero67 maximum, two cycles,1703 exact committed frames, no heap",
                size_of::<S>() * 8
            );
        }
    }
}
#[test]
fn actual_native32_graph_packed_gain_mix_to_fir() {
    run::<f32>(1e-7);
}
#[test]
fn actual_native64_graph_packed_gain_mix_to_fir() {
    run::<f64>(3e-15);
}
