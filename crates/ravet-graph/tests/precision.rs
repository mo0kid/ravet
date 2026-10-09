use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor,
    Tail, Transport,
};
use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
struct Delay64 {
    config: Option<ProcessConfig>,
    history: [f64; 2],
    position: usize,
}
impl Processor<f64> for Delay64 {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(config);
        self.reset();
        Ok(())
    }
    fn reset(&mut self) {
        self.history.fill(0.);
        self.position = 0;
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
    fn latency_samples(&self) -> u32 {
        2
    }
    fn tail(&self) -> Tail {
        Tail::Samples(2)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64>) -> ProcessStatus {
        self.config.unwrap().validate_block(block).unwrap();
        for frame in 0..block.audio.frames() {
            let input = block.audio.input(0).unwrap()[frame];
            let old = self.history[self.position];
            self.history[self.position] = input;
            self.position = (self.position + 1) % 2;
            block.audio.output_mut(0).unwrap()[frame] = old;
        }
        ProcessStatus::Continue
    }
}
fn node(layout: AudioLayout) -> GraphNode<f64> {
    GraphNode {
        id: NodeId(1),
        layout,
        processor: Box::new(Delay64 {
            config: None,
            history: [0.; 2],
            position: 0,
        }),
    }
}
fn render(graph: &mut Graph<f64>, samples: &mut [f64], frames: usize) {
    let mut channels = [samples];
    let audio = AudioBuffer::in_place(&mut channels, frames).unwrap();
    assert_eq!(
        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
        GraphStatus::Continue
    );
}
#[test]
fn native_precision_survives_routes_latency_and_block_partitions_without_heap() {
    let source = Source::Node {
        id: NodeId(1),
        channel: 0,
    };
    let routes = [
        Connection {
            source: Source::Input(0),
            destination: Destination::Node {
                id: NodeId(1),
                channel: 0,
            },
            gain: 1.,
        },
        Connection {
            source,
            destination: Destination::Output(0),
            gain: 0.5,
        },
        Connection {
            source: Source::Input(0),
            destination: Destination::Output(0),
            gain: 0.5,
        },
    ];
    let mut graph =
        Graph::<f64>::new(AudioLayout::MONO, vec![node(AudioLayout::MONO)], &routes).unwrap();
    graph
        .prepare(ProcessConfig::new(48000., 8, 0, AudioLayout::MONO).unwrap())
        .unwrap();
    assert_eq!(graph.latency_samples(), 2);
    assert_eq!(graph.tail(), Tail::Samples(2));
    let precise = 1. + 2_f64.powi(-40);
    assert_ne!(precise, f64::from(precise as f32));
    let mut samples = [precise, 0., 0., 0., 0., 99.];
    render(&mut graph, &mut samples, 5);
    assert_eq!(samples, [0., 0., precise, 0., 0., 99.]);
    graph.reset();
    let mut first = [precise];
    let mut second = [0.; 4];
    render(&mut graph, &mut first, 1);
    render(&mut graph, &mut second, 4);
    assert_eq!(first, [0.]);
    assert_eq!(second, [0., precise, 0., 0.]);
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            graph.reset();
            let mut samples = [precise, 0., 0., 0.];
            render(&mut graph, &mut samples, 4);
            assert_eq!(samples, [0., 0., precise, 0.]);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    let gain = 1. + 2_f64.powi(-40);
    let mut direct = Graph::<f64>::new(
        AudioLayout::STEREO,
        vec![],
        &[
            Connection {
                source: Source::Input(0),
                destination: Destination::Output(1),
                gain,
            },
            Connection {
                source: Source::Input(1),
                destination: Destination::Output(0),
                gain: 1.,
            },
        ],
    )
    .unwrap();
    direct
        .prepare(ProcessConfig::new(48000., 1, 0, AudioLayout::STEREO).unwrap())
        .unwrap();
    let mut left = [precise];
    let mut right = [2.];
    let mut channels = [left.as_mut_slice(), right.as_mut_slice()];
    let audio = AudioBuffer::in_place(&mut channels, 1).unwrap();
    assert_eq!(
        direct.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
        GraphStatus::Continue
    );
    assert_eq!(left, [2.]);
    assert_eq!(right, [precise * gain]);
}
#[test]
fn f64_preparation_caps_bytes_and_rejects_nonfinite_native_gains() {
    let layout = AudioLayout::new(64, 64).unwrap();
    let mut graph = Graph::<f64>::new(AudioLayout::MONO, vec![node(layout)], &[]).unwrap();
    // 129 channels * 65536 * 8 exceeds 64 MiB; rejects before scratch allocation.
    assert_eq!(
        graph.prepare(ProcessConfig::new(48000., 65536, 0, AudioLayout::MONO).unwrap()),
        Err(PrepareError::InvalidMaximumFrames)
    );
    assert!(
        Graph::<f64>::new(
            AudioLayout::MONO,
            vec![],
            &[Connection {
                source: Source::Input(0),
                destination: Destination::Output(0),
                gain: f64::NAN
            }]
        )
        .is_err()
    );
}
