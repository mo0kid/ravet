#![cfg(feature = "processor")]
use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Sample, Transport,
};
use ravet_graph::{
    Connection, Destination, Graph, GraphNode, GraphSample, GraphStatus, NodeId, Source,
};
use ravet_native_dsp::{DenormalMode, DenormalProcessor, FloatState, current_float_state};
use ravet_signal::{FirConvolver, FirstOrderTptDesign, FirstOrderTptFilter, FirstOrderTptFilter32};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Sender},
};
const TAPS: [f64; 4] = [0.2, 0.4, -0.1, 0.5];
trait Native: Sample + GraphSample + Send + 'static {
    type Filter: Send;
    fn widen(self) -> f64;
    fn narrow(x: f64) -> Self;
    fn create(d: FirstOrderTptDesign) -> Self::Filter;
    fn filter(f: &mut Self::Filter, x: Self) -> Self;
    fn reset_filter(f: &mut Self::Filter);
    fn gain(d: FirstOrderTptDesign) -> f64;
}
impl Native for f64 {
    type Filter = FirstOrderTptFilter;
    fn widen(self) -> f64 {
        self
    }
    fn narrow(x: f64) -> Self {
        x
    }
    fn create(d: FirstOrderTptDesign) -> Self::Filter {
        FirstOrderTptFilter::new(d)
    }
    fn filter(f: &mut Self::Filter, x: Self) -> Self {
        f.process_sample(x).low_pass
    }
    fn reset_filter(f: &mut Self::Filter) {
        f.reset();
    }
    fn gain(d: FirstOrderTptDesign) -> f64 {
        d.gain()
    }
}
impl Native for f32 {
    type Filter = FirstOrderTptFilter32;
    fn widen(self) -> f64 {
        self as f64
    }
    fn narrow(x: f64) -> Self {
        x as f32
    }
    fn create(d: FirstOrderTptDesign) -> Self::Filter {
        FirstOrderTptFilter32::new(d.quantize_f32().unwrap())
    }
    fn filter(f: &mut Self::Filter, x: Self) -> Self {
        f.process_sample(x).low_pass
    }
    fn reset_filter(f: &mut Self::Filter) {
        f.reset();
    }
    fn gain(d: FirstOrderTptDesign) -> f64 {
        d.quantize_f32().unwrap().gain() as f64
    }
}
struct Dsp<S: Native> {
    filter: [S::Filter; 2],
    fir: [FirConvolver; 2],
    config: Option<ProcessConfig>,
    committed: usize,
    starts: usize,
    stops: usize,
    deactivations: usize,
    start_count: Arc<AtomicUsize>,
    stop_count: Arc<AtomicUsize>,
    returned: Sender<(usize, usize, usize, usize)>,
}
impl<S: Native> Processor<S> for Dsp<S> {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        if config.layout() != AudioLayout::new(2, 2).unwrap() {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.config = Some(config);
        self.reset();
        Ok(())
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        self.starts += 1;
        self.start_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn stop_audio(&mut self) {
        self.stops += 1;
        self.stop_count.fetch_add(1, Ordering::Relaxed);
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        if let Err(e) = self.config.unwrap().validate_block(b) {
            b.clear_outputs();
            return ProcessStatus::Fault(e);
        }
        let observed = current_float_state().unwrap();
        assert!(observed.input_flush() && observed.result_flush());
        for ch in 0..2 {
            for n in 0..b.audio.frames() {
                let x = b.audio.input(ch).unwrap()[n];
                let y = S::filter(&mut self.filter[ch], x);
                let out = S::narrow(self.fir[ch].process_sample(y.widen()));
                b.audio.output_mut(ch).unwrap()[n] = out;
            }
        }
        self.committed += b.audio.frames();
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        for f in &mut self.filter {
            S::reset_filter(f);
        }
        for f in &mut self.fir {
            f.reset();
        }
        self.committed = 0;
    }
    fn deactivate(&mut self) {
        self.config = None;
        self.deactivations += 1;
        self.returned
            .send((self.committed, self.starts, self.stops, self.deactivations))
            .unwrap();
    }
}
// Independent scalar difference equation of the discrete transfer G(1+z^-1)/
// (1+(2G-1)z^-1), followed by explicit finite convolution. Actual native32
// filter recurrence rounds differently; the declared observation tolerance covers it.
fn reference<S: Native>(input: &[S], gain: f64) -> Vec<S> {
    let (mut previous_x, mut previous_y) = (0., 0.);
    let low: Vec<_> = input
        .iter()
        .map(|x| {
            let y = gain * x.widen() + gain * previous_x - (2. * gain - 1.) * previous_y;
            previous_x = x.widen();
            previous_y = y;
            S::narrow(y)
        })
        .collect();
    (0..input.len())
        .map(|n| {
            S::narrow(
                TAPS.iter()
                    .enumerate()
                    .filter_map(|(j, h)| n.checked_sub(j).map(|i| h * low[i].widen()))
                    .sum(),
            )
        })
        .collect()
}
fn run<S: Native>(tolerance: f64) {
    for inplace in [false, true] {
        let design = FirstOrderTptDesign::new(48000., 1700.).unwrap();
        let (sender, receiver) = mpsc::channel();
        let start_count = Arc::new(AtomicUsize::new(0));
        let stop_count = Arc::new(AtomicUsize::new(0));
        let node = Dsp::<S> {
            filter: std::array::from_fn(|_| S::create(design)),
            fir: std::array::from_fn(|_| FirConvolver::new(&TAPS).unwrap()),
            config: None,
            committed: 0,
            starts: 0,
            stops: 0,
            deactivations: 0,
            start_count: start_count.clone(),
            stop_count: stop_count.clone(),
            returned: sender,
        };
        let prior: FloatState = current_float_state().unwrap();
        let adapter = DenormalProcessor::new(node, DenormalMode::FlushBoth)
            .ok()
            .unwrap();
        assert_eq!(current_float_state().unwrap(), prior);
        let layout = AudioLayout::new(2, 2).unwrap();
        let routes: Vec<_> = (0..2)
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
                        destination: Destination::Output(channel),
                        gain: S::narrow(1.),
                    },
                ]
            })
            .collect();
        let mut graph = Graph::new(
            layout,
            vec![GraphNode {
                id: NodeId(0),
                layout,
                processor: Box::new(adapter),
            }],
            &routes,
        )
        .unwrap();
        let config = ProcessConfig::new(48000., 67, 0, layout).unwrap();
        let input: [Vec<S>; 2] = std::array::from_fn(|ch| {
            (0..1703)
                .map(|n| {
                    S::narrow(0.3 * (n as f64 * (0.19 + ch as f64 * 0.013) + ch as f64 * 0.7).sin())
                })
                .collect()
        });
        let expected: [Vec<S>; 2] = input.each_ref().map(|x| reference(x, S::gain(design)));
        for cycle in 0..2 {
            let prior = current_float_state().unwrap();
            graph.prepare(config).unwrap();
            assert_eq!(current_float_state().unwrap(), prior);
            let mut output: [Vec<S>; 2] = std::array::from_fn(|_| vec![S::narrow(99.); 1706]);
            // Real Graph session guard owns manual start/stop around callbacks.
            let mut audio_owner = graph.start_audio().unwrap();
            assert_eq!(start_count.load(Ordering::Relaxed), cycle + 1);
            let info = allocation_counter::measure(|| {
                let (mut done, mut turn) = (0, 0);
                while done < 1703 {
                    let n = [0, 1, 67, 2, 31, 5][turn % 6].min(1703 - done);
                    turn += 1;
                    let inputs = input.each_ref().map(|x| &x[done..]);
                    let [a, b] = &mut output;
                    let mut outputs = [&mut a[done..], &mut b[done..]];
                    let audio = if inplace {
                        for ch in 0..2 {
                            outputs[ch][..n].copy_from_slice(&inputs[ch][..n]);
                        }
                        AudioBuffer::in_place(&mut outputs, n).unwrap()
                    } else {
                        AudioBuffer::separate(&inputs, &mut outputs, n).unwrap()
                    };
                    assert_eq!(
                        audio_owner.process(&mut ProcessBlock::new(
                            audio,
                            &[],
                            Transport::default()
                        )),
                        GraphStatus::Continue
                    );
                    assert_eq!(current_float_state().unwrap(), prior);
                    done += n;
                }
            });
            assert_eq!(info.count_total, 0);
            assert_eq!(info.count_current, 0);
            assert_eq!(info.bytes_total, 0);
            drop(audio_owner);
            assert_eq!(stop_count.load(Ordering::Relaxed), cycle + 1);
            for ch in 0..2 {
                for (a, b) in output[ch][..1703].iter().zip(&expected[ch]) {
                    assert!(
                        (a.widen() - b.widen()).abs() < tolerance,
                        "{} vs {}",
                        a.widen(),
                        b.widen()
                    );
                }
                assert!(output[ch][1703..].iter().all(|x| x.widen() == 99.));
            }
            graph.deactivate();
            let counts = receiver.try_recv().unwrap();
            assert_eq!(counts, (1703, cycle + 1, cycle + 1, cycle + 1));
            assert_eq!(current_float_state().unwrap(), prior);
        }
        println!(
            "Graph native{} external_inplace={inplace}: Foundation → scoped native policy → actual TPT IIR/FIR; two manual session cycles, independent difference equation/convolution, exact full state restoration, heap0",
            size_of::<S>() * 8
        );
    }
}
#[test]
fn actual_native64_graph_iir_fir_and_manual_sessions() {
    run::<f64>(2e-14);
}
#[test]
fn actual_native32_graph_iir_fir_and_manual_sessions() {
    run::<f32>(4e-7);
}
