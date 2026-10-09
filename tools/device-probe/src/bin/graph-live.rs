//! Offline live native graph replacement through buffered input/output.
use clack_host::prelude::*;
use ravet_audio_file::audio_ring;
use ravet_device::PreparedDynamicGraphOutput;
use ravet_foundation::{AudioLayout, ProcessConfig, Tail, Transport};
use ravet_graph::{
    Connection, Destination, DynamicGraph, Graph, GraphNode, GraphSwapStatus as Swap, NodeId,
    Source,
};
use ravet_host::{
    ClapLibrary, HostRequests, HostedGraphNodeControl, HostedGraphNodeOptions, RequestHost,
    prepare_hosted_graph_node,
};
use std::sync::mpsc::sync_channel;
#[allow(unsafe_code)]
fn load(path: &std::path::Path) -> Result<ClapLibrary, ravet_host::HostError> {
    // SAFETY: explicit trusted compliant public native gain; no inferred trust.
    unsafe { ClapLibrary::load(path) }
}
fn prepare(
    library: &ClapLibrary,
    gain: f32,
    configuration: ProcessConfig,
) -> Result<(HostedGraphNodeControl<RequestHost>, Box<Graph>), Box<dyn std::error::Error>> {
    let instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        library.entry(),
        c"org.ravet.gain",
        &HostInfo::new("Offline live graph worker", "RAVET", "", "0.1")?,
    )?;
    let (control, node) = prepare_hosted_graph_node(
        instance,
        HostedGraphNodeOptions {
            configuration,
            declared_latency: 0,
            declared_tail: Tail::None,
        },
        |_, _| (),
    )
    .map_err(|error| format!("native preparation: {:?}", error.reason))?;
    let mut connections = Vec::new();
    for channel in 0..2 {
        connections.push(Connection {
            source: Source::Input(channel),
            destination: Destination::Node {
                id: NodeId(1),
                channel,
            },
            gain: 1.,
        });
        connections.push(Connection {
            source: Source::Node {
                id: NodeId(1),
                channel,
            },
            destination: Destination::Output(channel),
            gain,
        });
    }
    let mut graph = Graph::new(
        AudioLayout::STEREO,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::STEREO,
            processor: Box::new(node),
        }],
        &connections,
    )?;
    graph.prepare(configuration)?;
    Ok((control, Box::new(graph)))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        return Err(
            "usage: graph-live <trusted public RAVET Gain CLAP bundle> (offline only)".into(),
        );
    }
    let library = load(std::path::Path::new(&args[0]))?;
    let configuration = ProcessConfig::new(48000., 16, 0, AudioLayout::STEREO)?;
    let (mut first_control, first) = prepare(&library, 1., configuration)?;
    let (mut second_control, second) = prepare(&library, 0.5, configuration)?;
    let (mut control, engine) = DynamicGraph::new(first, 1)
        .map_err(|error| format!("dynamic preparation: {:?}", error.reason))?;
    let (mut source, input) =
        audio_ring(2, 16).map_err(|error| format!("input ring: {error:?}"))?;
    let (output, mut sink) = audio_ring(2, 8).map_err(|error| format!("output ring: {error:?}"))?;
    let mut prepared =
        PreparedDynamicGraphOutput::with_input(Box::new(engine), input, output, configuration)?;
    let (ready_tx, ready_rx) = sync_channel(0);
    let (go_tx, go_rx) = sync_channel(0);
    let worker = std::thread::spawn(move || {
        let mut session = prepared.start_audio().unwrap();
        let input: [f32; 32] =
            std::array::from_fn(|index| if index % 2 == 0 { 0.25 } else { -0.5 });
        let mut samples = [99.; 16];
        source.push(&input).unwrap();
        let first = session.pump(Transport::default()).unwrap();
        assert_eq!(first.swap, Some(Swap::Unchanged));
        sink.pop(&mut samples).unwrap();
        check(&samples, 1.);
        ready_tx.send(()).unwrap();
        go_rx.recv().unwrap();
        let pending = session.pump(Transport::default()).unwrap();
        assert_eq!(pending.swap, None);
        sink.pop(&mut samples).unwrap();
        check(&samples, 1.);
        source.push(&input).unwrap();
        let allocations = allocation_counter::measure(|| {
            let fresh = session.pump(Transport::default()).unwrap();
            assert_eq!(fresh.swap, Some(Swap::Applied));
            assert_eq!(fresh.input_frames_consumed, 16);
        });
        assert_eq!(allocations.count_total, 0);
        sink.pop(&mut samples).unwrap();
        check(&samples, 0.5);
        ready_tx.send(()).unwrap();
        go_rx.recv().unwrap();
        session.pump(Transport::default()).unwrap();
        sink.pop(&mut samples).unwrap();
        check(&samples, 0.5);
        let allocations = allocation_counter::measure(|| {
            for _ in 0..1000 {
                assert_eq!(source.push(&input).unwrap(), 16);
                let fresh = session.pump(Transport::default()).unwrap();
                assert_eq!(fresh.swap, Some(Swap::Unchanged));
                sink.pop(&mut samples).unwrap();
                check(&samples, 0.5);
                let pending = session.pump(Transport::default()).unwrap();
                assert_eq!(pending.swap, None);
                assert_eq!(pending.input_frames_consumed, 0);
                sink.pop(&mut samples).unwrap();
                check(&samples, 0.5);
            }
        });
        assert_eq!(allocations.count_total, 0);
        assert_eq!(allocations.count_current, 0);
        session.finish();
        prepared
    });
    ready_rx.recv()?;
    control
        .offer(second)
        .map_err(|error| format!("offer: {:?}", error.reason))?;
    go_tx.send(())?;
    ready_rx.recv()?;
    assert_eq!(control.reclaim(), 1);
    assert!(first_control.reclaim());
    drop(
        first_control
            .into_instance()
            .map_err(|_| "first native retirement pending")?,
    );
    go_tx.send(())?;
    let prepared = worker.join().map_err(|_| "native graph worker panicked")?;
    drop(prepared.into_graph());
    drop(control);
    assert!(second_control.reclaim());
    drop(
        second_control
            .into_instance()
            .map_err(|_| "second native retirement pending")?,
    );
    println!(
        "PASS 1002 queued native graph blocks/16032 frames, live replacement after old audio suffix, 1000 steady blocks, zero measured heap actions, main retired/current native cleanup; offline only"
    );
    Ok(())
}
fn check(samples: &[f32], gain: f32) {
    for pair in samples.as_chunks::<2>().0 {
        assert_eq!(*pair, [0.25 * gain, -0.5 * gain]);
    }
}
