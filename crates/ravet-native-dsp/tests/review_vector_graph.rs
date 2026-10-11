//! W254 r2 review follow-up (G4): the Foundation -> Graph mixer stage is
//! compared exactly with the independent rounded schedule, and a bank variant
//! composes `gain_planar_in_place` + `add_in_place` inside the Graph callback.
//! Only the FIR stage keeps the original tolerances.
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
const FRAMES: usize = 1703;
const MAXIMUM: usize = 67;
trait Native: Sample + GraphSample + Send + 'static {
    type Ops: Copy + Send;
    fn widen(self) -> f64;
    fn narrow(v: f64) -> Self;
    fn ops(b: Backend) -> Self::Ops;
    fn gain(ops: Self::Ops, input: &[Self], gain: Self, out: &mut [Self], n: usize);
    fn add(ops: Self::Ops, a: &[Self], b: &[Self], out: &mut [Self], n: usize);
    fn gain_bank(ops: Self::Ops, bank: &mut [&mut [Self]], gains: &[Self], n: usize);
    fn add_in_place(ops: Self::Ops, input: &[Self], out: &mut [Self], n: usize);
}
macro_rules! native {
    ($s:ty,$ops:ident) => {
        impl Native for $s {
            type Ops = $ops;
            fn widen(self) -> f64 {
                self as f64
            }
            fn narrow(v: f64) -> Self {
                v as $s
            }
            fn ops(b: Backend) -> Self::Ops {
                $ops::new(b, MAXIMUM).unwrap()
            }
            fn gain(ops: Self::Ops, input: &[Self], gain: Self, out: &mut [Self], n: usize) {
                ops.gain(input, gain, out, n).unwrap();
            }
            fn add(ops: Self::Ops, a: &[Self], b: &[Self], out: &mut [Self], n: usize) {
                ops.add(a, b, out, n).unwrap();
            }
            fn gain_bank(ops: Self::Ops, bank: &mut [&mut [Self]], gains: &[Self], n: usize) {
                ops.gain_planar_in_place(bank, gains, n).unwrap();
            }
            fn add_in_place(ops: Self::Ops, input: &[Self], out: &mut [Self], n: usize) {
                ops.add_in_place(input, out, n).unwrap();
            }
        }
    };
}
native!(f32, PreparedVectorOps32);
native!(f64, PreparedVectorOps64);

struct Mixer<S: Native> {
    ops: S::Ops,
    planar: bool,
    gains: [S; 3],
    mixes: [S; 3],
    first: [Vec<S>; 3],
    second: [Vec<S>; 3],
    ga: Vec<S>,
    gb: Vec<S>,
    config: Option<ProcessConfig>,
    committed: usize,
    returned: Sender<usize>,
}
impl<S: Native> Processor<S> for Mixer<S> {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        if c.layout() != AudioLayout::new(3, 3).unwrap() {
            return Err(PrepareError::UnsupportedLayout);
        }
        if c.maximum_frames() > MAXIMUM {
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
        // Copy every input before writing outputs (in-place Graph buffers alias).
        for ch in 0..3 {
            self.first[ch][..n].copy_from_slice(&b.audio.input(ch).unwrap()[..n]);
            self.second[ch][..n].copy_from_slice(&b.audio.input((ch + 1) % 3).unwrap()[..n]);
        }
        if self.planar {
            let [f0, f1, f2] = &mut self.first;
            S::gain_bank(
                self.ops,
                &mut [&mut f0[..], &mut f1[..], &mut f2[..]],
                &self.gains,
                n,
            );
            let [s0, s1, s2] = &mut self.second;
            S::gain_bank(
                self.ops,
                &mut [&mut s0[..], &mut s1[..], &mut s2[..]],
                &self.mixes,
                n,
            );
            for ch in 0..3 {
                let out = b.audio.output_mut(ch).unwrap();
                out[..n].copy_from_slice(&self.first[ch][..n]);
                S::add_in_place(self.ops, &self.second[ch], out, n);
            }
        } else {
            for ch in 0..3 {
                S::gain(self.ops, &self.first[ch], self.gains[ch], &mut self.ga, n);
                S::gain(self.ops, &self.second[ch], self.mixes[ch], &mut self.gb, n);
                S::add(
                    self.ops,
                    &self.ga,
                    &self.gb,
                    b.audio.output_mut(ch).unwrap(),
                    n,
                );
            }
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

/// `tolerance == None` selects the mixer-only graph with exact comparison.
fn run<S: Native>(planar: bool, tolerance: Option<f64>) {
    let layout = AudioLayout::new(3, 3).unwrap();
    let config = ProcessConfig::new(48000., MAXIMUM, 0, layout).unwrap();
    let gains = [0.37, -0.81, 1.2].map(S::narrow);
    let mixes = [-0.31, 0.2, 0.07].map(S::narrow);
    let input: [Vec<S>; 3] = std::array::from_fn(|ch| {
        (0..FRAMES)
            .map(|n| {
                S::narrow(0.4 * (n as f64 * (0.037 + 0.11 * ch as f64) + 0.7 * ch as f64).sin())
            })
            .collect()
    });
    // Independent schedule: f32 products of two f32 values are exact in f64,
    // and one f64 rounding of an f32 sum followed by narrowing is innocuous,
    // so this equals correctly rounded sample-precision arithmetic.
    let mixed: [Vec<S>; 3] = std::array::from_fn(|ch| {
        (0..FRAMES)
            .map(|n| {
                let a = S::narrow(input[ch][n].widen() * gains[ch].widen());
                let b = S::narrow(input[(ch + 1) % 3][n].widen() * mixes[ch].widen());
                S::narrow(a.widen() + b.widen())
            })
            .collect()
    });
    let expected: [Vec<S>; 3] = if tolerance.is_none() {
        mixed.clone()
    } else {
        std::array::from_fn(|ch| {
            (0usize..FRAMES)
                .map(|n| {
                    S::narrow(
                        TAPS.iter()
                            .enumerate()
                            .filter_map(|(j, h)| n.checked_sub(j).map(|i| h * mixed[ch][i].widen()))
                            .sum(),
                    )
                })
                .collect()
        })
    };
    let fir_stage = tolerance.is_some();
    for backend in [Backend::Scalar, Backend::native().unwrap()] {
        for inplace in [false, true] {
            let (sender, receiver) = mpsc::channel();
            let mixer = Mixer::<S> {
                ops: S::ops(backend),
                planar,
                gains,
                mixes,
                first: std::array::from_fn(|_| vec![S::default(); MAXIMUM]),
                second: std::array::from_fn(|_| vec![S::default(); MAXIMUM]),
                ga: vec![S::default(); MAXIMUM],
                gb: vec![S::default(); MAXIMUM],
                config: None,
                committed: 0,
                returned: sender,
            };
            let mut nodes = vec![GraphNode {
                id: NodeId(0),
                layout,
                processor: Box::new(mixer) as Box<dyn Processor<S> + Send>,
            }];
            let last = if fir_stage {
                nodes.push(GraphNode {
                    id: NodeId(1),
                    layout,
                    processor: Box::new(Fir::<S> {
                        fir: std::array::from_fn(|_| FirConvolver::new(&TAPS).unwrap()),
                        config: None,
                        marker: core::marker::PhantomData,
                    }),
                });
                NodeId(1)
            } else {
                NodeId(0)
            };
            let routes: Vec<_> = (0..3)
                .flat_map(|channel| {
                    let mut r = vec![Connection {
                        source: Source::Input(channel),
                        destination: Destination::Node {
                            id: NodeId(0),
                            channel,
                        },
                        gain: S::narrow(1.),
                    }];
                    if fir_stage {
                        r.push(Connection {
                            source: Source::Node {
                                id: NodeId(0),
                                channel,
                            },
                            destination: Destination::Node {
                                id: NodeId(1),
                                channel,
                            },
                            gain: S::narrow(1.),
                        });
                    }
                    r.push(Connection {
                        source: Source::Node { id: last, channel },
                        destination: Destination::Output(channel),
                        gain: S::narrow(1.),
                    });
                    r
                })
                .collect();
            let mut graph = Graph::new(layout, nodes, &routes).unwrap();
            for _cycle in 0..2 {
                graph.prepare(config).unwrap();
                let mut output: [Vec<S>; 3] =
                    std::array::from_fn(|_| vec![S::narrow(99.); FRAMES + 3]);
                let info = allocation_counter::measure(|| {
                    let (mut done, mut turn) = (0, 0);
                    while done < FRAMES {
                        let n = [0, 1, 67, 2, 31, 5][turn % 6].min(FRAMES - done);
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
                assert_eq!(
                    (info.count_total, info.count_current, info.bytes_total),
                    (0, 0, 0)
                );
                for ch in 0..3 {
                    for (frame, (a, b)) in
                        output[ch][..FRAMES].iter().zip(&expected[ch]).enumerate()
                    {
                        match tolerance {
                            // Numerical equality: exact up to the sign of zero.
                            None => assert!(
                                a.widen() == b.widen(),
                                "{backend:?} planar={planar} inplace={inplace} ch={ch} frame={frame}: {} vs {}",
                                a.widen(),
                                b.widen()
                            ),
                            Some(t) => assert!(
                                (a.widen() - b.widen()).abs() < t,
                                "{backend:?} planar={planar}: {} vs {}",
                                a.widen(),
                                b.widen()
                            ),
                        }
                    }
                    assert!(output[ch][FRAMES..].iter().all(|x| x.widen() == 99.));
                }
                graph.deactivate();
                assert_eq!(receiver.try_recv().unwrap(), FRAMES);
            }
        }
    }
}
#[test]
fn graph32_mixer_stage_exact_against_rounded_schedule() {
    run::<f32>(false, None);
    run::<f32>(true, None);
}
#[test]
fn graph64_mixer_stage_exact_against_rounded_schedule() {
    run::<f64>(false, None);
    run::<f64>(true, None);
}
#[test]
fn graph32_planar_bank_mixer_to_fir() {
    run::<f32>(true, Some(1e-7));
}
#[test]
fn graph64_planar_bank_mixer_to_fir() {
    run::<f64>(true, Some(3e-15));
}
