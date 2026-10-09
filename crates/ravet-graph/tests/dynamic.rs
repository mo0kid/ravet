use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, ParameterId, PrepareError, ProcessBlock,
    ProcessConfig, ProcessStatus, Processor, Transport,
};
use ravet_graph::{
    Connection, Destination, DynamicGraph, Graph, GraphNode, GraphOfferFailure as Failure,
    GraphStatus, GraphSwapStatus as Swap, NodeId, Source,
};
use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
thread_local! { static IN_CALLBACK: Cell<bool> = const { Cell::new(false) }; }
struct Constant {
    value: f32,
    config: Option<ProcessConfig>,
    drops: Arc<AtomicUsize>,
    bad_drops: Arc<AtomicUsize>,
}
impl Drop for Constant {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
        if IN_CALLBACK.get() {
            self.bad_drops.fetch_add(1, Ordering::Relaxed);
        }
    }
}
impl Processor for Constant {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(config);
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.config = None;
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.config.unwrap().validate_block(block).unwrap();
        block.audio.output_mut(0).unwrap().fill(self.value);
        ProcessStatus::Continue
    }
}
fn prepared(value: f32, drops: &Arc<AtomicUsize>, bad_drops: &Arc<AtomicUsize>) -> Box<Graph> {
    let node = GraphNode {
        id: NodeId(1),
        layout: AudioLayout::MONO_INSTRUMENT,
        processor: Box::new(Constant {
            value,
            config: None,
            drops: drops.clone(),
            bad_drops: bad_drops.clone(),
        }),
    };
    let route = Connection {
        source: Source::Node {
            id: NodeId(1),
            channel: 0,
        },
        destination: Destination::Output(0),
        gain: 1.,
    };
    let mut graph =
        Box::new(Graph::new(AudioLayout::MONO_INSTRUMENT, vec![node], &[route]).unwrap());
    graph
        .prepare(ProcessConfig::new(48000., 4, 1, AudioLayout::MONO_INSTRUMENT).unwrap())
        .unwrap();
    graph
}
fn render(engine: &mut DynamicGraph, events: &[Event]) -> (Swap, [f32; 4]) {
    let mut samples = [99.; 4];
    let mut output = [samples.as_mut_slice()];
    let audio = AudioBuffer::separate(&[], &mut output, 4).unwrap();
    IN_CALLBACK.set(true);
    let status = engine.process(&mut ProcessBlock::new(audio, events, Transport::default()));
    IN_CALLBACK.set(false);
    if status.swap != Swap::InvalidBlock {
        assert_eq!(status.processing, GraphStatus::Continue);
    }
    (status.swap, samples)
}
#[test]
fn full_retirement_defers_swap_invalid_block_preserves_pending_and_destruction_stays_on_control() {
    let drops = Arc::new(AtomicUsize::new(0));
    let bad = Arc::new(AtomicUsize::new(0));
    let (mut control, mut engine) = DynamicGraph::new(prepared(1., &drops, &bad), 1).unwrap();
    control.offer(prepared(2., &drops, &bad)).unwrap();
    let rejected = control.offer(prepared(3., &drops, &bad)).unwrap_err();
    assert_eq!(rejected.reason, Failure::Full);
    let invalid = [Event {
        frame_offset: 4,
        data: EventData::ParameterValue {
            id: ParameterId(1),
            value: 1.,
        },
    }];
    assert_eq!(render(&mut engine, &invalid), (Swap::InvalidBlock, [0.; 4]));
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    assert_eq!(render(&mut engine, &[]), (Swap::Applied, [2.; 4]));
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    control.offer(rejected.graph).unwrap();
    assert_eq!(render(&mut engine, &[]), (Swap::RetirementFull, [2.; 4]));
    assert_eq!(control.reclaim(), 1);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert_eq!(render(&mut engine, &[]), (Swap::Applied, [3.; 4]));
    assert_eq!(control.reclaim(), 1);
    assert_eq!(render(&mut engine, &[]), (Swap::Unchanged, [3.; 4]));
    control.offer(prepared(4., &drops, &bad)).unwrap();
    drop(control);
    assert_eq!(render(&mut engine, &[]), (Swap::ControlGone, [3.; 4]));
    drop(engine);
    assert_eq!(drops.load(Ordering::Relaxed), 4);
    assert_eq!(bad.load(Ordering::Relaxed), 0);
}
#[test]
fn graph_reuse_and_ten_thousand_swaps_perform_no_callback_heap_operations() {
    let drops = Arc::new(AtomicUsize::new(0));
    let bad = Arc::new(AtomicUsize::new(0));
    let (mut control, mut engine) = DynamicGraph::new(prepared(1., &drops, &bad), 2).unwrap();
    let mut next = prepared(2., &drops, &bad);
    for index in 0..10000 {
        control.offer(next).unwrap();
        let counts = allocation_counter::measure(|| {
            assert_eq!(
                render(&mut engine, &[]),
                (Swap::Applied, [if index % 2 == 0 { 2. } else { 1. }; 4])
            );
            engine.reset();
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
        next = control.take_retired().unwrap();
    }
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop(next);
    drop(engine);
    let rejected = control.offer(prepared(3., &drops, &bad)).unwrap_err();
    assert_eq!(rejected.reason, Failure::EngineGone);
    drop(rejected);
    drop(control);
    assert_eq!(drops.load(Ordering::Relaxed), 3);
    assert_eq!(bad.load(Ordering::Relaxed), 0);
}
#[test]
fn exact_prepared_configuration_is_required_and_endpoints_move_between_threads() {
    let drops = Arc::new(AtomicUsize::new(0));
    let bad = Arc::new(AtomicUsize::new(0));
    assert_eq!(
        DynamicGraph::new(prepared(1., &drops, &bad), 0)
            .err()
            .unwrap()
            .reason,
        Failure::Capacity
    );
    let (mut control, mut engine) = DynamicGraph::new(prepared(1., &drops, &bad), 1).unwrap();
    let mut wrong = prepared(9., &drops, &bad);
    wrong
        .prepare(ProcessConfig::new(96000., 4, 1, AudioLayout::MONO_INSTRUMENT).unwrap())
        .unwrap();
    assert_eq!(
        control.offer(wrong).unwrap_err().reason,
        Failure::Configuration
    );
    let (tick_tx, tick_rx) = std::sync::mpsc::sync_channel(1);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        while tick_rx.recv().is_ok() {
            let counts = allocation_counter::measure(|| {
                assert_eq!(render(&mut engine, &[]).0, Swap::Applied);
            });
            assert_eq!(counts.count_total, 0);
            assert_eq!(counts.count_current, 0);
            done_tx.send(()).unwrap();
        }
    });
    let mut next = prepared(2., &drops, &bad);
    for _ in 0..100 {
        control.offer(next).unwrap();
        tick_tx.send(()).unwrap();
        done_rx.recv().unwrap();
        next = control.take_retired().unwrap();
    }
    drop(tick_tx);
    worker.join().unwrap();
    drop(next);
    drop(control);
    assert_eq!(bad.load(Ordering::Relaxed), 0);
}

#[test]
fn f64_handoff_preserves_native_route_gain_and_sample_precision() {
    let direct = |gain: f64| {
        let mut graph = Box::new(
            Graph::<f64>::new(
                AudioLayout::MONO,
                vec![],
                &[Connection {
                    source: Source::Input(0),
                    destination: Destination::Output(0),
                    gain,
                }],
            )
            .unwrap(),
        );
        graph
            .prepare(ProcessConfig::new(48000., 4, 0, AudioLayout::MONO).unwrap())
            .unwrap();
        graph
    };
    let precise = 1. + 2_f64.powi(-40);
    let (mut control, mut engine) = DynamicGraph::new(direct(1.), 1).unwrap();
    control.offer(direct(precise)).unwrap();
    let mut samples = [precise; 4];
    let mut channels = [samples.as_mut_slice()];
    let audio = AudioBuffer::in_place(&mut channels, 4).unwrap();
    let mut block = ProcessBlock::new(audio, &[], Transport::default());
    let counts = allocation_counter::measure(|| {
        let status = engine.process(&mut block);
        assert_eq!(status.swap, Swap::Applied);
        assert_eq!(status.processing, GraphStatus::Continue);
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(samples, [precise * precise; 4]);
    assert_eq!(engine.latency_samples(), 0);
    assert_eq!(control.reclaim(), 1);
}
