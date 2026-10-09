//! Explicit trusted zero-latency stereo f32/f64 gain graph and audio-owner swap.
use clack_host::prelude::*;
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, ParameterId, ProcessBlock, ProcessConfig,
    Processor, Tail, Transport,
};
use ravet_graph::{
    Connection, Destination, DynamicGraph, DynamicGraphAudioSession, EventConnection,
    EventEndpoint, EventFilter, Graph, GraphNode, GraphSample, GraphStatus, GraphSwapStatus,
    NodeId, Source,
};
use ravet_host::{
    ClapLibrary, HostRequests, HostedGraphNodeBuilder, HostedGraphNodeControl,
    HostedGraphNodeOptions, HostedGraphNoteDialect, HostedGraphNoteInput, HostedGraphNoteOutput,
    HostedGraphParameterMapping, HostedGraphPortMode, RequestHost, prepare_hosted_graph_node,
    prepare_hosted_graph_node64, prepare_hosted_graph_ports_node,
    prepare_hosted_graph_ports_node64, prepare_hosted_parameter_graph_node,
    prepare_hosted_parameter_graph_node64,
};
use std::{
    ffi::{CStr, CString},
    path::PathBuf,
};
#[allow(unsafe_code)]
fn load(path: &std::path::Path) -> Result<ClapLibrary, ravet_host::HostError> {
    // SAFETY: caller-selected trusted compliant library; no scanner/sandbox or
    // filename/signature-based trust inference. Fixed known gain probe profile.
    unsafe { ClapLibrary::load(path) }
}
type NodeResult<S> = Result<
    (
        HostedGraphNodeControl<RequestHost>,
        Box<dyn Processor<S> + Send>,
    ),
    Box<dyn std::error::Error>,
>;
type PreparedResult<S> =
    Result<(HostedGraphNodeControl<RequestHost>, Box<Graph<S>>), Box<dyn std::error::Error>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParameterProfile {
    None,
    Input,
    InputOutput,
    Notes(HostedGraphNoteDialect),
    Gestures,
}
impl ParameterProfile {
    fn enabled(self) -> bool {
        self != Self::None
    }
    fn output(self) -> bool {
        matches!(self, Self::InputOutput | Self::Notes(_) | Self::Gestures)
    }
    fn gestures(self) -> bool {
        self == Self::Gestures
    }
    fn output_capacity(self) -> usize {
        if self.note().is_some() || self.gestures() {
            3
        } else {
            usize::from(self.output())
        }
    }
    fn note(self) -> Option<HostedGraphNoteDialect> {
        if let Self::Notes(dialect) = self {
            Some(dialect)
        } else {
            None
        }
    }
}
trait ProbeSample: GraphSample {
    const NAME: &'static str;
    fn value(value: f64) -> Self;
    fn as_f64(self) -> f64;
    fn input() -> [Self; 16];
    fn node(
        instance: PluginInstance<RequestHost>,
        options: HostedGraphNodeOptions,
        packed: bool,
        transport: bool,
        parameters: ParameterProfile,
    ) -> NodeResult<Self>;
}
macro_rules! probe_sample {
    ($sample:ty, $prepare:ident, $ports:ident, $params:ident, $build:ident, $input:expr) => {
        impl ProbeSample for $sample {
            const NAME: &'static str = stringify!($sample);
            fn value(value: f64) -> Self {
                value as Self
            }
            fn as_f64(self) -> f64 {
                self as f64
            }
            fn input() -> [Self; 16] {
                $input
            }
            fn node(
                instance: PluginInstance<RequestHost>,
                options: HostedGraphNodeOptions,
                packed: bool,
                transport: bool,
                parameters: ParameterProfile,
            ) -> NodeResult<Self> {
                let (control, mut node) = if parameters.output() {
                    let mut builder = HostedGraphNodeBuilder::new(options)
                        .with_parameter_gesture_output(parameters.gestures());
                    if let Some(dialect) = parameters.note() {
                        builder = builder
                            .with_note_input(HostedGraphNoteInput {
                                port_index: 1,
                                dialect,
                            })
                            .with_note_output(HostedGraphNoteOutput {
                                port_index: 1,
                                dialect,
                            });
                    }
                    builder
                        .with_parameter_input(vec![HostedGraphParameterMapping {
                            graph_id: ParameterId(9001),
                            native_id: ClapId::new(1),
                        }])
                        .with_parameter_output(vec![HostedGraphParameterMapping {
                            graph_id: ParameterId(9100),
                            native_id: ClapId::new(if parameters.gestures() { 1 } else { 2 }),
                        }])
                        .$build(instance, |_, _| ())?
                } else if parameters.enabled() {
                    $params(
                        instance,
                        options,
                        HostedGraphPortMode::MainOnly,
                        vec![HostedGraphParameterMapping {
                            graph_id: ParameterId(9001),
                            native_id: ClapId::new(1),
                        }],
                        |_, _| (),
                    )?
                } else if packed {
                    $ports(instance, options, |_, _| ())?
                } else {
                    $prepare(instance, options, |_, _| ())?
                };
                node.set_transport_enabled(transport)?;
                Ok((control, Box::new(node)))
            }
        }
    };
}
probe_sample!(
    f32,
    prepare_hosted_graph_node,
    prepare_hosted_graph_ports_node,
    prepare_hosted_parameter_graph_node,
    prepare,
    [0.25; 16]
);
probe_sample!(
    f64,
    prepare_hosted_graph_node64,
    prepare_hosted_graph_ports_node64,
    prepare_hosted_parameter_graph_node64,
    prepare64,
    [
        1. + 2f64.powi(-40),
        1e200,
        -1e200,
        1e-200,
        -1e-200,
        0.25,
        -0.5,
        0.,
        1. + 2f64.powi(-40),
        1e200,
        -1e200,
        1e-200,
        -1e-200,
        0.25,
        -0.5,
        0.,
    ]
);
fn prepared<S: ProbeSample>(
    library: &ClapLibrary,
    selected: &CStr,
    wet: S,
    sidechain: bool,
    transport: bool,
    parameters: ParameterProfile,
) -> PreparedResult<S> {
    let info = HostInfo::new(
        "RAVET explicit hosted graph validation",
        "RAVET",
        "",
        env!("CARGO_PKG_VERSION"),
    )?;
    let instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        library.entry(),
        selected,
        &info,
    )?;
    let layout = AudioLayout::new(if sidechain { 3 } else { 2 }, 2)?;
    let configuration =
        ProcessConfig::new(48000., 16, if parameters.enabled() { 4 } else { 0 }, layout)?
            .with_output_events(parameters.output_capacity());
    // Explicit known zero-latency/no-tail gain profile, not inferred from a
    // missing native extension or queried on a main-thread audio handle.
    let options = HostedGraphNodeOptions {
        configuration,
        declared_latency: 0,
        declared_tail: Tail::None,
    };
    let (control, node) = S::node(instance, options, sidechain, transport, parameters)?;
    let mut connections = Vec::new();
    for channel in 0..2 {
        connections.push(Connection {
            source: Source::Input(channel),
            destination: Destination::Node {
                id: NodeId(1),
                channel,
            },
            gain: S::value(1.),
        });
        connections.push(Connection {
            source: Source::Node {
                id: NodeId(1),
                channel,
            },
            destination: Destination::Output(channel),
            gain: wet,
        });
        connections.push(Connection {
            source: Source::Input(channel),
            destination: Destination::Output(channel),
            gain: S::value(0.25),
        });
    }
    if sidechain {
        connections.push(Connection {
            source: Source::Input(2),
            destination: Destination::Node {
                id: NodeId(1),
                channel: 2,
            },
            gain: S::value(1.),
        });
    }
    let mut graph = Box::new(Graph::new(
        layout,
        vec![GraphNode {
            id: NodeId(1),
            layout,
            processor: node,
        }],
        &connections,
    )?);
    if parameters.output() {
        graph.set_event_routes(&[
            EventConnection {
                source: EventEndpoint::Host,
                destination: EventEndpoint::Node(NodeId(1)),
                filter: if parameters.note().is_some() {
                    EventFilter::All
                } else {
                    EventFilter::Parameter(ParameterId(9001))
                },
            },
            EventConnection {
                source: EventEndpoint::Node(NodeId(1)),
                destination: EventEndpoint::Host,
                filter: if parameters.note().is_some() {
                    EventFilter::All
                } else {
                    EventFilter::Parameter(ParameterId(9100))
                },
            },
        ])?;
    }
    graph.prepare(configuration)?;
    Ok((control, graph))
}
fn gesture_output(frames: usize, value: f32) -> [Event; 3] {
    use ravet_foundation::ParameterGesturePhase;
    let last = frames.saturating_sub(1);
    [
        Event {
            frame_offset: 0,
            data: EventData::ParameterGesture {
                id: ParameterId(9100),
                phase: ParameterGesturePhase::Begin,
            },
        },
        Event {
            frame_offset: last,
            data: EventData::ParameterValue {
                id: ParameterId(9100),
                value,
            },
        },
        Event {
            frame_offset: last,
            data: EventData::ParameterGesture {
                id: ParameterId(9100),
                phase: ParameterGesturePhase::End,
            },
        },
    ]
}
fn render<S: ProbeSample>(
    session: &mut DynamicGraphAudioSession<'_, S>,
    expected: f64,
    first: GraphSwapStatus,
    sidechain: bool,
    wet: f64,
    transport: bool,
    parameters: ParameterProfile,
) {
    let detector =
        std::array::from_fn::<_, 16, _>(|frame| S::value(if frame % 2 == 0 { 0.05 } else { 0.2 }));
    let inputs = [S::input(), S::input(), detector];
    let mut output = [[S::value(f64::NAN); 16]; 2];
    let inputs_ref = inputs.each_ref().map(|channel| channel.as_slice());
    let input = &inputs_ref[..if sidechain { 3 } else { 2 }];
    let mut changes = [(0, 2.), (4, 3.), (4, 1.), (12, 0.)].map(|(frame_offset, value)| Event {
        frame_offset,
        data: EventData::ParameterValue {
            id: ParameterId(9001),
            value,
        },
    });
    if let Some(dialect) = parameters.note() {
        changes[0] = Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: ParameterId(9001),
                value: 3.,
            },
        };
        changes[1] = Event {
            frame_offset: 4,
            data: EventData::ParameterValue {
                id: ParameterId(9001),
                value: 4.,
            },
        };
        for (slot, on) in [(2, true), (3, false)] {
            changes[slot].data = if dialect == HostedGraphNoteDialect::Midi2 {
                EventData::Ump(
                    ravet_foundation::UmpPacket::from_words(&[
                        if on { 0x42903C00 } else { 0x42803C00 },
                        0x80000000,
                    ])
                    .unwrap(),
                )
            } else if on {
                EventData::NoteOn {
                    channel: 2,
                    key: 60,
                    velocity: 0.5,
                }
            } else {
                EventData::NoteOff {
                    channel: 2,
                    key: 60,
                    velocity: 0.5,
                }
            };
        }
    }
    let mut emitted = [Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: ParameterId(0),
            value: 0.,
        },
    }; 3];
    let counts = allocation_counter::measure(|| {
        for block in 0..1000 {
            let mut iter = output.iter_mut();
            let mut outputs = [
                iter.next().unwrap().as_mut_slice(),
                iter.next().unwrap().as_mut_slice(),
            ];
            let audio = AudioBuffer::separate(input, &mut outputs, 16).unwrap();
            let context = if transport {
                Transport {
                    playing: Some(block % 2 == 0),
                    sample_position: Some(-48000),
                    beat_position: Some(-0.25),
                    tempo: Some(123.5),
                    discontinuity: block == 500,
                    ..Transport::default()
                }
            } else {
                Transport::default()
            };
            let mut block_data = ProcessBlock::new(
                audio,
                if parameters.enabled() { &changes } else { &[] },
                context,
            );
            if parameters.output() {
                block_data.output_events = Some(ravet_foundation::OutputEvents::new(
                    &mut emitted[..parameters.output_capacity()],
                    16,
                ));
            }
            let status = session.process(&mut block_data);
            if parameters.output() {
                let events = block_data.output_events.as_ref().unwrap().as_slice();
                if let Some(dialect) = parameters.note() {
                    let expected: [Event; 2] = std::array::from_fn(|index| {
                        let event = changes[index + 2];
                        if matches!(
                            dialect,
                            HostedGraphNoteDialect::Midi | HostedGraphNoteDialect::MidiMpe
                        ) {
                            Event {
                                frame_offset: event.frame_offset,
                                data: EventData::Midi(
                                    ravet_foundation::MidiMessage::from_bytes(&[
                                        if index == 0 { 0x92 } else { 0x82 },
                                        60,
                                        64,
                                    ])
                                    .unwrap(),
                                ),
                            }
                        } else {
                            event
                        }
                    });
                    assert_eq!(&events[..2], &expected);
                }
                if parameters.gestures() {
                    assert_eq!(events, &gesture_output(16, 0.));
                } else {
                    assert_eq!(
                        &events[if parameters.note().is_some() { 2 } else { 0 }..],
                        &[Event {
                            frame_offset: 15,
                            data: EventData::ParameterValue {
                                id: ParameterId(9100),
                                value: 0.
                            }
                        }]
                    );
                }
            }
            assert_eq!(
                status.swap,
                if block == 0 {
                    first
                } else {
                    GraphSwapStatus::Unchanged
                }
            );
            assert_eq!(status.processing, GraphStatus::Continue);
            for (index, (actual, input)) in output
                .iter()
                .flatten()
                .zip(inputs[..2].iter().flatten())
                .enumerate()
            {
                let gain = if parameters.enabled() {
                    0.25 + wet
                        * if index % 16 < 4 {
                            if parameters.note().is_some() { 3. } else { 2. }
                        } else if index % 16 < 12 {
                            if parameters.note().is_some() { 2. } else { 1. }
                        } else {
                            0.
                        }
                } else if transport {
                    0.25 + if block % 2 == 0 { 2. * wet } else { 0. }
                } else if sidechain {
                    0.25 + if index % 2 == 0 { 0. } else { wet }
                } else {
                    expected
                };
                let expected = input.as_f64() * gain;
                let actual = actual.as_f64();
                assert!(actual.is_finite());
                assert!(
                    (actual - expected).abs()
                        <= expected.abs().max(1e-200) * if S::NAME == "f64" { 1e-14 } else { 1e-6 }
                );
            }
        }
        if parameters.enabled() {
            let inputs = [&[] as &[S]; 2];
            let mut a = [];
            let mut b = [];
            let mut outputs = [a.as_mut_slice(), b.as_mut_slice()];
            let audio = AudioBuffer::separate(&inputs, &mut outputs, 0).unwrap();
            let change = Event {
                frame_offset: 0,
                data: EventData::ParameterValue {
                    id: ParameterId(9001),
                    value: 4.,
                },
            };
            let changes = [change];
            let mut block_data = ProcessBlock::new(audio, &changes, Transport::default());
            if parameters.output() {
                block_data.output_events = Some(ravet_foundation::OutputEvents::new(
                    &mut emitted[..parameters.output_capacity()],
                    0,
                ));
            }
            let status = session.process(&mut block_data);
            if parameters.output() {
                if parameters.gestures() {
                    assert_eq!(
                        block_data.output_events.as_ref().unwrap().as_slice(),
                        &gesture_output(0, 4.)
                    );
                } else {
                    assert_eq!(
                        block_data.output_events.as_ref().unwrap().as_slice(),
                        &[Event {
                            frame_offset: 0,
                            data: EventData::ParameterValue {
                                id: ParameterId(9100),
                                value: 4.
                            }
                        }]
                    );
                }
            }
            assert_eq!(status.swap, GraphSwapStatus::Unchanged);
            assert_eq!(status.processing, GraphStatus::Continue);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    let f64 = args.first().is_some_and(|arg| arg == "--f64");
    if f64 {
        args.remove(0);
    }
    let sidechain = args.first().is_some_and(|arg| arg == "--sidechain");
    if sidechain {
        args.remove(0);
    }
    let transport = args.first().is_some_and(|arg| arg == "--transport");
    if transport {
        args.remove(0);
    }
    let parameters = match args.first().and_then(|arg| arg.to_str()) {
        Some("--parameters") => ParameterProfile::Input,
        Some("--parameter-io") => ParameterProfile::InputOutput,
        Some("--gestures") => ParameterProfile::Gestures,
        Some("--clap-notes") => ParameterProfile::Notes(HostedGraphNoteDialect::Clap),
        Some("--midi-notes") => ParameterProfile::Notes(HostedGraphNoteDialect::Midi),
        Some("--mpe-notes") => ParameterProfile::Notes(HostedGraphNoteDialect::MidiMpe),
        Some("--ump-notes") => ParameterProfile::Notes(HostedGraphNoteDialect::Midi2),
        _ => ParameterProfile::None,
    };
    if parameters.enabled() {
        args.remove(0);
    }
    if !(2..=3).contains(&args.len()) {
        return Err("usage: hosted-graph [--f64] [--sidechain] [--transport] [--parameters|--parameter-io|--gestures|--clap-notes|--midi-notes|--mpe-notes|--ump-notes] <trusted CLAP library> <zero-latency stereo gain ID> [expected default gain]".into());
    }
    let selected = CString::new(args[1].to_str().ok_or("ID must be UTF-8")?)?;
    if transport && (sidechain || selected.as_c_str() != c"org.ravet.host-graph-fixture") {
        return Err("--transport requires the public stereo graph fixture profile".into());
    }
    if parameters.enabled()
        && (sidechain
            || transport
            || selected.as_c_str()
                != if parameters.gestures() {
                    c"org.ravet.host-gesture-fixture"
                } else {
                    c"org.ravet.host-parameter-fixture"
                })
    {
        return Err("--parameters requires the public stepped parameter fixture profile".into());
    }
    let expected: f64 = args
        .get(2)
        .map(|s| s.to_str().ok_or("gain must be UTF-8"))
        .transpose()?
        .unwrap_or("1")
        .parse()?;
    if !expected.is_finite() || !(0. ..=4.).contains(&expected) {
        return Err("expected default gain must be finite 0..4".into());
    }
    let library = load(&PathBuf::from(&args[0]))?;
    if library.find(&selected).is_none() {
        return Err("selected plugin absent".into());
    }
    if f64 {
        run::<f64>(
            library, &selected, expected, sidechain, transport, parameters,
        )
    } else {
        run::<f32>(
            library, &selected, expected, sidechain, transport, parameters,
        )
    }
}
fn run<S: ProbeSample>(
    library: ClapLibrary,
    selected: &CStr,
    expected: f64,
    sidechain: bool,
    transport: bool,
    parameters: ParameterProfile,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut initial_control, initial) = prepared::<S>(
        &library,
        selected,
        S::value(1.),
        sidechain,
        transport,
        parameters,
    )?;
    let (mut next_control, next) = prepared::<S>(
        &library,
        selected,
        S::value(0.5),
        sidechain,
        transport,
        parameters,
    )?;
    drop(library);
    let (mut control, mut engine) = DynamicGraph::new(initial, 1).unwrap();
    let (command, receive) = std::sync::mpsc::sync_channel(0);
    let (results, wait) = std::sync::mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        {
            let mut session = engine.start_audio().unwrap();
            render(
                &mut session,
                expected + 0.25,
                GraphSwapStatus::Unchanged,
                sidechain,
                1.,
                transport,
                parameters,
            );
            results.send(()).unwrap();
            receive.recv().unwrap();
            render(
                &mut session,
                expected * 0.5 + 0.25,
                GraphSwapStatus::Applied,
                sidechain,
                0.5,
                transport,
                parameters,
            );
            results.send(()).unwrap();
            receive.recv().unwrap();
            session.finish();
        }
        engine
    });
    wait.recv()?;
    control.offer(next).unwrap();
    command.send(())?;
    wait.recv()?;
    assert_eq!(control.reclaim(), 1);
    assert!(initial_control.reclaim());
    assert!(!next_control.reclaim());
    command.send(())?;
    let engine = worker
        .join()
        .map_err(|_| "hosted graph audio owner panicked")?;
    drop(engine);
    assert!(next_control.reclaim());
    for owner in [&initial_control, &next_control] {
        let stats = owner.statistics();
        assert_eq!(stats.processed_blocks, 1000);
        assert_eq!(stats.processed_frames, 16000);
        assert_eq!(stats.parameter_flushes, u64::from(parameters.enabled()));
        assert_eq!(stats.backend_failures, 0);
        assert_eq!(stats.start_rejections, 0);
        assert_eq!(stats.phase_rejections, 0);
        assert!(owner.is_deactivated());
    }
    println!(
        "PASS 2000 stereo {} graph blocks/32000 frames, native wet/dry routing (sidechain={sidechain}, transport={transport}, parameters={parameters:?}), audio-owner swap/stop, zero measured allocations and main graph/instance retirement",
        S::NAME
    );
    Ok(())
}
