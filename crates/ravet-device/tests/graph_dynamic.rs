use ravet_audio_file::audio_ring;
use ravet_device::{GraphOutputError, PreparedDynamicGraphOutput};
use ravet_foundation::{
    AudioLayout, Event, EventData, ParameterId, PrepareError, ProcessBlock, ProcessConfig,
    ProcessError, ProcessStatus, Processor, Transport,
};
use ravet_graph::{
    Connection, Destination, DynamicGraph, Graph, GraphNode, GraphStatus, GraphSwapStatus as Swap,
    NodeId, Source,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc::sync_channel,
};
struct Observe {
    main: std::thread::ThreadId,
    drops: AtomicUsize,
    calls: AtomicUsize,
    starts: AtomicUsize,
    stops: AtomicUsize,
}
struct Constant {
    observe: Arc<Observe>,
    owner: Option<std::thread::ThreadId>,
    value: f32,
    fail: bool,
}
impl Drop for Constant {
    fn drop(&mut self) {
        assert_eq!(std::thread::current().id(), self.observe.main);
        self.observe.drops.fetch_add(1, Ordering::Relaxed);
    }
}
impl Processor for Constant {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        assert_eq!(std::thread::current().id(), self.observe.main);
        Ok(())
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        if self.fail {
            return Err(ProcessError::BackendFailure);
        }
        self.owner = Some(std::thread::current().id());
        self.observe.starts.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn stop_audio(&mut self) {
        assert_eq!(self.owner.take(), Some(std::thread::current().id()));
        self.observe.stops.fetch_add(1, Ordering::Relaxed);
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        assert_eq!(self.owner, Some(std::thread::current().id()));
        self.observe.calls.fetch_add(1, Ordering::Relaxed);
        block.audio.output_mut(0).unwrap().fill(self.value);
        if let Some(output) = block.output_events.as_mut()
            && output.capacity() != 0
        {
            output
                .push(Event {
                    frame_offset: block.audio.frames() - 1,
                    data: EventData::ParameterValue {
                        id: ParameterId(1),
                        value: self.value,
                    },
                })
                .unwrap();
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        assert_eq!(std::thread::current().id(), self.observe.main);
    }
}
fn observe() -> Arc<Observe> {
    Arc::new(Observe {
        main: std::thread::current().id(),
        drops: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
        starts: AtomicUsize::new(0),
        stops: AtomicUsize::new(0),
    })
}
fn configuration() -> ProcessConfig {
    ProcessConfig::new(48000., 8, 1, AudioLayout::MONO_INSTRUMENT).unwrap()
}
fn graph(value: f32, fail: bool, observe: &Arc<Observe>) -> Box<Graph> {
    graph_config(value, fail, observe, configuration())
}
fn graph_config(
    value: f32,
    fail: bool,
    observe: &Arc<Observe>,
    configuration: ProcessConfig,
) -> Box<Graph> {
    let mut graph = Graph::new(
        AudioLayout::MONO_INSTRUMENT,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::MONO_INSTRUMENT,
            processor: Box::new(Constant {
                observe: observe.clone(),
                owner: None,
                value,
                fail,
            }),
        }],
        &[Connection {
            source: Source::Node {
                id: NodeId(1),
                channel: 0,
            },
            destination: Destination::Output(0),
            gain: 1.,
        }],
    )
    .unwrap();
    if configuration.maximum_output_events() != 0 {
        graph
            .set_event_routes(&[ravet_graph::EventConnection {
                source: ravet_graph::EventEndpoint::Node(NodeId(1)),
                destination: ravet_graph::EventEndpoint::Host,
                filter: ravet_graph::EventFilter::All,
            }])
            .unwrap();
    }
    graph.prepare(configuration).unwrap();
    Box::new(graph)
}
#[test]
fn live_offers_wait_for_old_output_suffix_and_retirement_reclamation_stays_on_main() {
    let observe = observe();
    let (mut control, engine) = DynamicGraph::new(graph(1., false, &observe), 1).unwrap();
    let second = graph(2., false, &observe);
    let third = graph(3., false, &observe);
    let (producer, mut consumer) = audio_ring(1, 4).unwrap();
    let mut prepared =
        PreparedDynamicGraphOutput::new(Box::new(engine), producer, configuration()).unwrap();
    let (ready_tx, ready_rx) = sync_channel(0);
    let (go_tx, go_rx) = sync_channel(0);
    let worker = std::thread::spawn(move || {
        let mut session = prepared.start_audio().unwrap();
        let mut samples = [99.; 4];
        let first = session.pump(Transport::default()).unwrap();
        assert_eq!(first.swap, Some(Swap::Unchanged));
        consumer.pop(&mut samples).unwrap();
        assert_eq!(samples, [1.; 4]);
        ready_tx.send(()).unwrap();
        go_rx.recv().unwrap();
        let pending = session.pump(Transport::default()).unwrap();
        assert_eq!(pending.swap, None);
        consumer.pop(&mut samples).unwrap();
        assert_eq!(samples, [1.; 4]);
        let allocations = allocation_counter::measure(|| {
            let fresh = session.pump(Transport::default()).unwrap();
            assert_eq!(fresh.swap, Some(Swap::Applied));
        });
        assert_eq!(allocations.count_total, 0);
        consumer.pop(&mut samples).unwrap();
        assert_eq!(samples, [2.; 4]);
        session.pump(Transport::default()).unwrap();
        consumer.pop(&mut samples).unwrap();
        assert_eq!(samples, [2.; 4]);
        ready_tx.send(()).unwrap();
        go_rx.recv().unwrap();
        let full = session.pump(Transport::default()).unwrap();
        assert_eq!(full.swap, Some(Swap::RetirementFull));
        consumer.pop(&mut samples).unwrap();
        assert_eq!(samples, [2.; 4]);
        session.pump(Transport::default()).unwrap();
        consumer.pop(&mut samples).unwrap();
        ready_tx.send(()).unwrap();
        go_rx.recv().unwrap();
        let allocations = allocation_counter::measure(|| {
            let applied = session.pump(Transport::default()).unwrap();
            assert_eq!(applied.swap, Some(Swap::Applied));
            consumer.pop(&mut samples).unwrap();
            assert_eq!(samples, [3.; 4]);
            session.pump(Transport::default()).unwrap();
            consumer.pop(&mut samples).unwrap();
            for _ in 0..1000 {
                let fresh = session.pump(Transport::default()).unwrap();
                assert_eq!(fresh.swap, Some(Swap::Unchanged));
                consumer.pop(&mut samples).unwrap();
                assert_eq!(samples, [3.; 4]);
                let pending = session.pump(Transport::default()).unwrap();
                assert_eq!(pending.swap, None);
                consumer.pop(&mut samples).unwrap();
                assert_eq!(samples, [3.; 4]);
            }
        });
        assert_eq!(allocations.count_total, 0);
        assert_eq!(allocations.count_current, 0);
        session.finish();
        prepared
    });
    ready_rx.recv().unwrap();
    control.offer(second).unwrap();
    go_tx.send(()).unwrap();
    ready_rx.recv().unwrap();
    control.offer(third).unwrap();
    go_tx.send(()).unwrap();
    ready_rx.recv().unwrap();
    assert_eq!(control.reclaim(), 1);
    assert_eq!(observe.drops.load(Ordering::Relaxed), 1);
    go_tx.send(()).unwrap();
    let prepared = worker.join().unwrap();
    assert_eq!(control.reclaim(), 1);
    drop(prepared.into_graph());
    drop(control);
    assert_eq!(observe.drops.load(Ordering::Relaxed), 3);
    assert_eq!(observe.starts.load(Ordering::Relaxed), 3);
    assert_eq!(observe.stops.load(Ordering::Relaxed), 3);
}
#[test]
fn invalid_outer_input_keeps_offer_and_failed_start_preserves_old_processing() {
    for fail in [false, true] {
        let observe = observe();
        let (mut control, engine) = DynamicGraph::new(graph(1., false, &observe), 1).unwrap();
        control.offer(graph(2., fail, &observe)).unwrap();
        let (producer, mut consumer) = audio_ring(1, 8).unwrap();
        let mut prepared =
            PreparedDynamicGraphOutput::new(Box::new(engine), producer, configuration()).unwrap();
        let prepared = std::thread::spawn(move || {
            let mut session = prepared.start_audio().unwrap();
            let invalid = [Event {
                frame_offset: 8,
                data: EventData::ParameterValue {
                    id: ParameterId(1),
                    value: 1.,
                },
            }];
            assert!(matches!(
                session.pump_events(&invalid, Transport::default()),
                Err(GraphStatus::Fault {
                    node: None,
                    error: ProcessError::Events(_)
                })
            ));
            let valid = session.pump(Transport::default()).unwrap();
            assert_eq!(
                valid.swap,
                Some(if fail {
                    Swap::StartRejected(GraphStatus::Fault {
                        node: Some(NodeId(1)),
                        error: ProcessError::BackendFailure,
                    })
                } else {
                    Swap::Applied
                })
            );
            let mut samples = [99.; 8];
            consumer.pop(&mut samples).unwrap();
            assert_eq!(samples, [if fail { 1. } else { 2. }; 8]);
            session.finish();
            prepared
        })
        .join()
        .unwrap();
        assert_eq!(control.reclaim(), 1);
        drop(prepared.into_graph());
        drop(control);
        assert_eq!(observe.drops.load(Ordering::Relaxed), 2);
        assert_eq!(observe.calls.load(Ordering::Relaxed), 1);
    }
}
#[test]
fn dynamic_worker_requires_exact_already_prepared_configuration() {
    let observe = observe();
    let (control, engine) = DynamicGraph::new(graph(1., false, &observe), 1).unwrap();
    assert_eq!(engine.configuration(), configuration());
    let (producer, _consumer) = audio_ring(1, 8).unwrap();
    assert!(matches!(
        PreparedDynamicGraphOutput::new(
            Box::new(engine),
            producer,
            ProcessConfig::new(48000., 16, 1, AudioLayout::MONO_INSTRUMENT).unwrap()
        ),
        Err(GraphOutputError::Configuration)
    ));
    drop(control);
}

#[test]
fn queued_dynamic_input_is_consumed_once_and_swaps_only_on_fresh_render() {
    let configuration = ProcessConfig::new(48000., 8, 0, AudioLayout::MONO).unwrap();
    let graph = |gain| {
        let mut graph = Graph::new(
            AudioLayout::MONO,
            vec![],
            &[Connection {
                source: Source::Input(0),
                destination: Destination::Output(0),
                gain,
            }],
        )
        .unwrap();
        graph.prepare(configuration).unwrap();
        Box::new(graph)
    };
    let (mut control, engine) = DynamicGraph::new(graph(2.), 1).unwrap();
    control.offer(graph(3.)).unwrap();
    let (mut source, input) = audio_ring(1, 8).unwrap();
    source.push(&[1.; 8]).unwrap();
    let (output, mut sink) = audio_ring(1, 4).unwrap();
    let mut prepared =
        PreparedDynamicGraphOutput::with_input(Box::new(engine), input, output, configuration)
            .unwrap();
    let prepared = std::thread::spawn(move || {
        let mut session = prepared.start_audio().unwrap();
        let fresh = session.pump(Transport::default()).unwrap();
        assert_eq!(fresh.swap, Some(Swap::Applied));
        assert_eq!(fresh.input_frames_consumed, 8);
        let mut samples = [99.; 4];
        sink.pop(&mut samples).unwrap();
        assert_eq!(samples, [3.; 4]);
        let pending = session.pump(Transport::default()).unwrap();
        assert_eq!(pending.swap, None);
        assert_eq!(pending.input_frames_consumed, 0);
        sink.pop(&mut samples).unwrap();
        assert_eq!(samples, [3.; 4]);
        let shortage = session.pump(Transport::default()).unwrap();
        assert_eq!(shortage.swap, Some(Swap::Unchanged));
        assert_eq!(
            (
                shortage.input_frames_consumed,
                shortage.input_underflow_frames
            ),
            (0, 8)
        );
        sink.pop(&mut samples).unwrap();
        assert_eq!(samples, [0.; 4]);
        session.pump(Transport::default()).unwrap();
        sink.pop(&mut samples).unwrap();
        assert_eq!(samples, [0.; 4]);
        session.finish();
        prepared
    })
    .join()
    .unwrap();
    assert_eq!(control.reclaim(), 1);
    drop(prepared.into_graph());
    drop(control);
}

#[test]
fn dynamic_generated_prefix_belongs_to_new_graph_and_is_not_repeated_with_audio_suffix() {
    let observe = observe();
    let configuration = configuration().with_output_events(1);
    let (mut control, engine) =
        DynamicGraph::new(graph_config(1., false, &observe, configuration), 1).unwrap();
    control
        .offer(graph_config(2., false, &observe, configuration))
        .unwrap();
    let (producer, mut consumer) = audio_ring(1, 4).unwrap();
    let mut prepared =
        PreparedDynamicGraphOutput::new(Box::new(engine), producer, configuration).unwrap();
    let prepared = std::thread::spawn(move || {
        let mut session = prepared.start_audio().unwrap();
        let mut storage = [Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: ParameterId(1),
                value: 99.,
            },
        }];
        let fresh = session
            .pump_io(&[], Transport::default(), &mut storage)
            .unwrap();
        assert_eq!(fresh.swap, Some(Swap::Applied));
        assert_eq!(fresh.output_event_count, 1);
        assert_eq!(storage[0].frame_offset, 7);
        assert_eq!(
            storage[0].data,
            EventData::ParameterValue {
                id: ParameterId(1),
                value: 2.
            }
        );
        let expected = storage;
        let mut samples = [99.; 4];
        consumer.pop(&mut samples).unwrap();
        assert_eq!(samples, [2.; 4]);
        let pending = session
            .pump_io(&[], Transport::default(), &mut storage)
            .unwrap();
        assert_eq!(pending.swap, None);
        assert_eq!(pending.output_event_count, 0);
        assert_eq!(storage, expected);
        consumer.pop(&mut samples).unwrap();
        assert_eq!(samples, [2.; 4]);
        session.finish();
        prepared
    })
    .join()
    .unwrap();
    assert_eq!(control.reclaim(), 1);
    drop(prepared.into_graph());
    drop(control);
    assert_eq!(observe.calls.load(Ordering::Relaxed), 1);
}

#[test]
fn swap_followed_by_processing_fault_remains_observable_after_error() {
    struct Fault;
    impl Processor for Fault {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
            Ok(())
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {}
        fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
            ProcessStatus::Fault(ProcessError::BackendFailure)
        }
    }
    let observe = observe();
    let (mut control, engine) = DynamicGraph::new(graph(1., false, &observe), 1).unwrap();
    let mut failed = Graph::new(
        AudioLayout::MONO_INSTRUMENT,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::MONO_INSTRUMENT,
            processor: Box::new(Fault),
        }],
        &[Connection {
            source: Source::Node {
                id: NodeId(1),
                channel: 0,
            },
            destination: Destination::Output(0),
            gain: 1.,
        }],
    )
    .unwrap();
    failed.prepare(configuration()).unwrap();
    control.offer(Box::new(failed)).unwrap();
    let (producer, mut consumer) = audio_ring(1, 8).unwrap();
    let mut prepared =
        PreparedDynamicGraphOutput::new(Box::new(engine), producer, configuration()).unwrap();
    let prepared = std::thread::spawn(move || {
        let mut session = prepared.start_audio().unwrap();
        assert_eq!(session.last_swap(), None);
        let failure = session.pump(Transport::default()).unwrap_err();
        assert_eq!(session.last_swap(), Some(Swap::Applied));
        assert_eq!(session.pump(Transport::default()), Err(failure));
        assert_eq!(session.last_swap(), Some(Swap::Applied));
        let mut samples = [99.; 8];
        assert_eq!(consumer.pop(&mut samples).unwrap(), 0);
        assert_eq!(samples, [99.; 8]);
        session.finish();
        prepared
    })
    .join()
    .unwrap();
    assert_eq!(control.reclaim(), 1);
    drop(prepared.into_graph());
    drop(control);
}
