use clack_extensions::audio_ports::*;
use clack_host::prelude::*;
use clack_plugin::{entry::SinglePluginEntry, prelude::*};
use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus as Portable, Processor, Tail, Transport,
};
use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
use ravet_host::{HostRequests, HostedGraphNodeOptions, RequestHost, prepare_hosted_graph_node};
use std::{
    cell::RefCell,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc,
    },
    thread::ThreadId,
};
struct Observe {
    main: ThreadId,
    audio: OnceLock<ThreadId>,
    wrong: AtomicUsize,
    activated: AtomicUsize,
    started: AtomicUsize,
    stopped: AtomicUsize,
    reset: AtomicUsize,
    calls: AtomicUsize,
    deactivated: AtomicUsize,
    destroyed: AtomicUsize,
    requests: HostRequests,
    callbacks: AtomicUsize,
    supports64: AtomicBool,
    reject_aux64: AtomicBool,
    transport: AtomicBool,
    multi: AtomicBool,
    multi_output: AtomicBool,
    mode: AtomicU8,
}
impl Observe {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            main: std::thread::current().id(),
            audio: OnceLock::new(),
            wrong: AtomicUsize::new(0),
            activated: AtomicUsize::new(0),
            started: AtomicUsize::new(0),
            stopped: AtomicUsize::new(0),
            reset: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            deactivated: AtomicUsize::new(0),
            destroyed: AtomicUsize::new(0),
            requests: HostRequests::new(),
            callbacks: AtomicUsize::new(0),
            supports64: AtomicBool::new(false),
            reject_aux64: AtomicBool::new(false),
            transport: AtomicBool::new(false),
            multi: AtomicBool::new(false),
            multi_output: AtomicBool::new(false),
            mode: AtomicU8::new(0),
        })
    }
    fn owner(&self, audio: bool) {
        let expected = if audio {
            self.audio.get().copied()
        } else {
            Some(self.main)
        };
        if expected != Some(std::thread::current().id()) {
            self.wrong.fetch_add(1, Ordering::SeqCst);
        }
    }
}
thread_local! { static CONTEXT: RefCell<Option<Arc<Observe>>> = const { RefCell::new(None) }; }
struct Fixture;
struct Shared(Arc<Observe>);
struct Main(Arc<Observe>);
struct Audio {
    observe: Arc<Observe>,
    next: u64,
}
impl clack_plugin::plugin::PluginShared<'_> for Shared {}
impl Plugin for Fixture {
    type Shared<'a> = Shared;
    type MainThread<'a> = Main;
    type AudioProcessor<'a> = Audio;
    fn declare_extensions(builder: &mut PluginExtensions<Self>, _: Option<&Shared>) {
        builder.register::<PluginAudioPorts>();
    }
}
impl DefaultPluginFactory for Fixture {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new("org.ravet.graph-node-fixture", "Synthetic graph node")
    }
    fn new_shared(_: HostSharedHandle<'_>) -> Result<Shared, PluginError> {
        Ok(Shared(CONTEXT.with(|context| {
            context.borrow().as_ref().unwrap().clone()
        })))
    }
    fn new_main_thread(_: HostMainThreadHandle<'_>, shared: &Shared) -> Result<Main, PluginError> {
        shared.0.owner(false);
        Ok(Main(shared.0.clone()))
    }
}
impl PluginMainThread<'_, Shared> for Main {
    fn on_main_thread(&self) {
        self.0.owner(false);
        self.0.callbacks.fetch_add(1, Ordering::SeqCst);
    }
}
impl Drop for Main {
    fn drop(&mut self) {
        self.0.owner(false);
        self.0.destroyed.fetch_add(1, Ordering::SeqCst);
    }
}
impl PluginAudioPortsImpl for Main {
    fn count(&self, input: bool) -> u32 {
        self.0.owner(false);
        if (input && self.0.multi.load(Ordering::SeqCst))
            || (!input && self.0.multi_output.load(Ordering::SeqCst))
        {
            2
        } else {
            1
        }
    }
    fn get(&self, index: u32, _: bool, writer: &mut AudioPortInfoWriter) {
        self.0.owner(false);
        writer.set(&AudioPortInfo {
            id: ClapId::new(index),
            name: b"Synthetic stereo",
            flags: (if index == 0 {
                AudioPortFlags::IS_MAIN
            } else {
                AudioPortFlags::empty()
            }) | if self.0.supports64.load(Ordering::SeqCst)
                && (index == 0 || !self.0.reject_aux64.load(Ordering::SeqCst))
            {
                AudioPortFlags::SUPPORTS_64BITS
            } else {
                AudioPortFlags::empty()
            },
            channel_count: 2,
            port_type: Some(AudioPortType::STEREO),
            in_place_pair: None,
        });
    }
}
impl<'a> clack_plugin::plugin::PluginAudioProcessor<'a, Shared, Main> for Audio {
    fn activate(
        _: HostAudioProcessorHandle<'a>,
        main: &Main,
        _: &'a Shared,
        _: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        main.0.owner(false);
        main.0.activated.fetch_add(1, Ordering::SeqCst);
        Ok(Self {
            observe: main.0.clone(),
            next: 0,
        })
    }
    fn start_processing(&mut self) -> Result<(), PluginError> {
        self.observe.owner(true);
        self.observe.started.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn stop_processing(&mut self) {
        self.observe.owner(true);
        self.observe.stopped.fetch_add(1, Ordering::SeqCst);
    }
    fn reset(&mut self) {
        self.observe.owner(true);
        self.observe.reset.fetch_add(1, Ordering::SeqCst);
        self.next = 0;
    }
    fn deactivate(self, _: &Main) {
        self.observe.owner(false);
        self.observe.deactivated.fetch_add(1, Ordering::SeqCst);
    }
    fn process(
        &mut self,
        process: Process,
        mut audio: clack_plugin::process::Audio,
        events: clack_plugin::process::Events,
    ) -> Result<ProcessStatus, PluginError> {
        self.observe.owner(true);
        assert_eq!(process.steady_time, Some(self.next));
        if self.observe.transport.load(Ordering::SeqCst) {
            use clack_host::events::event_types::TransportFlags;
            let transport = process.transport.unwrap();
            assert!(transport.flags.contains(TransportFlags::IS_PLAYING));
            assert!(transport.flags.contains(TransportFlags::HAS_TEMPO));
            assert!(
                transport
                    .flags
                    .contains(TransportFlags::HAS_SECONDS_TIMELINE)
            );
            assert!(transport.flags.contains(TransportFlags::HAS_BEATS_TIMELINE));
            assert_eq!(transport.song_pos_seconds.to_float(), -1.);
            assert_eq!(transport.song_pos_beats.to_float(), -0.25);
            assert_eq!(transport.tempo, 123.5);
        } else {
            assert!(process.transport.is_none());
        }
        assert_eq!(events.input.len(), 0);
        self.next += u64::from(audio.frames_count());
        self.observe.calls.fetch_add(1, Ordering::SeqCst);
        let mode = self.observe.mode.load(Ordering::SeqCst);
        if mode == 3 {
            return Ok(ProcessStatus::Sleep);
        }
        use clack_plugin::process::audio::{ChannelPair, SampleType};
        macro_rules! write {
            ($channels:expr, $multiplier:expr, $sample:ty) => {
                for pair in $channels.iter_mut() {
                    let ChannelPair::InputOutput(input, output) = pair else {
                        panic!("separate prepared buffers required")
                    };
                    for (output, input) in output.iter_mut().zip(input) {
                        *output = if mode == 1 {
                            99.
                        } else {
                            *input * ($multiplier as $sample)
                        };
                    }
                }
            };
        }
        let count = if self.observe.multi_output.load(Ordering::SeqCst) {
            2
        } else {
            1
        };
        for index in 0..count {
            let multiplier = if index == 0 { 2. } else { 3. };
            match audio.port_pair(index).unwrap().channels().unwrap() {
                SampleType::F32(mut channels) => write!(channels, multiplier, f32),
                SampleType::F64(mut channels) => write!(channels, multiplier, f64),
                SampleType::Both(_, _) => panic!("one native precision required"),
            }
        }
        if mode == 1 {
            Err(PluginError::Message("synthetic native runtime failure"))
        } else {
            Ok(ProcessStatus::Continue)
        }
    }
}
fn instance(observe: &Arc<Observe>) -> PluginInstance<RequestHost> {
    CONTEXT.with(|context| *context.borrow_mut() = Some(observe.clone()));
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<Fixture>>(c"").unwrap();
    let info = HostInfo::new("Synthetic node host", "RAVET", "", "0.1").unwrap();
    PluginInstance::new(
        |_| observe.requests.clone(),
        |_| (),
        &entry,
        c"org.ravet.graph-node-fixture",
        &info,
    )
    .unwrap()
}
fn options(frames: usize, events: usize, layout: AudioLayout) -> HostedGraphNodeOptions {
    HostedGraphNodeOptions {
        configuration: ProcessConfig::new(48000., frames, events, layout).unwrap(),
        declared_latency: 0,
        declared_tail: Tail::None,
    }
}
#[test]
fn fixed_one_shot_node_rejects_main_audio_start_and_returns_matching_token() {
    let observe = Observe::new();
    let mut opts = options(8, 0, AudioLayout::STEREO);
    opts.declared_latency = 3;
    opts.declared_tail = Tail::Samples(17);
    let (control, mut node) =
        prepare_hosted_graph_node(instance(&observe), opts, |_, _| ()).unwrap();
    let mut control = control.into_instance().err().unwrap();
    assert!(matches!(
        control.editor(clack_extensions::gui::GuiConfiguration {
            api_type: clack_extensions::gui::GuiApiType::COCOA,
            is_floating: false,
        }),
        Err(ravet_host::EditorError::Unavailable)
    ));
    observe.requests.defer(ravet_host::PendingHostRequests {
        callback: true,
        restart: true,
        ..Default::default()
    });
    assert!(control.service_callback(&observe.requests));
    assert!(!control.service_callback(&observe.requests));
    assert!(observe.requests.take().restart);
    assert_eq!(observe.callbacks.load(Ordering::SeqCst), 1);
    assert_eq!(node.latency_samples(), 3);
    assert_eq!(node.tail(), Tail::Samples(17));
    assert!(!control.reclaim());
    assert_eq!(
        node.prepare(options(4, 0, AudioLayout::STEREO).configuration),
        Err(PrepareError::UnsupportedLayout)
    );
    node.prepare(opts.configuration).unwrap();
    assert_eq!(node.start_audio(), Err(ProcessError::BackendFailure));
    let inputs = [[0.25; 8], [-0.5; 8]];
    let mut outputs = [[99.; 8]; 2];
    let input = [inputs[0].as_slice(), inputs[1].as_slice()];
    let mut output: Vec<_> = outputs.iter_mut().map(|c| c.as_mut_slice()).collect();
    let audio = AudioBuffer::separate(&input, &mut output, 8).unwrap();
    assert_eq!(
        node.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
        Portable::Fault(ProcessError::NotPrepared)
    );
    assert!(outputs.iter().flatten().all(|x| *x == 0.));
    assert_eq!(control.statistics().phase_rejections, 1);
    assert_eq!(observe.started.load(Ordering::SeqCst), 0);
    node.deactivate();
    assert_eq!(
        node.prepare(opts.configuration),
        Err(PrepareError::UnsupportedLayout)
    );
    assert!(control.reclaim());
    assert!(control.is_deactivated());
    assert!(!control.reclaim());
    drop(node);
    let inactive = control.into_instance().ok().unwrap();
    assert!(!inactive.is_active());
    drop(inactive);
    assert_eq!(observe.activated.load(Ordering::SeqCst), 1);
    assert_eq!(observe.deactivated.load(Ordering::SeqCst), 1);
    assert_eq!(observe.destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
}
#[test]
fn unsupported_configuration_or_ports_retains_inactive_instance_before_activation() {
    for opts in [
        options(8, 1, AudioLayout::STEREO),
        options(65537, 0, AudioLayout::STEREO),
        options(8, 0, AudioLayout::MONO),
        HostedGraphNodeOptions {
            configuration: options(8, 0, AudioLayout::STEREO)
                .configuration
                .with_output_events(1),
            ..options(8, 0, AudioLayout::STEREO)
        },
        HostedGraphNodeOptions {
            configuration: options(8, 0, AudioLayout::STEREO)
                .configuration
                .with_auxiliary_buses(&[AudioLayout::MONO])
                .unwrap(),
            ..options(8, 0, AudioLayout::STEREO)
        },
    ] {
        let observe = Observe::new();
        let error = prepare_hosted_graph_node(instance(&observe), opts, |_, _| {
            panic!("invalid node invoked audio-handler factory")
        })
        .err()
        .unwrap();
        assert!(!error.instance.is_active());
        drop(error);
        assert_eq!(observe.activated.load(Ordering::SeqCst), 0);
        assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
    }
    let observe = Observe::new();
    observe.multi.store(true, Ordering::SeqCst);
    let error = prepare_hosted_graph_node(
        instance(&observe),
        options(8, 0, AudioLayout::STEREO),
        |_, _| panic!("unsupported ports invoked factory"),
    )
    .err()
    .unwrap();
    assert!(!error.instance.is_active());
    drop(error);
    assert_eq!(observe.activated.load(Ordering::SeqCst), 0);
    let observe = Observe::new();
    let mut active = instance(&observe);
    let stopped = active
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 48000.,
                min_frames_count: 1,
                max_frames_count: 8,
            },
        )
        .unwrap();
    let mut error =
        prepare_hosted_graph_node(active, options(8, 0, AudioLayout::STEREO), |_, _| {
            panic!("active instance invoked factory")
        })
        .err()
        .unwrap();
    assert!(error.instance.is_active());
    error.instance.deactivate(stopped);
}
fn render(
    session: &mut ravet_graph::GraphAudioSession<'_>,
    frames: usize,
    bad: bool,
    measure: bool,
) -> (GraphStatus, [[f32; 8]; 2]) {
    let inputs = [[0.25; 8], [-0.5; 8]];
    let mut outputs = [[99.; 8]; 2];
    let input = [inputs[0][..frames].as_ref(), inputs[1][..frames].as_ref()];
    let mut iter = outputs.iter_mut();
    let mut output = [
        iter.next().unwrap()[..frames].as_mut(),
        iter.next().unwrap()[..frames].as_mut(),
    ];
    let audio =
        AudioBuffer::separate(&input, &mut output[..if bad { 1 } else { 2 }], frames).unwrap();
    let mut status = None;
    if measure {
        let allocation = allocation_counter::measure(|| {
            status = Some(session.process(&mut ProcessBlock::new(
                audio,
                &[],
                Transport {
                    sample_position: Some(-100),
                    ..Transport::default()
                },
            )));
        });
        assert_eq!(allocation.count_total, 0);
        assert_eq!(allocation.count_current, 0);
    } else {
        status = Some(session.process(&mut ProcessBlock::new(audio, &[], Transport::default())));
    }
    (status.unwrap(), outputs)
}
#[test]
fn graph_session_maps_native_audio_fault_sleep_reset_and_main_retirement() {
    let observe = Observe::new();
    let opts = options(8, 0, AudioLayout::STEREO);
    let (mut control, node) =
        prepare_hosted_graph_node(instance(&observe), opts, |_, _| ()).unwrap();
    let mut routes = Vec::new();
    for channel in 0..2 {
        routes.push(Connection {
            source: Source::Input(channel),
            destination: Destination::Node {
                id: NodeId(1),
                channel,
            },
            gain: 1.,
        });
        routes.push(Connection {
            source: Source::Node {
                id: NodeId(1),
                channel,
            },
            destination: Destination::Output(channel),
            gain: 1.,
        });
    }
    let mut graph = Graph::new(
        AudioLayout::STEREO,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::STEREO,
            processor: Box::new(node),
        }],
        &routes,
    )
    .unwrap();
    graph.prepare(opts.configuration).unwrap();
    assert!(matches!(
        graph.start_audio(),
        Err(GraphStatus::Fault {
            error: ProcessError::BackendFailure,
            ..
        })
    ));
    graph.reset();
    let worker_observe = observe.clone();
    let (sleeping, wait) = mpsc::sync_channel(0);
    let (resume, receive) = mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        worker_observe
            .audio
            .set(std::thread::current().id())
            .unwrap();
        {
            let mut session = graph.start_audio().unwrap();
            for _ in 0..1000 {
                assert_eq!(
                    render(&mut session, 8, false, true),
                    (GraphStatus::Continue, [[0.5; 8], [-1.; 8]])
                );
            }
            assert_eq!(
                render(&mut session, 0, false, true).0,
                GraphStatus::Continue
            );
            assert!(matches!(
                render(&mut session, 8, true, true).0,
                GraphStatus::Fault {
                    node: None,
                    error: ProcessError::LayoutMismatch
                }
            ));
            worker_observe.mode.store(1, Ordering::SeqCst);
            let (status, audio) = render(&mut session, 8, false, false);
            assert_eq!(
                status,
                GraphStatus::Fault {
                    node: Some(NodeId(1)),
                    error: ProcessError::BackendFailure
                }
            );
            assert_eq!(audio, [[0.; 8]; 2]);
            session.reset();
            worker_observe.mode.store(3, Ordering::SeqCst);
            assert_eq!(
                render(&mut session, 8, false, true),
                (GraphStatus::Continue, [[0.; 8]; 2])
            );
            sleeping.send(()).unwrap();
            receive.recv().unwrap();
            worker_observe.mode.store(0, Ordering::SeqCst);
            assert_eq!(
                render(&mut session, 8, false, true),
                (GraphStatus::Continue, [[0.5; 8], [-1.; 8]])
            );
            session.finish();
        }
        graph
    });
    wait.recv().unwrap();
    let snapshot = control.statistics();
    assert_eq!(snapshot.last_status, Some(ProcessStatus::Sleep));
    assert_eq!(snapshot.backend_failures, 1);
    assert_eq!(snapshot.processed_blocks, 1001);
    assert_eq!(snapshot.processed_frames, 8008);
    resume.send(()).unwrap();
    let graph = worker.join().unwrap();
    assert!(!control.reclaim());
    drop(graph);
    assert!(control.reclaim());
    let snapshot = control.statistics();
    assert_eq!(snapshot.last_status, Some(ProcessStatus::Continue));
    assert_eq!(snapshot.processed_blocks, 1002);
    assert_eq!(snapshot.processed_frames, 8016);
    assert_eq!(snapshot.phase_rejections, 1);
    drop(control);
    assert_eq!(observe.calls.load(Ordering::SeqCst), 1003);
    assert_eq!(observe.started.load(Ordering::SeqCst), 1);
    assert_eq!(observe.stopped.load(Ordering::SeqCst), 1);
    assert_eq!(observe.reset.load(Ordering::SeqCst), 2);
    assert_eq!(observe.deactivated.load(Ordering::SeqCst), 1);
    assert_eq!(observe.destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
}

fn routed(
    observe: &Arc<Observe>,
    wet: f32,
) -> (ravet_host::HostedGraphNodeControl<RequestHost>, Box<Graph>) {
    let opts = options(8, 0, AudioLayout::STEREO);
    let (control, node) = prepare_hosted_graph_node(instance(observe), opts, |_, _| ()).unwrap();
    let mut routes = Vec::new();
    for channel in 0..2 {
        routes.push(Connection {
            source: Source::Input(channel),
            destination: Destination::Node {
                id: NodeId(1),
                channel,
            },
            gain: 1.,
        });
        routes.push(Connection {
            source: Source::Node {
                id: NodeId(1),
                channel,
            },
            destination: Destination::Output(channel),
            gain: wet,
        });
        routes.push(Connection {
            source: Source::Input(channel),
            destination: Destination::Output(channel),
            gain: 0.25,
        });
    }
    let mut graph = Box::new(
        Graph::new(
            AudioLayout::STEREO,
            vec![GraphNode {
                id: NodeId(1),
                layout: AudioLayout::STEREO,
                processor: Box::new(node),
            }],
            &routes,
        )
        .unwrap(),
    );
    graph.prepare(opts.configuration).unwrap();
    (control, graph)
}
fn dynamic_render(
    session: &mut ravet_graph::DynamicGraphAudioSession<'_>,
) -> (ravet_graph::GraphSwapStatus, [[f32; 8]; 2]) {
    let inputs = [[0.25; 8], [-0.5; 8]];
    let mut outputs = [[99.; 8]; 2];
    let input = [inputs[0].as_slice(), inputs[1].as_slice()];
    let mut iter = outputs.iter_mut();
    let mut output = [
        iter.next().unwrap().as_mut_slice(),
        iter.next().unwrap().as_mut_slice(),
    ];
    let audio = AudioBuffer::separate(&input, &mut output, 8).unwrap();
    let mut status = None;
    let allocation = allocation_counter::measure(|| {
        status = Some(session.process(&mut ProcessBlock::new(audio, &[], Transport::default())));
    });
    assert_eq!(allocation.count_total, 0);
    assert_eq!(allocation.count_current, 0);
    let status = status.unwrap();
    assert_eq!(status.processing, GraphStatus::Continue);
    (status.swap, outputs)
}
#[test]
fn dynamic_native_node_stops_on_audio_before_control_graph_and_instance_retirement() {
    let initial_observe = Observe::new();
    let next_observe = Observe::new();
    let (mut initial_control, initial) = routed(&initial_observe, 1.);
    let (mut next_control, next) = routed(&next_observe, 0.5);
    let (mut control, mut engine) = ravet_graph::DynamicGraph::new(initial, 1).unwrap();
    let (command, receive) = mpsc::sync_channel(0);
    let (results, wait) = mpsc::sync_channel(0);
    let a = initial_observe.clone();
    let b = next_observe.clone();
    let worker = std::thread::spawn(move || {
        a.audio.set(std::thread::current().id()).unwrap();
        b.audio.set(std::thread::current().id()).unwrap();
        {
            let mut session = engine.start_audio().unwrap();
            results.send(dynamic_render(&mut session)).unwrap();
            receive.recv().unwrap();
            let (swap, output) = dynamic_render(&mut session);
            assert_eq!(swap, ravet_graph::GraphSwapStatus::Applied);
            assert_eq!(output, [[0.3125; 8], [-0.625; 8]]);
            for _ in 0..999 {
                assert_eq!(
                    dynamic_render(&mut session),
                    (
                        ravet_graph::GraphSwapStatus::Unchanged,
                        [[0.3125; 8], [-0.625; 8]]
                    )
                );
            }
            results.send((swap, output)).unwrap();
            receive.recv().unwrap();
            session.finish();
        }
        engine
    });
    assert_eq!(
        wait.recv().unwrap(),
        (
            ravet_graph::GraphSwapStatus::Unchanged,
            [[0.5625; 8], [-1.125; 8]]
        )
    );
    control.offer(next).unwrap();
    command.send(()).unwrap();
    wait.recv().unwrap();
    assert_eq!(initial_observe.stopped.load(Ordering::SeqCst), 1);
    assert_eq!(initial_observe.deactivated.load(Ordering::SeqCst), 0);
    assert_eq!(control.reclaim(), 1);
    assert!(initial_control.reclaim());
    assert!(!next_control.reclaim());
    command.send(()).unwrap();
    let engine = worker.join().unwrap();
    drop(engine);
    assert!(next_control.reclaim());
    drop(control);
    drop(initial_control);
    drop(next_control);
    for observe in [&initial_observe, &next_observe] {
        assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
        assert_eq!(observe.started.load(Ordering::SeqCst), 1);
        assert_eq!(observe.stopped.load(Ordering::SeqCst), 1);
        assert_eq!(observe.deactivated.load(Ordering::SeqCst), 1);
        assert_eq!(observe.destroyed.load(Ordering::SeqCst), 1);
    }
    assert_eq!(initial_observe.calls.load(Ordering::SeqCst), 1);
    assert_eq!(next_observe.calls.load(Ordering::SeqCst), 1000);
}

#[test]
fn f64_node_rejects_unadvertised_precision_then_preserves_wide_routed_values() {
    let unsupported = Observe::new();
    let error = ravet_host::prepare_hosted_graph_node64(
        instance(&unsupported),
        options(8, 0, AudioLayout::STEREO),
        |_, _| panic!("unsupported precision invoked factory"),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        ravet_host::HostedGraphNodeBuildFailure::Precision
    ));
    assert!(!error.instance.is_active());
    drop(error);
    assert_eq!(unsupported.activated.load(Ordering::SeqCst), 0);
    let observe = Observe::new();
    observe.supports64.store(true, Ordering::SeqCst);
    let opts = options(8, 0, AudioLayout::STEREO);
    let (mut control, node) =
        ravet_host::prepare_hosted_graph_node64(instance(&observe), opts, |_, _| ()).unwrap();
    let mut routes = Vec::new();
    for channel in 0..2 {
        routes.push(Connection {
            source: Source::Input(channel),
            destination: Destination::Node {
                id: NodeId(1),
                channel,
            },
            gain: 1_f64,
        });
        routes.push(Connection {
            source: Source::Node {
                id: NodeId(1),
                channel,
            },
            destination: Destination::Output(channel),
            gain: 0.5,
        });
    }
    let mut graph = Graph::<f64>::new(
        AudioLayout::STEREO,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::STEREO,
            processor: Box::new(node),
        }],
        &routes,
    )
    .unwrap();
    graph.prepare(opts.configuration).unwrap();
    let a = observe.clone();
    let graph = std::thread::spawn(move || {
        a.audio.set(std::thread::current().id()).unwrap();
        {
            let mut session = graph.start_audio().unwrap();
            let input = [
                1. + 2_f64.powi(-40),
                1e200,
                -1e200,
                1e-200,
                -1e-200,
                0.25,
                -0.5,
                0.,
            ];
            let inputs = [input.as_slice(), input.as_slice()];
            let mut output = [[0_f64; 8]; 2];
            let allocation = allocation_counter::measure(|| {
                for _ in 0..1000 {
                    let mut iter = output.iter_mut();
                    let mut outputs = [
                        iter.next().unwrap().as_mut_slice(),
                        iter.next().unwrap().as_mut_slice(),
                    ];
                    let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                    assert_eq!(
                        session.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        GraphStatus::Continue
                    );
                    assert_eq!(output, [input; 2]);
                }
            });
            assert_eq!(allocation.count_total, 0);
            assert_eq!(allocation.count_current, 0);
            session.finish();
        }
        graph
    })
    .join()
    .unwrap();
    drop(graph);
    assert!(control.reclaim());
    assert_eq!(control.statistics().processed_frames, 8000);
    drop(control);
    assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
    assert_eq!(observe.started.load(Ordering::SeqCst), 1);
    assert_eq!(observe.stopped.load(Ordering::SeqCst), 1);
    assert_eq!(observe.deactivated.load(Ordering::SeqCst), 1);
    assert_eq!(observe.destroyed.load(Ordering::SeqCst), 1);
}

macro_rules! packed_port_test {
    ($test:ident, $sample:ty, $prepare:ident, $inputs:expr) => {
        #[test]
        fn $test() {
            let observe = Observe::new();
            observe.multi.store(true, Ordering::SeqCst);
            observe.multi_output.store(true, Ordering::SeqCst);
            observe.supports64.store(true, Ordering::SeqCst);
            let mismatch = options(8, 0, AudioLayout::STEREO);
            let error = ravet_host::$prepare(instance(&observe), mismatch, |_, _| {
                panic!("before activation")
            })
            .err()
            .unwrap();
            assert!(matches!(
                error.reason,
                ravet_host::HostedGraphNodeBuildFailure::Configuration
            ));
            drop(error);
            assert_eq!(observe.activated.load(Ordering::SeqCst), 0);
            let layout = AudioLayout::new(4, 4).unwrap();
            let opts = options(8, 0, layout);
            let (mut control, node) =
                ravet_host::$prepare(instance(&observe), opts, |_, _| ()).unwrap();
            let mut routes = Vec::new();
            for channel in 0..4 {
                routes.push(Connection {
                    source: Source::Input(channel),
                    destination: Destination::Node {
                        id: NodeId(1),
                        channel,
                    },
                    gain: 1. as $sample,
                });
                routes.push(Connection {
                    source: Source::Node {
                        id: NodeId(1),
                        channel,
                    },
                    destination: Destination::Output(channel),
                    gain: 1. as $sample,
                });
            }
            let mut graph = Graph::<$sample>::new(
                layout,
                vec![GraphNode {
                    id: NodeId(1),
                    layout,
                    processor: Box::new(node),
                }],
                &routes,
            )
            .unwrap();
            graph.prepare(opts.configuration).unwrap();
            let a = observe.clone();
            let graph = std::thread::spawn(move || {
                a.audio.set(std::thread::current().id()).unwrap();
                {
                    let mut session = graph.start_audio().unwrap();
                    let input: [[$sample; 8]; 4] = $inputs;
                    let inputs = input.each_ref().map(|channel| channel.as_slice());
                    let mut output = [[0. as $sample; 8]; 4];
                    let allocation = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            let mut outputs =
                                output.each_mut().map(|channel| channel.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            assert_eq!(
                                session.process(&mut ProcessBlock::new(
                                    audio,
                                    &[],
                                    Transport::default()
                                )),
                                GraphStatus::Continue
                            );
                            for channel in 0..4 {
                                let multiplier = if channel < 2 { 2. } else { 3. };
                                for frame in 0..8 {
                                    assert_eq!(
                                        output[channel][frame],
                                        input[channel][frame] * multiplier
                                    );
                                }
                            }
                        }
                    });
                    assert_eq!(allocation.count_total, 0);
                    assert_eq!(allocation.count_current, 0);
                    session.finish();
                }
                graph
            })
            .join()
            .unwrap();
            drop(graph);
            assert!(control.reclaim());
            assert_eq!(control.statistics().processed_frames, 8000);
            drop(control);
            assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
            assert_eq!(observe.started.load(Ordering::SeqCst), 1);
            assert_eq!(observe.stopped.load(Ordering::SeqCst), 1);
            assert_eq!(observe.deactivated.load(Ordering::SeqCst), 1);
            assert_eq!(observe.destroyed.load(Ordering::SeqCst), 2);
        }
    };
}
packed_port_test!(
    packed_f32_ports_preserve_native_groups_channels_and_owner_retirement,
    f32,
    prepare_hosted_graph_ports_node,
    [[0.25; 8], [-0.5; 8], [0.125; 8], [-0.0625; 8]]
);
packed_port_test!(
    packed_f64_ports_preserve_native_groups_wide_channels_and_owner_retirement,
    f64,
    prepare_hosted_graph_ports_node64,
    [
        [1. + 2_f64.powi(-40); 8],
        [1e200; 8],
        [1e-200; 8],
        [-1e200; 8]
    ]
);

#[test]
fn packed_f64_checks_auxiliary_precision_before_activation_and_retains_instance() {
    let observe = Observe::new();
    observe.multi.store(true, Ordering::SeqCst);
    observe.supports64.store(true, Ordering::SeqCst);
    observe.reject_aux64.store(true, Ordering::SeqCst);
    let layout = AudioLayout::new(4, 2).unwrap();
    let error = ravet_host::prepare_hosted_graph_ports_node64(
        instance(&observe),
        options(8, 0, layout),
        |_, _| panic!("unsupported auxiliary before activation"),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        ravet_host::HostedGraphNodeBuildFailure::Precision
    ));
    assert!(!error.instance.is_active());
    drop(error);
    assert_eq!(observe.activated.load(Ordering::SeqCst), 0);
    assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
    assert_eq!(observe.destroyed.load(Ordering::SeqCst), 1);
}

#[test]
fn opted_in_native_transport_rejects_before_calls_and_keeps_independent_steady_time() {
    let observe = Observe::new();
    observe.transport.store(true, Ordering::SeqCst);
    let opts = options(8, 0, AudioLayout::STEREO);
    let (mut control, mut node) =
        prepare_hosted_graph_node(instance(&observe), opts, |_, _| ()).unwrap();
    node.set_transport_enabled(true).unwrap();
    node.prepare(opts.configuration).unwrap();
    let a = observe.clone();
    let mut node = std::thread::spawn(move || {
        a.audio.set(std::thread::current().id()).unwrap();
        assert_eq!(
            node.set_transport_enabled(false),
            Err(PrepareError::UnsupportedLayout)
        );
        node.start_audio().unwrap();
        let context = Transport {
            playing: Some(true),
            sample_position: Some(-48000),
            beat_position: Some(-0.25),
            tempo: Some(123.5),
            discontinuity: true,
            ..Transport::default()
        };
        let input = [0.25; 8];
        let inputs = [input.as_slice(), input.as_slice()];
        let mut output = [[9.; 8]; 2];
        let counts = allocation_counter::measure(|| {
            for _ in 0..1000 {
                for context in [
                    Transport {
                        playing: None,
                        ..context
                    },
                    Transport {
                        tempo: Some(f64::NAN),
                        ..context
                    },
                    Transport {
                        sample_position: Some(i64::MAX),
                        ..context
                    },
                ] {
                    let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
                    let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                    assert_eq!(
                        node.process(&mut ProcessBlock::new(audio, &[], context)),
                        Portable::Fault(ProcessError::InvalidTransport)
                    );
                    assert_eq!(output, [[0.; 8]; 2]);
                }
                let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
                let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                assert_eq!(
                    node.process(&mut ProcessBlock::new(audio, &[], context)),
                    Portable::Continue
                );
                assert_eq!(output, [[0.5; 8]; 2]);
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
        node.stop_audio();
        node
    })
    .join()
    .unwrap();
    node.deactivate();
    assert!(control.reclaim());
    assert_eq!(control.statistics().processed_blocks, 1000);
    assert_eq!(control.statistics().backend_failures, 0);
    assert_eq!(observe.calls.load(Ordering::SeqCst), 1000);
    assert_eq!(observe.reset.load(Ordering::SeqCst), 0);
    assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
    drop(node);
    drop(control);
    assert_eq!(observe.destroyed.load(Ordering::SeqCst), 1);
}

#[test]
fn f64_builder_transport_and_packed_profile_reach_owner_callbacks() {
    let observe = Observe::new();
    observe.supports64.store(true, Ordering::SeqCst);
    observe.transport.store(true, Ordering::SeqCst);
    let opts = options(8, 0, AudioLayout::STEREO);
    let (mut control, mut node) = ravet_host::HostedGraphNodeBuilder::new(opts)
        .with_port_mode(ravet_host::HostedGraphPortMode::Packed)
        .with_transport_enabled(true)
        .prepare64(instance(&observe), |_, _| ())
        .unwrap();
    node.prepare(opts.configuration).unwrap();
    let a = observe.clone();
    let mut node = std::thread::spawn(move || {
        a.audio.set(std::thread::current().id()).unwrap();
        node.start_audio().unwrap();
        let input = [1e200; 8];
        let inputs = [input.as_slice(), input.as_slice()];
        let mut output = [[0_f64; 8]; 2];
        let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
        let context = Transport {
            playing: Some(true),
            sample_position: Some(-48000),
            beat_position: Some(-0.25),
            tempo: Some(123.5),
            discontinuity: false,
            ..Transport::default()
        };
        assert_eq!(
            node.process(&mut ProcessBlock::new(audio, &[], context)),
            Portable::Continue
        );
        assert_eq!(output, [[2e200; 8]; 2]);
        node.stop_audio();
        node
    })
    .join()
    .unwrap();
    node.deactivate();
    assert!(control.reclaim());
    drop(node);
    drop(control);
    assert_eq!(observe.wrong.load(Ordering::SeqCst), 0);
    assert_eq!(observe.calls.load(Ordering::SeqCst), 1);
}
