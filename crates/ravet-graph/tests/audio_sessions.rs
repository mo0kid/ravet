use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Transport,
};
use ravet_graph::{
    Connection, Destination, DynamicGraph, DynamicGraphAudioSession, Graph, GraphNode, GraphStatus,
    GraphSwapStatus as Swap, NodeId, Source,
};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU8, AtomicUsize, Ordering},
    mpsc,
};
use std::thread::ThreadId;
struct Observe {
    main: ThreadId,
    audio: OnceLock<ThreadId>,
    wrong: AtomicUsize,
    events: [AtomicUsize; 64],
    count: AtomicUsize,
    mode: AtomicU8,
    deactivations: AtomicUsize,
    drops: AtomicUsize,
}
impl Observe {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            main: std::thread::current().id(),
            audio: OnceLock::new(),
            wrong: AtomicUsize::new(0),
            events: [const { AtomicUsize::new(0) }; 64],
            count: AtomicUsize::new(0),
            mode: AtomicU8::new(0),
            deactivations: AtomicUsize::new(0),
            drops: AtomicUsize::new(0),
        })
    }
    fn check(&self, audio: bool) {
        let expected = if audio {
            *self.audio.get().unwrap()
        } else {
            self.main
        };
        if expected != std::thread::current().id() {
            self.wrong.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn event(&self, tag: usize, stop: bool) {
        let index = self.count.fetch_add(1, Ordering::SeqCst);
        self.events[index].store(tag * 2 + usize::from(stop), Ordering::SeqCst);
    }
    fn trace(&self) -> Vec<usize> {
        self.events[..self.count.load(Ordering::SeqCst)]
            .iter()
            .map(|x| x.load(Ordering::SeqCst))
            .collect()
    }
}
struct Node {
    observe: Arc<Observe>,
    tag: usize,
    value: f32,
    fail: bool,
    config: Option<ProcessConfig>,
    started: bool,
}
impl Processor for Node {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.observe.check(false);
        self.config = Some(config);
        Ok(())
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        self.observe.check(true);
        assert!(!self.started);
        self.observe.event(self.tag, false);
        if self.fail {
            match self.observe.mode.load(Ordering::SeqCst) {
                1 => return Err(ProcessError::NotPrepared),
                2 => panic!("synthetic audio start panic"),
                _ => {}
            }
        }
        self.started = true;
        Ok(())
    }
    fn stop_audio(&mut self) {
        self.observe.check(true);
        assert!(self.started);
        self.observe.event(self.tag, true);
        self.started = false;
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.observe.check(true);
        assert!(self.started);
        self.config.unwrap().validate_block(block).unwrap();
        for frame in 0..block.audio.frames() {
            let input = if block.audio.input_channels() == 0 {
                1.
            } else {
                block.audio.input(0).unwrap()[frame]
            };
            block.audio.output_mut(0).unwrap()[frame] = input * self.value;
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.observe.check(true);
    }
    fn deactivate(&mut self) {
        self.observe.check(false);
        assert!(!self.started);
        if self.config.take().is_some() {
            self.observe.deactivations.fetch_add(1, Ordering::SeqCst);
        }
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        self.observe.check(false);
        assert!(!self.started);
        self.observe.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn prepared(observe: &Arc<Observe>, tag: usize, value: f32, fail: bool, pair: bool) -> Box<Graph> {
    let mut nodes = vec![GraphNode {
        id: NodeId(1),
        layout: AudioLayout::MONO_INSTRUMENT,
        processor: Box::new(Node {
            observe: observe.clone(),
            tag,
            value,
            fail: fail && !pair,
            config: None,
            started: false,
        }),
    }];
    let mut routes = vec![];
    let last = if pair {
        nodes.push(GraphNode {
            id: NodeId(2),
            layout: AudioLayout::MONO,
            processor: Box::new(Node {
                observe: observe.clone(),
                tag: tag + 1,
                value: 2.,
                fail,
                config: None,
                started: false,
            }),
        });
        routes.push(Connection {
            source: Source::Node {
                id: NodeId(1),
                channel: 0,
            },
            destination: Destination::Node {
                id: NodeId(2),
                channel: 0,
            },
            gain: 1.,
        });
        NodeId(2)
    } else {
        NodeId(1)
    };
    routes.push(Connection {
        source: Source::Node {
            id: last,
            channel: 0,
        },
        destination: Destination::Output(0),
        gain: 1.,
    });
    let mut graph = Box::new(Graph::new(AudioLayout::MONO_INSTRUMENT, nodes, &routes).unwrap());
    graph
        .prepare(ProcessConfig::new(48000., 8, 0, AudioLayout::MONO_INSTRUMENT).unwrap())
        .unwrap();
    graph
}
fn render(session: &mut DynamicGraphAudioSession<'_>, invalid: bool) -> (Swap, [f32; 8]) {
    let mut samples = [99.; 8];
    let mut extra = [99.; 8];
    let mut outputs = [samples.as_mut_slice(), extra.as_mut_slice()];
    let audio = AudioBuffer::separate(&[], &mut outputs[..if invalid { 2 } else { 1 }], 8).unwrap();
    let mut status = None;
    let allocation = allocation_counter::measure(|| {
        status = Some(session.process(&mut ProcessBlock::new(audio, &[], Transport::default())));
    });
    assert_eq!(allocation.count_total, 0);
    assert_eq!(allocation.count_current, 0);
    let status = status.unwrap();
    if !invalid {
        assert_eq!(status.processing, GraphStatus::Continue);
    }
    (status.swap, samples)
}
#[test]
fn static_session_rolls_back_error_and_panic_then_stops_in_reverse_owner_order() {
    let observe = Observe::new();
    let mut graph = prepared(&observe, 1, 3., true, true);
    let worker_observe = observe.clone();
    let graph = std::thread::spawn(move || {
        worker_observe
            .audio
            .set(std::thread::current().id())
            .unwrap();
        worker_observe.mode.store(1, Ordering::SeqCst);
        assert!(matches!(
            graph.start_audio(),
            Err(GraphStatus::Fault {
                node: Some(NodeId(2)),
                error: ProcessError::NotPrepared
            })
        ));
        assert!(graph.start_audio().is_err());
        graph.reset();
        worker_observe.mode.store(2, Ordering::SeqCst);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _session = graph.start_audio();
            }))
            .is_err()
        );
        worker_observe.mode.store(0, Ordering::SeqCst);
        graph.reset();
        {
            let mut session = graph.start_audio().unwrap();
            let mut samples = [0.; 8];
            let mut outputs = [samples.as_mut_slice()];
            let audio = AudioBuffer::separate(&[], &mut outputs, 8).unwrap();
            let allocation = allocation_counter::measure(|| {
                assert_eq!(
                    session.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                    GraphStatus::Continue
                );
            });
            assert_eq!(allocation.count_total, 0);
            assert_eq!(samples, [6.; 8]);
            session.reset();
            session.finish();
        }
        graph
    })
    .join()
    .unwrap();
    assert_eq!(observe.trace(), [2, 4, 3, 2, 4, 3, 2, 4, 5, 3]);
    assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
    drop(graph);
    assert_eq!(observe.deactivations.load(Ordering::SeqCst), 2);
    assert_eq!(observe.drops.load(Ordering::SeqCst), 2);
}
#[test]
fn dynamic_session_retains_old_on_start_error_and_stops_before_control_retirement() {
    let observe = Observe::new();
    observe.mode.store(1, Ordering::SeqCst);
    let (mut control, mut engine) =
        DynamicGraph::new(prepared(&observe, 1, 1., false, false), 1).unwrap();
    control
        .offer(prepared(&observe, 2, 2., true, false))
        .unwrap();
    let worker_observe = observe.clone();
    let (command, receive) = mpsc::sync_channel(0);
    let (results, wait) = mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        worker_observe
            .audio
            .set(std::thread::current().id())
            .unwrap();
        {
            let mut session = engine.start_audio().unwrap();
            assert_eq!(render(&mut session, true), (Swap::InvalidBlock, [0.; 8]));
            results.send(render(&mut session, false)).unwrap();
            receive.recv().unwrap();
            results.send(render(&mut session, false)).unwrap();
            receive.recv().unwrap();
            results.send(render(&mut session, false)).unwrap();
            receive.recv().unwrap();
            session.finish();
        }
        engine
    });
    assert_eq!(
        wait.recv().unwrap(),
        (
            Swap::StartRejected(GraphStatus::Fault {
                node: Some(NodeId(1)),
                error: ProcessError::NotPrepared
            }),
            [1.; 8]
        )
    );
    control
        .offer(prepared(&observe, 3, 3., false, false))
        .unwrap();
    command.send(()).unwrap();
    assert_eq!(wait.recv().unwrap(), (Swap::RetirementFull, [1.; 8]));
    assert_eq!(control.reclaim(), 1);
    command.send(()).unwrap();
    assert_eq!(wait.recv().unwrap(), (Swap::Applied, [3.; 8]));
    assert_eq!(control.reclaim(), 1);
    command.send(()).unwrap();
    let engine = worker.join().unwrap();
    drop(engine);
    drop(control);
    assert_eq!(observe.trace(), [2, 4, 6, 3, 7]);
    assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
    assert_eq!(observe.deactivations.load(Ordering::SeqCst), 3);
    assert_eq!(observe.drops.load(Ordering::SeqCst), 3);
}

#[test]
fn native_f64_guard_retains_default_portable_hooks_and_precision() {
    let mut graph = Graph::<f64>::new(
        AudioLayout::MONO,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::MONO,
            processor: Box::new(ravet_gain::GainProcessor64::default()),
        }],
        &[
            Connection {
                source: Source::Input(0),
                destination: Destination::Node {
                    id: NodeId(1),
                    channel: 0,
                },
                gain: 1.,
            },
            Connection {
                source: Source::Node {
                    id: NodeId(1),
                    channel: 0,
                },
                destination: Destination::Output(0),
                gain: 1.,
            },
        ],
    )
    .unwrap();
    let config = ProcessConfig::new(48000., 2, 0, AudioLayout::MONO).unwrap();
    graph.prepare(config).unwrap();
    let input = [1. + 2_f64.powi(-40), 1e200];
    let mut output = [0.; 2];
    {
        let mut session = graph.start_audio().unwrap();
        let inputs = [input.as_slice()];
        let mut outputs = [output.as_mut_slice()];
        let audio = AudioBuffer::separate(&inputs, &mut outputs, 2).unwrap();
        assert_eq!(
            session.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
            GraphStatus::Continue
        );
    }
    assert_eq!(output, input);
    graph.prepare(config).unwrap();
}
