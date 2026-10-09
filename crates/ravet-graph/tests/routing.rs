use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, ParameterId, PrepareError, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Tail, Transport,
};
use ravet_gain::GainProcessor;
use ravet_graph::{
    Connection, Destination, Graph, GraphError, GraphNode, GraphStatus, NodeId, Source,
};
fn node(id: u32) -> GraphNode {
    GraphNode {
        id: NodeId(id),
        layout: AudioLayout::MONO,
        processor: Box::new(GainProcessor::new(0.).unwrap()),
    }
}
fn source(id: u32) -> Source {
    Source::Node {
        id: NodeId(id),
        channel: 0,
    }
}
fn destination(id: u32) -> Destination {
    Destination::Node {
        id: NodeId(id),
        channel: 0,
    }
}
fn connection(source: Source, destination: Destination, gain: f32) -> Connection {
    Connection {
        source,
        destination,
        gain,
    }
}
fn config() -> ProcessConfig {
    ProcessConfig::new(48000., 64, 16, AudioLayout::MONO).unwrap()
}
fn render(graph: &mut Graph, samples: &mut [f32], frames: usize, events: &[Event]) -> GraphStatus {
    let mut views = [samples];
    let audio = AudioBuffer::in_place(&mut views, frames).unwrap();
    graph.process(&mut ProcessBlock::new(audio, events, Transport::default()))
}
#[test]
fn topological_serial_parallel_gain_and_in_place_input_snapshot() {
    let routes = [
        connection(Source::Input(0), destination(10), 1.),
        connection(source(10), destination(20), 1.),
        connection(source(20), Destination::Output(0), 0.5),
        connection(Source::Input(0), Destination::Output(0), 0.5),
    ];
    // Definitions intentionally reverse execution order.
    let mut graph = Graph::new(AudioLayout::MONO, vec![node(20), node(10)], &routes).unwrap();
    graph.prepare(config()).unwrap();
    let event = Event {
        frame_offset: 1,
        data: EventData::ParameterValue {
            id: ParameterId(1),
            value: 2.,
        },
    };
    let mut samples = [1., 2., 3., 99.];
    assert_eq!(
        render(&mut graph, &mut samples, 3, &[event]),
        GraphStatus::Continue
    );
    assert_eq!(samples, [1., 5., 7.5, 99.]); // both nodes share gain ID by explicit broadcast
    assert_eq!(graph.latency_samples(), 0);
    assert_eq!(graph.tail(), Tail::None);
    let mut empty = [];
    let event = Event {
        frame_offset: 0,
        ..event
    };
    assert_eq!(
        render(&mut graph, &mut empty, 0, &[event]),
        GraphStatus::Continue
    );
}
struct Delay {
    config: Option<ProcessConfig>,
    history: [f32; 8],
    position: usize,
    count: usize,
}
impl Processor for Delay {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(c);
        self.reset();
        Ok(())
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.config.unwrap().validate_block(b).unwrap();
        for frame in 0..b.audio.frames() {
            let input = b.audio.input(0).unwrap()[frame];
            let sample = self.history[self.position];
            self.history[self.position] = input;
            self.position = (self.position + 1) % self.count;
            b.audio.output_mut(0).unwrap()[frame] = sample;
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.history.fill(0.);
        self.position = 0;
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
    fn latency_samples(&self) -> u32 {
        self.count as u32
    }
    fn tail(&self) -> Tail {
        Tail::Samples(self.count as u64)
    }
}
fn delayed(id: u32, count: usize) -> GraphNode {
    GraphNode {
        id: NodeId(id),
        layout: AudioLayout::MONO,
        processor: Box::new(Delay {
            config: None,
            history: [0.; 8],
            position: 0,
            count,
        }),
    }
}
#[test]
fn serial_latencies_compensate_parallel_dry_and_preserve_partitions() {
    let routes = [
        connection(Source::Input(0), destination(1), 1.),
        connection(source(1), destination(2), 1.),
        connection(source(2), Destination::Output(0), 0.5),
        connection(Source::Input(0), Destination::Output(0), 0.5),
    ];
    let create = || {
        let mut g = Graph::new(
            AudioLayout::MONO,
            vec![delayed(2, 3), delayed(1, 2)],
            &routes,
        )
        .unwrap();
        g.prepare(config()).unwrap();
        g
    };
    let mut a = create();
    let mut b = create();
    assert_eq!(a.latency_samples(), 5);
    assert_eq!(a.tail(), Tail::Samples(5));
    let mut whole = [1., 0., 0., 0., 0., 0., 0., 0., 0., 0.];
    let mut split = whole;
    assert_eq!(render(&mut a, &mut whole, 10, &[]), GraphStatus::Continue);
    render(&mut b, &mut split[..3], 3, &[]);
    render(&mut b, &mut split[3..], 7, &[]);
    assert_eq!(whole, split);
    assert_eq!(whole, [0., 0., 0., 0., 0., 1., 0., 0., 0., 0.]);
    a.reset();
    let mut next = [0.; 8];
    render(&mut a, &mut next, 8, &[]);
    assert_eq!(next, [0.; 8]);
}
struct Fault;
impl Processor for Fault {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        ProcessStatus::Fault(ProcessError::NotPrepared)
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
}
#[test]
fn invalid_outer_events_preserve_history_and_node_fault_disables_graph() {
    let routes = [
        connection(Source::Input(0), destination(1), 1.),
        connection(source(1), Destination::Output(0), 1.),
    ];
    let mut graph = Graph::new(AudioLayout::MONO, vec![delayed(1, 2)], &routes).unwrap();
    graph.prepare(config()).unwrap();
    let bad = Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: ParameterId(1),
            value: f32::NAN,
        },
    };
    let mut samples = [1.; 4];
    assert!(matches!(
        render(&mut graph, &mut samples, 4, &[bad]),
        GraphStatus::Fault { node: None, .. }
    ));
    assert_eq!(samples, [0.; 4]);
    let mut impulse = [1., 0., 0., 0.];
    render(&mut graph, &mut impulse, 4, &[]);
    assert_eq!(impulse, [0., 0., 1., 0.]);
    let mut graph = Graph::new(
        AudioLayout::MONO,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::MONO,
            processor: Box::new(Fault),
        }],
        &routes,
    )
    .unwrap();
    graph.prepare(config()).unwrap();
    let mut samples = [99.; 4];
    assert!(matches!(
        render(&mut graph, &mut samples, 4, &[]),
        GraphStatus::Fault {
            node: Some(NodeId(1)),
            ..
        }
    ));
    assert_eq!(samples, [0.; 4]);
    assert!(matches!(
        render(&mut graph, &mut samples, 4, &[]),
        GraphStatus::Fault {
            node: None,
            error: ProcessError::NotPrepared
        }
    ));
    graph.reset();
    assert!(matches!(
        render(&mut graph, &mut samples, 4, &[]),
        GraphStatus::Fault {
            node: Some(NodeId(1)),
            ..
        }
    ));
}
#[test]
fn cycles_bad_ids_channels_and_gains_are_rejected() {
    let cycle = [
        connection(source(1), destination(2), 1.),
        connection(source(2), destination(1), 1.),
    ];
    assert!(matches!(
        Graph::new(AudioLayout::MONO, vec![node(1), node(2)], &cycle),
        Err(GraphError::Cycle)
    ));
    assert!(matches!(
        Graph::new(AudioLayout::MONO, vec![node(1), node(1)], &[]),
        Err(GraphError::DuplicateId(NodeId(1)))
    ));
    assert!(matches!(
        Graph::new(
            AudioLayout::MONO,
            vec![node(1)],
            &[connection(source(99), Destination::Output(0), 1.)]
        ),
        Err(GraphError::MissingNode(NodeId(99)))
    ));
    assert!(matches!(
        Graph::new(
            AudioLayout::MONO,
            vec![],
            &[connection(Source::Input(1), Destination::Output(0), 1.)]
        ),
        Err(GraphError::InvalidChannel)
    ));
    assert!(matches!(
        Graph::new(
            AudioLayout::MONO,
            vec![],
            &[connection(
                Source::Input(0),
                Destination::Output(0),
                f32::NAN
            )]
        ),
        Err(GraphError::InvalidGain)
    ));
}
#[test]
fn prepared_graph_routes_and_reset_have_no_heap_operations() {
    let routes = [
        connection(Source::Input(0), destination(1), 1.),
        connection(source(1), Destination::Output(0), 1.),
        connection(Source::Input(0), Destination::Output(0), 0.25),
    ];
    let mut graph = Graph::new(AudioLayout::MONO, vec![delayed(1, 8)], &routes).unwrap();
    graph.prepare(config()).unwrap();
    let mut samples = [0.25; 64];
    let counts = allocation_counter::measure(|| {
        for i in 0..1000 {
            std::hint::black_box(render(&mut graph, &mut samples, 64, &[]));
            if i % 10 == 0 {
                graph.reset();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    graph.deactivate();
    assert!(matches!(
        render(&mut graph, &mut samples, 64, &[]),
        GraphStatus::Fault {
            error: ProcessError::NotPrepared,
            ..
        }
    ));
}

#[test]
fn empty_graph_cross_channel_aliases_and_failed_reprepare_are_safe() {
    let routes = [
        connection(Source::Input(0), Destination::Output(1), 1.),
        connection(Source::Input(1), Destination::Output(0), 1.),
    ];
    let mut graph = Graph::new(AudioLayout::STEREO, vec![], &routes).unwrap();
    graph
        .prepare(ProcessConfig::new(48000., 64, 0, AudioLayout::STEREO).unwrap())
        .unwrap();
    let mut left = [1., 2., 99.];
    let mut right = [3., 4., 99.];
    let mut views = [&mut left[..], &mut right[..]];
    let audio = AudioBuffer::in_place(&mut views, 2).unwrap();
    assert_eq!(
        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
        GraphStatus::Continue
    );
    assert_eq!(left, [3., 4., 99.]);
    assert_eq!(right, [1., 2., 99.]);
    assert!(graph.prepare(config()).is_err()); // mismatched layout leaves inactive
    let mut views = [&mut left[..], &mut right[..]];
    let audio = AudioBuffer::in_place(&mut views, 2).unwrap();
    assert!(matches!(
        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
        GraphStatus::Fault {
            error: ProcessError::NotPrepared,
            ..
        }
    ));
    assert_eq!(left, [0., 0., 99.]);
    assert_eq!(right, [0., 0., 99.]);
}
