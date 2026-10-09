use ravet_audio_file::audio_ring;
use ravet_device::{GraphOutputError, PreparedGraphOutput};
use ravet_foundation::{
    AudioLayout, Event, EventData, ParameterId, PrepareError, ProcessBlock, ProcessConfig,
    ProcessError, ProcessStatus, Processor, Transport,
};
use ravet_graph::{
    Connection, Destination, EventConnection, EventEndpoint, EventFilter, Graph, GraphNode,
    GraphStatus, NodeId, Source,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Effect {
    gain: f32,
    fail: bool,
    calls: Arc<AtomicUsize>,
}
impl Processor for Effect {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn reset(&mut self) {
        self.fail = false;
        self.gain = 1.;
    }
    fn deactivate(&mut self) {}
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.fail {
            return ProcessStatus::Fault(ProcessError::BackendFailure);
        }
        let mut next = 0;
        for frame in 0..block.audio.frames() {
            while next < block.events.len() && block.events[next].frame_offset == frame {
                if let EventData::ParameterValue { value, .. } = block.events[next].data {
                    self.gain = value;
                }
                next += 1;
            }
            let sample = block.audio.input(0).unwrap()[frame];
            block.audio.output_mut(0).unwrap()[frame] = sample * self.gain;
        }
        if let Some(output) = block.output_events.as_mut()
            && output.capacity() != 0
        {
            output
                .push(value(block.audio.frames() - 1, self.gain))
                .unwrap();
        }
        ProcessStatus::Continue
    }
}
fn value(frame_offset: usize, value: f32) -> Event {
    Event {
        frame_offset,
        data: EventData::ParameterValue {
            id: ParameterId(7),
            value,
        },
    }
}
fn graph(fail: bool, output_events: usize) -> (Arc<AtomicUsize>, Box<Graph>, ProcessConfig) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = Graph::new(
        AudioLayout::MONO,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::MONO,
            processor: Box::new(Effect {
                gain: 1.,
                fail,
                calls: calls.clone(),
            }),
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
    graph
        .set_event_routes(&[
            EventConnection {
                source: EventEndpoint::Host,
                destination: EventEndpoint::Node(NodeId(1)),
                filter: EventFilter::All,
            },
            EventConnection {
                source: EventEndpoint::Node(NodeId(1)),
                destination: EventEndpoint::Host,
                filter: EventFilter::All,
            },
        ])
        .unwrap();
    let configuration = ProcessConfig::new(48000., 8, 1, AudioLayout::MONO)
        .unwrap()
        .with_output_events(output_events);
    (calls, Box::new(graph), configuration)
}
#[test]
fn input_consumption_is_once_per_render_under_output_backpressure_and_missing_input_is_zero() {
    let (calls, graph, configuration) = graph(false, 1);
    let (mut source, input) = audio_ring(1, 16).unwrap();
    let (output, mut sink) = audio_ring(1, 4).unwrap();
    source
        .push(&[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.])
        .unwrap();
    let mut prepared =
        PreparedGraphOutput::with_input(graph, input, output, configuration).unwrap();
    let prepared = std::thread::spawn(move || {
        let mut session = prepared.start_audio().unwrap();
        let mut storage = [value(0, 0.)];
        let first = session
            .pump_io(&[value(2, 2.)], Transport::default(), &mut storage)
            .unwrap();
        assert_eq!(
            (
                first.rendered_frames,
                first.input_frames_consumed,
                first.input_underflow_frames,
                first.published_frames,
                first.pending_frames
            ),
            (8, 8, 0, 4, 4)
        );
        assert_eq!(storage, [value(7, 2.)]);
        let pending = session
            .pump_io(&[value(100, 99.)], Transport::default(), &mut [])
            .unwrap();
        assert_eq!(
            (
                pending.rendered_frames,
                pending.input_frames_consumed,
                pending.input_underflow_frames,
                pending.output_event_count
            ),
            (0, 0, 0, 0)
        );
        let mut samples = [99.; 4];
        assert_eq!(sink.pop(&mut samples).unwrap(), 4);
        assert_eq!(samples, [1., 2., 6., 8.]);
        session.pump_io(&[], Transport::default(), &mut []).unwrap();
        assert_eq!(sink.pop(&mut samples).unwrap(), 4);
        assert_eq!(samples, [10., 12., 14., 16.]);
        let next = session
            .pump_io(&[], Transport::default(), &mut storage)
            .unwrap();
        assert_eq!(
            (next.input_frames_consumed, next.input_underflow_frames),
            (2, 6)
        );
        assert_eq!(sink.pop(&mut samples).unwrap(), 4);
        assert_eq!(samples, [18., 20., 0., 0.]);
        session.pump_io(&[], Transport::default(), &mut []).unwrap();
        assert_eq!(sink.pop(&mut samples).unwrap(), 4);
        assert_eq!(samples, [0.; 4]);
        session.finish();
        prepared
    })
    .join()
    .unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    drop(prepared.into_graph());
}
#[test]
fn malformed_outer_contracts_preserve_input_and_do_not_latch_then_prepared_pumping_is_heap_free() {
    let (calls, graph, configuration) = graph(false, 1);
    let (mut source, input) = audio_ring(1, 8).unwrap();
    let (output, mut sink) = audio_ring(1, 8).unwrap();
    source.push(&[1.; 8]).unwrap();
    let mut prepared =
        PreparedGraphOutput::with_input(graph, input, output, configuration).unwrap();
    let prepared = std::thread::spawn(move || {
        let mut session = prepared.start_audio().unwrap();
        let mut storage = [value(0, 0.)];
        assert!(matches!(
            session.pump_events(&[], Transport::default()),
            Err(GraphStatus::Fault {
                node: None,
                error: ProcessError::OutputEventCapacity
            })
        ));
        assert!(matches!(
            session.pump_io(&[value(8, 2.)], Transport::default(), &mut storage),
            Err(GraphStatus::Fault {
                node: None,
                error: ProcessError::Events(_)
            })
        ));
        let first = session
            .pump_io(&[], Transport::default(), &mut storage)
            .unwrap();
        assert_eq!(first.input_frames_consumed, 8);
        let mut samples = [99.; 8];
        sink.pop(&mut samples).unwrap();
        assert_eq!(samples, [1.; 8]);
        let allocations = allocation_counter::measure(|| {
            for _ in 0..1000 {
                assert_eq!(source.push(&[2.; 8]).unwrap(), 8);
                let progress = session
                    .pump_io(&[], Transport::default(), &mut storage)
                    .unwrap();
                assert_eq!(
                    (
                        progress.input_frames_consumed,
                        progress.input_underflow_frames
                    ),
                    (8, 0)
                );
                assert_eq!(sink.pop(&mut samples).unwrap(), 8);
                assert_eq!(samples, [2.; 8]);
            }
        });
        assert_eq!(allocations.count_total, 0);
        assert_eq!(allocations.count_current, 0);
        session.finish();
        prepared
    })
    .join()
    .unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1001);
    drop(prepared.into_graph());
}
#[test]
fn graph_fault_consumes_only_first_input_block_and_latches_until_explicit_reset() {
    let (calls, graph, configuration) = graph(true, 0);
    let (mut source, input) = audio_ring(1, 16).unwrap();
    let (output, mut sink) = audio_ring(1, 8).unwrap();
    source.push(&[1.; 8]).unwrap();
    source.push(&[2.; 8]).unwrap();
    let mut prepared =
        PreparedGraphOutput::with_input(graph, input, output, configuration).unwrap();
    let prepared = std::thread::spawn(move || {
        let mut session = prepared.start_audio().unwrap();
        let fault = session.pump(Transport::default()).unwrap_err();
        assert_eq!(session.pump(Transport::default()), Err(fault));
        let mut samples = [99.; 8];
        assert_eq!(sink.pop(&mut samples).unwrap(), 0);
        assert_eq!(samples, [99.; 8]);
        session.reset();
        let next = session.pump(Transport::default()).unwrap();
        assert_eq!(next.input_frames_consumed, 8);
        assert_eq!(sink.pop(&mut samples).unwrap(), 8);
        assert_eq!(samples, [2.; 8]);
        session.finish();
        prepared
    })
    .join()
    .unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    drop(prepared.into_graph());
}
#[test]
fn constructor_requires_matching_input_channels_and_combined_worker_memory_budget() {
    let (_, graph, configuration) = graph(false, 0);
    let (_source, input) = audio_ring(2, 8).unwrap();
    let (output, _sink) = audio_ring(1, 8).unwrap();
    assert!(matches!(
        PreparedGraphOutput::with_input(graph, input, output, configuration),
        Err(GraphOutputError::Configuration)
    ));
    let layout = AudioLayout::new(64, 64).unwrap();
    let graph = Box::new(Graph::new(layout, vec![], &[]).unwrap());
    let (_source, input) = audio_ring(64, 8).unwrap();
    let (output, _sink) = audio_ring(64, 8).unwrap();
    assert!(matches!(
        PreparedGraphOutput::with_input(
            graph,
            input,
            output,
            ProcessConfig::new(48000., 65536, 0, layout).unwrap()
        ),
        Err(GraphOutputError::Configuration)
    ));
}
