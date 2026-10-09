use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, OutputEvents, ParameterId, PrepareError,
    ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor, Transport,
};
use ravet_graph::{
    Connection, Destination, EventConnection, EventEndpoint as EP, EventFilter as EF, Graph,
    GraphError, GraphNode, GraphStatus, NodeId, Source,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Echo {
    config: Option<ProcessConfig>,
    calls: Arc<AtomicUsize>,
}
impl Processor for Echo {
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
        self.calls.fetch_add(1, Ordering::Relaxed);
        block
            .audio
            .output_mut(0)
            .unwrap()
            .fill(block.events.len() as f32);
        if let Some(output) = &mut block.output_events {
            for &event in block.events {
                output.push(event).unwrap();
            }
        }
        ProcessStatus::Continue
    }
}
fn node(id: u32, layout: AudioLayout, calls: &Arc<AtomicUsize>) -> GraphNode {
    GraphNode {
        id: NodeId(id),
        layout,
        processor: Box::new(Echo {
            config: None,
            calls: calls.clone(),
        }),
    }
}
fn edge(source: EP, destination: EP, filter: EF) -> EventConnection {
    EventConnection {
        source,
        destination,
        filter,
    }
}
fn parameter(offset: usize, id: u32) -> Event {
    Event {
        frame_offset: offset,
        data: EventData::ParameterValue {
            id: ParameterId(id),
            value: id as f32,
        },
    }
}

#[test]
fn parameter_filters_include_gestures_and_performance_filters_exclude_them() {
    use ravet_foundation::ParameterGesturePhase;
    let calls = Arc::new(AtomicUsize::new(0));
    let events = [
        Event {
            frame_offset: 0,
            data: EventData::ParameterGesture {
                id: ParameterId(1),
                phase: ParameterGesturePhase::Begin,
            },
        },
        parameter(1, 1),
        Event {
            frame_offset: 1,
            data: EventData::ParameterGesture {
                id: ParameterId(1),
                phase: ParameterGesturePhase::End,
            },
        },
        Event {
            frame_offset: 2,
            data: EventData::NoteOn {
                channel: 2,
                key: 60,
                velocity: 0.5,
            },
        },
    ];
    for filter in [EF::Parameter(ParameterId(1)), EF::Performance] {
        let mut graph = Graph::new(
            AudioLayout::MONO_INSTRUMENT,
            vec![node(1, AudioLayout::MONO_INSTRUMENT, &calls)],
            &[],
        )
        .unwrap();
        graph
            .set_event_routes(&[
                edge(EP::Host, EP::Node(NodeId(1)), filter),
                edge(EP::Node(NodeId(1)), EP::Host, EF::All),
            ])
            .unwrap();
        graph
            .prepare(
                ProcessConfig::new(48000., 4, 4, AudioLayout::MONO_INSTRUMENT)
                    .unwrap()
                    .with_output_events(4),
            )
            .unwrap();
        let (status, _, storage, count) = render(&mut graph, &events, 4, 4);
        assert_eq!(status, GraphStatus::Continue);
        assert_eq!(
            &storage[..count],
            if matches!(filter, EF::Performance) {
                &events[3..]
            } else {
                &events[..3]
            }
        );
        let modulation_events = [
            Event {
                frame_offset: 0,
                data: EventData::ParameterModulation {
                    id: ParameterId(1),
                    amount: -3.,
                },
            },
            events[3],
        ];
        let (status, _, storage, count) = render(&mut graph, &modulation_events, 4, 4);
        assert_eq!(status, GraphStatus::Continue);
        assert_eq!(
            &storage[..count],
            if matches!(filter, EF::Performance) {
                &modulation_events[1..]
            } else {
                &modulation_events[..1]
            }
        );
        let scoped_events = [
            Event {
                frame_offset: 0,
                data: EventData::ParameterModulationScoped {
                    id: ParameterId(1),
                    target: ravet_foundation::NoteTarget::default(),
                    amount: -f64::MAX,
                },
            },
            events[3],
        ];
        let (status, _, storage, count) = render(&mut graph, &scoped_events, 4, 4);
        assert_eq!(status, GraphStatus::Continue);
        assert_eq!(
            &storage[..count],
            if matches!(filter, EF::Performance) {
                &scoped_events[1..]
            } else {
                &scoped_events[..1]
            }
        );
        let completion_events = [
            events[0],
            Event {
                frame_offset: 2,
                data: EventData::NoteEnd {
                    target: ravet_foundation::NoteTarget::default(),
                },
            },
        ];
        let (status, _, storage, count) = render(&mut graph, &completion_events, 4, 4);
        assert_eq!(status, GraphStatus::Continue);
        assert_eq!(
            &storage[..count],
            if matches!(filter, EF::Performance) {
                &completion_events[1..]
            } else {
                &completion_events[..1]
            }
        );
        let identified_events = [
            events[1],
            Event {
                frame_offset: 2,
                data: EventData::NoteWithId {
                    id: ravet_foundation::NoteId(u32::MAX),
                    phase: ravet_foundation::NotePhase::On,
                    channel: 2,
                    key: 60,
                    velocity: 0.,
                },
            },
        ];
        let (status, _, storage, count) = render(&mut graph, &identified_events, 4, 4);
        assert_eq!(status, GraphStatus::Continue);
        assert_eq!(
            &storage[..count],
            if matches!(filter, EF::Performance) {
                &identified_events[1..]
            } else {
                &identified_events[..1]
            }
        );
        let expression_events = [
            events[1],
            Event {
                frame_offset: 2,
                data: EventData::NoteExpression {
                    target: ravet_foundation::NoteTarget::default(),
                    expression: ravet_foundation::NoteExpression::Pressure,
                    value: 0.12345678912345678,
                },
            },
        ];
        let (status, _, storage, count) = render(&mut graph, &expression_events, 4, 4);
        assert_eq!(status, GraphStatus::Continue);
        assert_eq!(
            &storage[..count],
            if matches!(filter, EF::Performance) {
                &expression_events[1..]
            } else {
                &expression_events[..1]
            }
        );
        let release_events = [
            events[1],
            Event {
                frame_offset: 2,
                data: EventData::NoteOffScoped {
                    target: ravet_foundation::NoteTarget::default(),
                    velocity: 0.5,
                },
            },
        ];
        let (status, _, storage, count) = render(&mut graph, &release_events, 4, 4);
        assert_eq!(status, GraphStatus::Continue);
        assert_eq!(
            &storage[..count],
            if matches!(filter, EF::Performance) {
                &release_events[1..]
            } else {
                &release_events[..1]
            }
        );
    }
}
fn render(
    graph: &mut Graph,
    events: &[Event],
    capacity: usize,
    frames: usize,
) -> (GraphStatus, [f32; 4], [Event; 4], usize) {
    let mut samples = [99.; 4];
    let mut channels = [samples.as_mut_slice()];
    let audio = AudioBuffer::separate(&[], &mut channels, frames).unwrap();
    let mut storage = [parameter(0, 99); 4];
    let mut block = ProcessBlock::new(audio, events, Transport::default());
    block.output_events = Some(OutputEvents::new(&mut storage[..capacity], frames));
    let status = graph.process(&mut block);
    let count = block.output_events.as_ref().unwrap().as_slice().len();
    (status, samples, storage, count)
}
fn chain() -> (Graph, [Event; 4]) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = Graph::new(
        AudioLayout::MONO_INSTRUMENT,
        vec![
            node(2, AudioLayout::MONO_INSTRUMENT, &calls),
            node(1, AudioLayout::MONO_INSTRUMENT, &calls),
        ],
        &[Connection {
            source: Source::Node {
                id: NodeId(2),
                channel: 0,
            },
            destination: Destination::Output(0),
            gain: 1.,
        }],
    )
    .unwrap();
    graph
        .set_event_routes(&[
            edge(EP::Host, EP::Node(NodeId(1)), EF::Parameter(ParameterId(1))),
            edge(EP::Host, EP::Node(NodeId(2)), EF::Performance),
            edge(EP::Node(NodeId(1)), EP::Node(NodeId(2)), EF::All),
            edge(EP::Node(NodeId(2)), EP::Host, EF::All),
            edge(EP::Host, EP::Host, EF::Parameter(ParameterId(3))),
        ])
        .unwrap();
    graph
        .prepare(
            ProcessConfig::new(48000., 4, 4, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_output_events(4),
        )
        .unwrap();
    let events = [
        parameter(0, 1),
        Event {
            frame_offset: 1,
            data: EventData::NoteOn {
                channel: 0,
                key: 60,
                velocity: 1.,
            },
        },
        parameter(1, 3),
        parameter(1, 1),
    ];
    (graph, events)
}
#[test]
fn filters_combined_topology_equal_time_order_and_zero_frame_events_are_explicit() {
    let (mut graph, mut events) = chain();
    let (status, samples, output, count) = render(&mut graph, &events, 4, 4);
    assert_eq!(status, GraphStatus::Continue);
    assert_eq!(samples, [3.; 4]);
    assert_eq!(count, 4);
    assert_eq!(output, [events[0], events[1], events[3], events[2]]);
    for event in &mut events {
        event.frame_offset = 0;
    }
    let (status, samples, output, count) = render(&mut graph, &events, 4, 0);
    assert_eq!(status, GraphStatus::Continue);
    assert_eq!(samples, [99.; 4]);
    assert_eq!(count, 4);
    assert_eq!(output, [events[1], events[0], events[3], events[2]]);
}
#[test]
fn merge_overflow_discards_host_output_disables_graph_and_exposes_nonrollback_progress() {
    let first = Arc::new(AtomicUsize::new(0));
    let second = Arc::new(AtomicUsize::new(0));
    let mut graph = Graph::new(
        AudioLayout::MONO_INSTRUMENT,
        vec![
            node(1, AudioLayout::MONO_INSTRUMENT, &first),
            node(2, AudioLayout::MONO_INSTRUMENT, &second),
        ],
        &[],
    )
    .unwrap();
    graph
        .set_event_routes(&[
            edge(EP::Host, EP::Node(NodeId(1)), EF::All),
            edge(EP::Node(NodeId(1)), EP::Node(NodeId(2)), EF::All),
            edge(EP::Node(NodeId(1)), EP::Node(NodeId(2)), EF::All),
        ])
        .unwrap();
    graph
        .prepare(
            ProcessConfig::new(48000., 4, 2, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_output_events(2),
        )
        .unwrap();
    let events = [parameter(0, 1), parameter(1, 2)];
    let (status, samples, _, count) = render(&mut graph, &events, 2, 4);
    assert!(matches!(
        status,
        GraphStatus::EventFault {
            source: Some(NodeId(1)),
            destination: Some(NodeId(2)),
            ..
        }
    ));
    assert_eq!(samples, [0.; 4]);
    assert_eq!(count, 0);
    assert_eq!(first.load(Ordering::Relaxed), 1);
    assert_eq!(second.load(Ordering::Relaxed), 0);
    assert_eq!(
        render(&mut graph, &events, 2, 4).0,
        GraphStatus::Fault {
            node: None,
            error: ProcessError::NotPrepared
        }
    );
    graph.reset();
    assert_eq!(
        render(&mut graph, &events[..1], 2, 4).0,
        GraphStatus::Continue
    );
    graph.deactivate();
    graph
        .set_event_routes(&[
            edge(EP::Host, EP::Node(NodeId(1)), EF::All),
            edge(EP::Node(NodeId(1)), EP::Host, EF::All),
            edge(EP::Node(NodeId(1)), EP::Host, EF::All),
        ])
        .unwrap();
    graph
        .prepare(
            ProcessConfig::new(48000., 4, 2, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_output_events(2),
        )
        .unwrap();
    let (status, samples, _, count) = render(&mut graph, &events, 2, 4);
    assert!(matches!(
        status,
        GraphStatus::EventFault {
            source: Some(NodeId(1)),
            destination: None,
            ..
        }
    ));
    assert_eq!(samples, [0.; 4]);
    assert_eq!(count, 0);
}
#[test]
fn event_route_edits_reject_missing_nodes_active_state_and_combined_cycles_atomically() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = Graph::new(
        AudioLayout::MONO_INSTRUMENT,
        vec![
            node(1, AudioLayout::MONO, &calls),
            node(2, AudioLayout::MONO, &calls),
        ],
        &[Connection {
            source: Source::Node {
                id: NodeId(1),
                channel: 0,
            },
            destination: Destination::Node {
                id: NodeId(2),
                channel: 0,
            },
            gain: 1.,
        }],
    )
    .unwrap();
    assert_eq!(
        graph.set_event_routes(&[edge(EP::Node(NodeId(2)), EP::Node(NodeId(1)), EF::All)]),
        Err(GraphError::Cycle)
    );
    assert_eq!(
        graph.set_event_routes(&[edge(EP::Host, EP::Node(NodeId(99)), EF::All)]),
        Err(GraphError::MissingNode(NodeId(99)))
    );
    graph.set_event_routes(&[]).unwrap();
    graph
        .prepare(ProcessConfig::new(48000., 4, 1, AudioLayout::MONO_INSTRUMENT).unwrap())
        .unwrap();
    assert_eq!(graph.set_event_routes(&[]), Err(GraphError::Active));
    assert_eq!(
        render(&mut graph, &[parameter(0, 1)], 0, 4).0,
        GraphStatus::Continue
    );
}
#[test]
fn prepared_event_merges_and_reset_have_no_heap_operations() {
    let (mut graph, events) = chain();
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            let (status, _, output, count) = render(&mut graph, &events, 4, 4);
            assert_eq!(status, GraphStatus::Continue);
            assert_eq!(count, 4);
            assert_eq!(output, [events[0], events[1], events[3], events[2]]);
            graph.reset();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}

#[test]
fn opaque_ump_words_route_without_quantization_in_native_f64_graphs() {
    use ravet_foundation::UmpPacket;
    let packet = UmpPacket::from_words(&[0x4f913cff, 0xabcd1234]).unwrap();
    let event = Event {
        frame_offset: 0,
        data: EventData::Ump(packet),
    };
    let mut graph = Graph::<f64>::new(AudioLayout::MONO_INSTRUMENT, vec![], &[]).unwrap();
    graph
        .set_event_routes(&[edge(EP::Host, EP::Host, EF::Performance)])
        .unwrap();
    graph
        .prepare(
            ProcessConfig::new(48000., 4, 1, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_output_events(1),
        )
        .unwrap();
    let mut samples = [99_f64; 4];
    let mut channels = [samples.as_mut_slice()];
    let audio = AudioBuffer::separate(&[], &mut channels, 4).unwrap();
    let mut storage = [parameter(0, 99)];
    let input = [event];
    let mut block = ProcessBlock::new(audio, &input, Transport::default());
    block.output_events = Some(OutputEvents::new(&mut storage, 4));
    assert_eq!(graph.process(&mut block), GraphStatus::Continue);
    assert_eq!(block.output_events.as_ref().unwrap().as_slice(), &[event]);
    assert_eq!(samples, [0.; 4]);
}
