//! Loaded public fixture output through a buffered worker, without devices.
use clack_host::prelude::*;
use ravet_audio_file::audio_ring;
use ravet_device::{
    GraphEventQueueConfig, GraphEventQueueError, PreparedGraphOutput, graph_event_queue,
};
use ravet_foundation::{
    AudioLayout, Event, EventData, ParameterId, PrepareError, ProcessBlock, ProcessConfig,
    ProcessStatus, Processor, Tail, Transport,
};
use ravet_graph::{
    Connection, Destination, EventConnection, EventEndpoint, EventFilter, Graph, GraphNode, NodeId,
    Source,
};
use ravet_host::{
    ClapLibrary, HostRequests, HostedGraphNodeBuilder, HostedGraphNodeOptions,
    HostedGraphParameterMapping, RequestHost,
};
struct Constant;
impl Processor for Constant {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        block.audio.output_mut(0).unwrap().fill(0.25);
        block.audio.output_mut(1).unwrap().fill(-0.5);
        ProcessStatus::Continue
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
}
#[allow(unsafe_code)]
fn load(path: &std::path::Path) -> Result<ClapLibrary, ravet_host::HostError> {
    // SAFETY: explicitly selected trusted compliant public validation fixture.
    unsafe { ClapLibrary::load(path) }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        return Err("usage: graph-telemetry <trusted public parameter-fixture library>".into());
    }
    let library = load(std::path::Path::new(&args[0]))?;
    let instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        library.entry(),
        c"org.ravet.host-parameter-fixture",
        &HostInfo::new("Buffered native output probe", "RAVET", "", "0.1")?,
    )?;
    let cfg =
        ProcessConfig::new(48000., 8, 4, AudioLayout::STEREO_INSTRUMENT)?.with_output_events(1);
    let node_cfg = ProcessConfig::new(48000., 8, 4, AudioLayout::STEREO)?.with_output_events(1);
    let (mut control, node) = HostedGraphNodeBuilder::new(HostedGraphNodeOptions {
        configuration: node_cfg,
        declared_latency: 0,
        declared_tail: Tail::None,
    })
    .with_parameter_input(vec![HostedGraphParameterMapping {
        graph_id: ParameterId(9001),
        native_id: ClapId::new(1),
    }])
    .with_parameter_output(vec![HostedGraphParameterMapping {
        graph_id: ParameterId(9100),
        native_id: ClapId::new(2),
    }])
    .prepare(instance, |_, _| ())?;
    let routes: Vec<_> = (0..2)
        .flat_map(|channel| {
            [
                Connection {
                    source: Source::Node {
                        id: NodeId(1),
                        channel,
                    },
                    destination: Destination::Node {
                        id: NodeId(2),
                        channel,
                    },
                    gain: 1.,
                },
                Connection {
                    source: Source::Node {
                        id: NodeId(2),
                        channel,
                    },
                    destination: Destination::Output(channel),
                    gain: 1.,
                },
            ]
        })
        .collect();
    let mut graph = Box::new(Graph::new(
        AudioLayout::STEREO_INSTRUMENT,
        vec![
            GraphNode {
                id: NodeId(1),
                layout: AudioLayout::STEREO_INSTRUMENT,
                processor: Box::new(Constant),
            },
            GraphNode {
                id: NodeId(2),
                layout: AudioLayout::STEREO,
                processor: Box::new(node),
            },
        ],
        &routes,
    )?);
    graph.set_event_routes(&[
        EventConnection {
            source: EventEndpoint::Host,
            destination: EventEndpoint::Node(NodeId(2)),
            filter: EventFilter::Parameter(ParameterId(9001)),
        },
        EventConnection {
            source: EventEndpoint::Node(NodeId(2)),
            destination: EventEndpoint::Host,
            filter: EventFilter::Parameter(ParameterId(9100)),
        },
    ])?;
    let (producer, mut consumer) = audio_ring(2, 4).map_err(|error| format!("ring: {error:?}"))?;
    let mut prepared = PreparedGraphOutput::new(graph, producer, cfg)?;
    let (mut inputs, mut incoming) = graph_event_queue(GraphEventQueueConfig {
        block_frames: 8,
        event_capacity: 4,
        queued_batches: 2,
    })?;
    let (mut generated, mut outgoing) = graph_event_queue(GraphEventQueueConfig {
        block_frames: 8,
        event_capacity: 1,
        queued_batches: 2,
    })?;
    let changes = [(0, 2.), (4, 3.), (4, 1.), (7, 0.)].map(|(frame_offset, value)| Event {
        frame_offset,
        data: EventData::ParameterValue {
            id: ParameterId(9001),
            value,
        },
    });
    for tag in 0..2 {
        inputs.try_publish(tag, Transport::default(), &changes)?;
    }
    let worker = std::thread::spawn(move || {
        {
            let mut session = prepared.start_audio().unwrap();
            let mut storage = [Event {
                frame_offset: 0,
                data: EventData::ParameterValue {
                    id: ParameterId(0),
                    value: 0.,
                },
            }];
            let mut output = [0.; 8];
            let counts = allocation_counter::measure(|| {
                for tag in 0..1000 {
                    let batch = loop {
                        if let Some(batch) = incoming.next_batch() {
                            break batch;
                        }
                        assert!(
                            !incoming.producer_disconnected(),
                            "control input disconnected"
                        );
                        std::thread::yield_now();
                    };
                    assert_eq!(batch.tag(), tag);
                    let first = session
                        .pump_io(batch.events(), batch.transport(), &mut storage)
                        .unwrap();
                    assert_eq!(
                        (
                            first.rendered_frames,
                            first.published_frames,
                            first.pending_frames,
                            first.output_event_count
                        ),
                        (8, 4, 4, 1)
                    );
                    assert!(first.input_events_consumed);
                    assert_eq!(
                        storage[0],
                        Event {
                            frame_offset: 7,
                            data: EventData::ParameterValue {
                                id: ParameterId(9100),
                                value: 0.
                            }
                        }
                    );
                    let pending = session
                        .pump_io(batch.events(), batch.transport(), &mut storage)
                        .unwrap();
                    assert_eq!(
                        (
                            pending.rendered_frames,
                            pending.published_frames,
                            pending.output_event_count
                        ),
                        (0, 0, 0)
                    );
                    assert!(!pending.input_events_consumed);
                    assert_eq!(consumer.pop(&mut output).unwrap(), 4);
                    assert_eq!(output, [0.5, -1., 0.5, -1., 0.5, -1., 0.5, -1.]);
                    let pending = session
                        .pump_io(batch.events(), batch.transport(), &mut storage)
                        .unwrap();
                    assert_eq!(
                        (
                            pending.rendered_frames,
                            pending.published_frames,
                            pending.output_event_count
                        ),
                        (0, 4, 0)
                    );
                    assert!(!pending.input_events_consumed);
                    assert_eq!(consumer.pop(&mut output).unwrap(), 4);
                    assert_eq!(output, [0.25, -0.5, 0.25, -0.5, 0.25, -0.5, 0., 0.]);
                    loop {
                        match generated.try_publish(
                            tag,
                            batch.transport(),
                            &storage[..first.output_event_count],
                        ) {
                            Ok(()) => break,
                            Err(GraphEventQueueError::Full) => std::thread::yield_now(),
                            Err(error) => panic!("generated queue: {error:?}"),
                        }
                    }
                    batch.acknowledge();
                }
            });
            assert_eq!(counts.count_total, 0);
            assert_eq!(counts.count_current, 0);
        }
        prepared
    });
    let delivery = (|| -> Result<(), Box<dyn std::error::Error>> {
        for tag in 0..1000 {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                if let Some(batch) = outgoing.next_batch() {
                    assert_eq!(batch.tag(), tag);
                    assert_eq!(
                        batch.events(),
                        &[Event {
                            frame_offset: 7,
                            data: EventData::ParameterValue {
                                id: ParameterId(9100),
                                value: 0.
                            }
                        }]
                    );
                    batch.acknowledge();
                    break;
                }
                if outgoing.producer_disconnected() || std::time::Instant::now() > deadline {
                    return Err("worker telemetry delivery failed".into());
                }
                std::thread::yield_now();
            }
            if tag + 2 < 1000 {
                inputs.try_publish(tag + 2, Transport::default(), &changes)?;
            }
        }
        Ok(())
    })();
    drop(inputs);
    drop(outgoing);
    let prepared = worker
        .join()
        .map_err(|_| "native buffered output worker panicked")?;
    delivery?;
    drop(prepared.into_graph());
    assert!(control.reclaim());
    assert_eq!(control.statistics().processed_blocks, 1000);
    assert_eq!(control.statistics().processed_frames, 8000);
    assert_eq!(control.statistics().backend_failures, 0);
    assert_eq!(control.statistics().rejected_native_events, 0);
    let instance = control.into_instance().ok().ok_or("inactive main return")?;
    assert!(!instance.is_active());
    println!(
        "PASS 1000 loaded native parameter-I/O worker blocks/8000 frames, complete control/worker input-output queues, exactly-once read-only output despite retained queue suffixes, zero measured allocations and main retirement; no device/clock claim"
    );
    Ok(())
}
