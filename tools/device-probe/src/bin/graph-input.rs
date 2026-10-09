//! Offline queued input/output over a trusted public native gain graph.
use clack_host::prelude::*;
use ravet_audio_file::audio_ring;
use ravet_device::{ClockedDuplexOptions, PreparedClockedInput, PreparedGraphOutput};
use ravet_foundation::{AudioLayout, ProcessConfig, Tail, Transport};
use ravet_graph::{Connection, Destination, Graph, GraphNode, NodeId, Source};
use ravet_host::{
    ClapLibrary, HostRequests, HostedGraphNodeOptions, RequestHost, prepare_hosted_graph_node,
};
use ravet_signal::FifoClockConfig;
#[allow(unsafe_code)]
fn load(path: &std::path::Path) -> Result<ClapLibrary, ravet_host::HostError> {
    // SAFETY: explicit trusted compliant public gain bundle; no trust inference.
    unsafe { ClapLibrary::load(path) }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    let clocked = args.first().is_some_and(|arg| arg == "--clocked");
    if clocked {
        args.remove(0);
    }
    if args.len() != 1 {
        return Err(
            "usage: graph-input [--clocked] <trusted RAVET Gain CLAP bundle> (offline, no device/GUI)".into(),
        );
    }
    let library = load(std::path::Path::new(&args[0]))?;
    let instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        library.entry(),
        c"org.ravet.gain",
        &HostInfo::new("Offline queued graph input", "RAVET", "", "0.1")?,
    )?;
    let configuration = ProcessConfig::new(48000., 16, 0, AudioLayout::STEREO)?;
    let (mut control, node) = prepare_hosted_graph_node(
        instance,
        HostedGraphNodeOptions {
            configuration,
            declared_latency: 0,
            declared_tail: Tail::None,
        },
        |_, _| (),
    )
    .map_err(|error| format!("native graph preparation: {:?}", error.reason))?;
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
            gain: 1.,
        });
    }
    let graph = Box::new(Graph::new(
        AudioLayout::STEREO,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::STEREO,
            processor: Box::new(node),
        }],
        &connections,
    )?);
    let (mut source, input, mut converter) = if clocked {
        let (source, raw) =
            audio_ring(2, 128).map_err(|error| format!("raw input ring: {error:?}"))?;
        let (converted, input) =
            audio_ring(2, 16).map_err(|error| format!("converted input ring: {error:?}"))?;
        let converter = PreparedClockedInput::new(
            raw,
            converted,
            96000,
            48000,
            ClockedDuplexOptions {
                maximum_frames: 16,
                taps: 32,
                clock: FifoClockConfig {
                    capacity_frames: 128,
                    target_frames: 64,
                    ..FifoClockConfig::default()
                },
            },
        )?;
        (source, input, Some(converter))
    } else {
        let (source, input) =
            audio_ring(2, 16).map_err(|error| format!("input ring: {error:?}"))?;
        (source, input, None)
    };
    let (output, mut sink) = audio_ring(2, 8).map_err(|error| format!("output ring: {error:?}"))?;
    let mut prepared = PreparedGraphOutput::with_input(graph, input, output, configuration)?;
    let prepared = std::thread::spawn(move || {
        {
            let mut session = prepared.start_audio().unwrap();
            let input: [f32; 64] =
                std::array::from_fn(|index| if index % 2 == 0 { 0.25 } else { -0.5 });
            let mut samples = [99.; 16];
            if clocked {
                let prime: [f32; 128] =
                    std::array::from_fn(|index| if index % 2 == 0 { 0.25 } else { -0.5 });
                assert_eq!(source.push(&prime).unwrap(), 64);
            }
            for iteration in 0..1000 {
                let source_frames = if clocked { 32 } else { 16 };
                assert_eq!(
                    source.push(&input[..source_frames * 2]).unwrap(),
                    source_frames
                );
                let allocations = allocation_counter::measure(|| {
                    if let Some(converter) = converter.as_mut() {
                        let progress = converter.pump();
                        assert_eq!(
                            (
                                progress.converted_frames,
                                progress.published_frames,
                                progress.pending_frames
                            ),
                            (16, 16, 0)
                        );
                    }
                    let first = session.pump(Transport::default()).unwrap();
                    assert_eq!(
                        (
                            first.rendered_frames,
                            first.input_frames_consumed,
                            first.input_underflow_frames,
                            first.published_frames,
                            first.pending_frames
                        ),
                        (16, 16, 0, 8, 8)
                    );
                    assert_eq!(sink.pop(&mut samples).unwrap(), 8);
                    check_samples(&samples, &input[..16], clocked, iteration);
                    let pending = session.pump(Transport::default()).unwrap();
                    assert_eq!(
                        (
                            pending.rendered_frames,
                            pending.input_frames_consumed,
                            pending.input_underflow_frames,
                            pending.published_frames,
                            pending.pending_frames
                        ),
                        (0, 0, 0, 8, 0)
                    );
                    assert_eq!(sink.pop(&mut samples).unwrap(), 8);
                    check_samples(&samples, &input[..16], clocked, iteration);
                });
                assert_eq!(allocations.count_total, 0);
                assert_eq!(allocations.count_current, 0);
            }
            if let Some(converter) = converter.as_ref() {
                assert_eq!(converter.statistics().callbacks, 1000);
                assert_eq!(converter.clock_statistics().underflow_output_frames, 0);
            }
            let allocations = allocation_counter::measure(|| {
                let shortage = session.pump(Transport::default()).unwrap();
                assert_eq!(
                    (
                        shortage.input_frames_consumed,
                        shortage.input_underflow_frames
                    ),
                    (0, 16)
                );
                assert_eq!(sink.pop(&mut samples).unwrap(), 8);
                assert_eq!(samples, [0.; 16]);
                session.pump(Transport::default()).unwrap();
                assert_eq!(sink.pop(&mut samples).unwrap(), 8);
                assert_eq!(samples, [0.; 16]);
            });
            assert_eq!(allocations.count_total, 0);
            session.finish();
        }
        prepared
    })
    .join()
    .map_err(|_| "native queued graph worker panicked")?;
    drop(prepared.into_graph());
    assert!(control.reclaim());
    let _instance = control
        .into_instance()
        .map_err(|_| "native main retirement pending")?;
    println!(
        "PASS 1000 native gain input/output queue blocks/16000 graph input frames plus underflow block (clocked={clocked}), retained output suffixes, zero measured heap actions and owner stop/main retirement; offline only"
    );
    Ok(())
}

fn check_samples(samples: &[f32], expected: &[f32], clocked: bool, iteration: usize) {
    if !clocked {
        assert_eq!(samples, expected);
        return;
    }
    for pair in samples.as_chunks::<2>().0 {
        assert!(pair[0].is_finite());
        assert_eq!(pair[1], -2. * pair[0]);
        if iteration > 2 {
            assert!((pair[0] - 0.25).abs() < 0.001);
        }
    }
}
