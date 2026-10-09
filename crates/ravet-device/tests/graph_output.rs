use ravet_audio_file::audio_ring;
use ravet_device::{PreparedGraphOutput, RingOutput};
use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Transport,
};
use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicUsize, Ordering},
};
struct Observe {
    main: std::thread::ThreadId,
    owner: OnceLock<std::thread::ThreadId>,
    calls: AtomicUsize,
    prepared: AtomicUsize,
    stopped: AtomicUsize,
    deactivated: AtomicUsize,
}
struct Ramp {
    observe: Arc<Observe>,
    position: usize,
    fail: bool,
}
impl Processor for Ramp {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        self.observe.prepared.fetch_add(1, Ordering::Relaxed);
        assert_eq!(std::thread::current().id(), self.observe.main);
        Ok(())
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        self.observe.owner.set(std::thread::current().id()).unwrap();
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        assert_eq!(Some(&std::thread::current().id()), self.observe.owner.get());
        self.observe.calls.fetch_add(1, Ordering::Relaxed);
        if self.fail {
            return ProcessStatus::Fault(ProcessError::BackendFailure);
        }
        for channel in 0..2 {
            for (frame, sample) in block
                .audio
                .output_mut(channel)
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                *sample = (self.position + frame) as f32 + channel as f32 * 10000.;
            }
        }
        self.position += block.audio.frames();
        ProcessStatus::Continue
    }
    fn stop_audio(&mut self) {
        assert_eq!(Some(&std::thread::current().id()), self.observe.owner.get());
        self.observe.stopped.fetch_add(1, Ordering::Relaxed);
    }
    fn reset(&mut self) {
        self.fail = false;
    }
    fn deactivate(&mut self) {
        assert_eq!(std::thread::current().id(), self.observe.main);
        self.observe.deactivated.fetch_add(1, Ordering::Relaxed);
    }
}
fn graph(fail: bool) -> (Arc<Observe>, Box<Graph>) {
    let observe = Arc::new(Observe {
        main: std::thread::current().id(),
        owner: OnceLock::new(),
        calls: AtomicUsize::new(0),
        prepared: AtomicUsize::new(0),
        stopped: AtomicUsize::new(0),
        deactivated: AtomicUsize::new(0),
    });
    let graph = Graph::new(
        AudioLayout::STEREO_INSTRUMENT,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::STEREO_INSTRUMENT,
            processor: Box::new(Ramp {
                observe: observe.clone(),
                position: 0,
                fail,
            }),
        }],
        &(0..2)
            .map(|channel| Connection {
                source: Source::Node {
                    id: NodeId(1),
                    channel,
                },
                destination: Destination::Output(channel),
                gain: 1.,
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    (observe, Box::new(graph))
}
fn config(frames: usize) -> ProcessConfig {
    ProcessConfig::new(48000., frames, 0, AudioLayout::STEREO_INSTRUMENT).unwrap()
}
#[test]
fn worker_retains_exact_suffix_before_rerender_and_returns_stopped_graph_to_main() {
    let (observe, graph) = graph(false);
    let (producer, mut consumer) = audio_ring(2, 4).unwrap();
    let mut prepared = PreparedGraphOutput::new(graph, producer, config(8)).unwrap();
    let (prepared, consumer) = std::thread::spawn(move || {
        {
            let mut session = prepared.start_audio().unwrap();
            let first = session.pump(Transport::default()).unwrap();
            assert_eq!(
                (
                    first.rendered_frames,
                    first.published_frames,
                    first.pending_frames
                ),
                (8, 4, 4)
            );
            let full = session.pump(Transport::default()).unwrap();
            assert_eq!(
                (
                    full.rendered_frames,
                    full.published_frames,
                    full.pending_frames
                ),
                (0, 0, 4)
            );
            let mut samples = [0.; 6];
            assert_eq!(consumer.pop(&mut samples).unwrap(), 3);
            assert_eq!(samples, [0., 10000., 1., 10001., 2., 10002.]);
            let progress = session.pump(Transport::default()).unwrap();
            assert_eq!(
                (
                    progress.rendered_frames,
                    progress.published_frames,
                    progress.pending_frames
                ),
                (0, 3, 1)
            );
            let mut samples = [0.; 8];
            assert_eq!(consumer.pop(&mut samples).unwrap(), 4);
            assert_eq!(samples, [3., 10003., 4., 10004., 5., 10005., 6., 10006.]);
            let progress = session.pump(Transport::default()).unwrap();
            assert_eq!(
                (
                    progress.rendered_frames,
                    progress.published_frames,
                    progress.pending_frames
                ),
                (0, 1, 0)
            );
            let mut samples = [-1.; 8];
            assert_eq!(consumer.pop(&mut samples).unwrap(), 1);
            assert_eq!(&samples[..2], &[7., 10007.]);
            assert_eq!(&samples[2..], &[-1.; 6]);
            session.finish();
        }
        (prepared, consumer)
    })
    .join()
    .unwrap();
    assert_eq!(observe.calls.load(Ordering::Relaxed), 1);
    assert_eq!(observe.stopped.load(Ordering::Relaxed), 1);
    // Graph preparation first deactivates its fresh portable nodes on main.
    assert_eq!(observe.deactivated.load(Ordering::Relaxed), 1);
    drop(prepared.into_graph());
    drop(consumer);
    assert_eq!(observe.deactivated.load(Ordering::Relaxed), 2);
}
#[test]
fn worker_render_queue_and_device_consumer_preserve_order_without_heap_actions() {
    let (observe, graph) = graph(false);
    let (producer, consumer) = audio_ring(2, 8).unwrap();
    let mut prepared = PreparedGraphOutput::new(graph, producer, config(8)).unwrap();
    let mut playback = RingOutput::new(consumer, 48000.).unwrap();
    playback.prepare(config(8)).unwrap();
    let (prepared, playback) = std::thread::spawn(move || {
        {
            let mut session = prepared.start_audio().unwrap();
            let mut output = [[0.; 8]; 2];
            let counts = allocation_counter::measure(|| {
                for index in 0..1000 {
                    let progress = session.pump(Transport::default()).unwrap();
                    assert_eq!(
                        (
                            progress.rendered_frames,
                            progress.published_frames,
                            progress.pending_frames
                        ),
                        (8, 8, 0)
                    );
                    let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
                    let audio = AudioBuffer::separate(&[], &mut outputs, 8).unwrap();
                    assert_eq!(
                        playback.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        ProcessStatus::Continue
                    );
                    for (channel, samples) in output.iter().enumerate() {
                        for (frame, sample) in samples.iter().enumerate() {
                            assert_eq!(
                                *sample,
                                (index * 8 + frame) as f32 + channel as f32 * 10000.
                            );
                        }
                    }
                }
            });
            assert_eq!(counts.count_total, 0);
            assert_eq!(counts.count_current, 0);
        }
        (prepared, playback)
    })
    .join()
    .unwrap();
    assert_eq!(playback.statistics().source_frames, 8000);
    assert_eq!(playback.statistics().underflow_frames, 0);
    drop(prepared.into_graph());
    drop(playback);
    assert_eq!(observe.stopped.load(Ordering::Relaxed), 1);
}
#[test]
fn graph_fault_publishes_nothing_and_reset_is_explicit() {
    let (observe, graph) = graph(true);
    let (producer, mut consumer) = audio_ring(2, 8).unwrap();
    let mut prepared = PreparedGraphOutput::new(graph, producer, config(8)).unwrap();
    let prepared = std::thread::spawn(move || {
        {
            let mut session = prepared.start_audio().unwrap();
            assert_eq!(
                session.pump(Transport::default()),
                Err(GraphStatus::Fault {
                    node: Some(NodeId(1)),
                    error: ProcessError::BackendFailure,
                })
            );
            assert_eq!(consumer.queued_frames(), 0);
            session.reset();
            assert_eq!(
                session.pump(Transport::default()).unwrap().published_frames,
                8
            );
            let mut samples = [0.; 16];
            assert_eq!(consumer.pop(&mut samples).unwrap(), 8);
        }
        prepared
    })
    .join()
    .unwrap();
    drop(prepared.into_graph());
    assert_eq!(observe.calls.load(Ordering::Relaxed), 2);
}

#[test]
fn queue_playback_silences_underflow_and_invalid_blocks_preserve_queued_audio() {
    let (mut producer, consumer) = audio_ring(2, 8).unwrap();
    producer.push(&[1., 10., 2., 20.]).unwrap();
    let mut playback = RingOutput::new(consumer, 48000.).unwrap();
    playback.prepare(config(8)).unwrap();
    let mut output = [[9.; 3]; 2];
    let input = [0.; 3];
    let inputs = [input.as_slice()];
    {
        let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
        let audio = AudioBuffer::separate(&inputs, &mut outputs, 3).unwrap();
        assert!(matches!(
            playback.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
            ProcessStatus::Fault(_)
        ));
    }
    assert_eq!(output, [[0.; 3]; 2]);
    let counts = allocation_counter::measure(|| {
        let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
        let audio = AudioBuffer::separate(&[], &mut outputs, 3).unwrap();
        assert_eq!(
            playback.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
            ProcessStatus::Continue
        );
    });
    assert_eq!(output, [[1., 2., 0.], [10., 20., 0.]]);
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(playback.statistics().source_frames, 2);
    assert_eq!(playback.statistics().underflow_frames, 1);
    let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
    let audio = AudioBuffer::separate(&[], &mut outputs, 3).unwrap();
    assert_eq!(
        playback.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
        ProcessStatus::Silence
    );
    assert_eq!(output, [[0.; 3]; 2]);
    assert_eq!(playback.statistics().underflow_frames, 4);
    assert_eq!(
        playback.prepare(ProcessConfig::new(44100., 8, 0, AudioLayout::STEREO_INSTRUMENT).unwrap()),
        Err(PrepareError::UnsupportedLayout)
    );
    let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
    let audio = AudioBuffer::separate(&[], &mut outputs, 3).unwrap();
    assert_eq!(
        playback.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
        ProcessStatus::Fault(ProcessError::NotPrepared)
    );
}

#[test]
fn preparation_rejects_rate_layout_and_event_contracts_before_worker_start() {
    for config in [
        ProcessConfig::new(48000., 8, 0, AudioLayout::MONO_INSTRUMENT).unwrap(),
        ProcessConfig::new(48000., 8, 4097, AudioLayout::STEREO_INSTRUMENT).unwrap(),
        config(8).with_output_events(4097),
        config(8)
            .with_auxiliary_buses(&[AudioLayout::MONO])
            .unwrap(),
        config(65537),
    ] {
        let (observe, graph) = graph(false);
        let (producer, _) = audio_ring(2, 8).unwrap();
        assert!(matches!(
            PreparedGraphOutput::new(graph, producer, config),
            Err(ravet_device::GraphOutputError::Configuration)
        ));
        assert!(observe.owner.get().is_none());
        assert_eq!(observe.calls.load(Ordering::Relaxed), 0);
    }
    let (_, consumer) = audio_ring(2, 8).unwrap();
    assert!(matches!(
        RingOutput::new(consumer, f64::NAN),
        Err(PrepareError::InvalidSampleRate)
    ));
    let (_, consumer) = audio_ring(2, 8).unwrap();
    let mut playback = RingOutput::new(consumer, 48000.).unwrap();
    assert_eq!(
        playback.prepare(ProcessConfig::new(48000., 8, 1, AudioLayout::STEREO_INSTRUMENT).unwrap()),
        Err(PrepareError::UnsupportedLayout)
    );
}

#[test]
fn pending_audio_does_not_consume_new_events_and_offsets_apply_once() {
    use ravet_foundation::{Event, EventData, ParameterId};
    struct Values {
        count: Arc<AtomicUsize>,
        value: f32,
    }
    impl Processor for Values {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
            Ok(())
        }
        fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
            let mut events = block.events.iter().peekable();
            for frame in 0..block.audio.frames() {
                while events
                    .peek()
                    .is_some_and(|event| event.frame_offset == frame)
                {
                    let EventData::ParameterValue { value, .. } = events.next().unwrap().data
                    else {
                        panic!("parameter")
                    };
                    self.value = value;
                    self.count.fetch_add(1, Ordering::Relaxed);
                }
                for channel in 0..2 {
                    block.audio.output_mut(channel).unwrap()[frame] =
                        self.value + channel as f32 * 100.;
                }
            }
            ProcessStatus::Continue
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {}
    }
    let count = Arc::new(AtomicUsize::new(0));
    let graph = Box::new(
        Graph::new(
            AudioLayout::STEREO_INSTRUMENT,
            vec![GraphNode {
                id: NodeId(1),
                layout: AudioLayout::STEREO_INSTRUMENT,
                processor: Box::new(Values {
                    count: count.clone(),
                    value: 1.,
                }),
            }],
            &(0..2)
                .map(|channel| Connection {
                    source: Source::Node {
                        id: NodeId(1),
                        channel,
                    },
                    destination: Destination::Output(channel),
                    gain: 1.,
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    );
    let (producer, mut consumer) = audio_ring(2, 4).unwrap();
    let cfg = ProcessConfig::new(48000., 8, 2, AudioLayout::STEREO_INSTRUMENT).unwrap();
    let mut prepared = PreparedGraphOutput::new(graph, producer, cfg).unwrap();
    let (prepared, count) = std::thread::spawn(move || {
        {
            let mut session = prepared.start_audio().unwrap();
            let first = [Event {
                frame_offset: 2,
                data: EventData::ParameterValue {
                    id: ParameterId(5),
                    value: 2.,
                },
            }];
            let next = [Event {
                frame_offset: 0,
                data: EventData::ParameterValue {
                    id: ParameterId(5),
                    value: 4.,
                },
            }];
            let counts = allocation_counter::measure(|| {
                let progress = session.pump_events(&first, Transport::default()).unwrap();
                assert!(progress.input_events_consumed);
                assert_eq!(count.load(Ordering::Relaxed), 1);
                let progress = session.pump_events(&next, Transport::default()).unwrap();
                assert!(!progress.input_events_consumed);
                assert_eq!(count.load(Ordering::Relaxed), 1);
                let mut samples = [0.; 8];
                assert_eq!(consumer.pop(&mut samples).unwrap(), 4);
                assert_eq!(samples, [1., 101., 1., 101., 2., 102., 2., 102.]);
                let progress = session.pump_events(&next, Transport::default()).unwrap();
                assert!(!progress.input_events_consumed);
                assert_eq!(progress.pending_frames, 0);
                assert_eq!(consumer.pop(&mut samples).unwrap(), 4);
                assert_eq!(samples, [2., 102., 2., 102., 2., 102., 2., 102.]);
                let progress = session.pump_events(&next, Transport::default()).unwrap();
                assert!(progress.input_events_consumed);
                assert_eq!(count.load(Ordering::Relaxed), 2);
                assert_eq!(consumer.pop(&mut samples).unwrap(), 4);
                assert_eq!(samples, [4., 104., 4., 104., 4., 104., 4., 104.]);
                let progress = session.pump(Transport::default()).unwrap();
                assert!(!progress.input_events_consumed);
                assert_eq!(consumer.pop(&mut samples).unwrap(), 4);
                let rejected = [Event {
                    frame_offset: 8,
                    data: EventData::ParameterValue {
                        id: ParameterId(5),
                        value: 3.,
                    },
                }];
                assert!(matches!(
                    session.pump_events(&rejected, Transport::default()),
                    Err(GraphStatus::Fault { node: None, .. })
                ));
                assert_eq!(consumer.queued_frames(), 0);
                assert_eq!(count.load(Ordering::Relaxed), 2);
            });
            assert_eq!(counts.count_total, 0);
            assert_eq!(counts.count_current, 0);
        }
        (prepared, count)
    })
    .join()
    .unwrap();
    assert_eq!(count.load(Ordering::Relaxed), 2);
    drop(prepared.into_graph());
}

#[test]
fn generated_output_is_returned_once_and_wrong_capacity_rejects_before_render() {
    use ravet_foundation::{Event, EventData, ParameterId};
    use ravet_graph::{EventConnection, EventEndpoint, EventFilter};
    struct Generated(Arc<AtomicUsize>);
    impl Processor for Generated {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
            Ok(())
        }
        fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
            let value = self.0.fetch_add(1, Ordering::Relaxed) + 1;
            for channel in 0..2 {
                block.audio.output_mut(channel).unwrap().fill(0.25);
            }
            block
                .output_events
                .as_mut()
                .unwrap()
                .push(Event {
                    frame_offset: 7,
                    data: EventData::ParameterValue {
                        id: ParameterId(12),
                        value: value as f32,
                    },
                })
                .unwrap();
            ProcessStatus::Continue
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {}
    }
    let count = Arc::new(AtomicUsize::new(0));
    let mut graph = Box::new(
        Graph::new(
            AudioLayout::STEREO_INSTRUMENT,
            vec![GraphNode {
                id: NodeId(1),
                layout: AudioLayout::STEREO_INSTRUMENT,
                processor: Box::new(Generated(count.clone())),
            }],
            &(0..2)
                .map(|channel| Connection {
                    source: Source::Node {
                        id: NodeId(1),
                        channel,
                    },
                    destination: Destination::Output(channel),
                    gain: 1.,
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    );
    graph
        .set_event_routes(&[EventConnection {
            source: EventEndpoint::Node(NodeId(1)),
            destination: EventEndpoint::Host,
            filter: EventFilter::All,
        }])
        .unwrap();
    let (producer, mut consumer) = audio_ring(2, 4).unwrap();
    let mut prepared =
        PreparedGraphOutput::new(graph, producer, config(8).with_output_events(1)).unwrap();
    let (prepared, count) = std::thread::spawn(move || {
        {
            let mut session = prepared.start_audio().unwrap();
            let mut storage = [Event {
                frame_offset: 0,
                data: EventData::ParameterValue {
                    id: ParameterId(0),
                    value: 0.,
                },
            }];
            let counts = allocation_counter::measure(|| {
                assert_eq!(
                    session.pump(Transport::default()),
                    Err(GraphStatus::Fault {
                        node: None,
                        error: ProcessError::OutputEventCapacity
                    })
                );
                assert_eq!(count.load(Ordering::Relaxed), 0);
                assert_eq!(consumer.queued_frames(), 0);
                let first = session
                    .pump_io(&[], Transport::default(), &mut storage)
                    .unwrap();
                assert_eq!(
                    (
                        first.rendered_frames,
                        first.output_event_count,
                        first.pending_frames
                    ),
                    (8, 1, 4)
                );
                assert_eq!(
                    storage[0],
                    Event {
                        frame_offset: 7,
                        data: EventData::ParameterValue {
                            id: ParameterId(12),
                            value: 1.
                        }
                    }
                );
                let pending = session
                    .pump_io(&[], Transport::default(), &mut storage)
                    .unwrap();
                assert_eq!(
                    (
                        pending.rendered_frames,
                        pending.output_event_count,
                        pending.published_frames
                    ),
                    (0, 0, 0)
                );
                let mut samples = [0.; 8];
                assert_eq!(consumer.pop(&mut samples).unwrap(), 4);
                assert_eq!(samples, [0.25; 8]);
                assert_eq!(
                    session
                        .pump(Transport::default())
                        .unwrap()
                        .output_event_count,
                    0
                );
                assert_eq!(consumer.pop(&mut samples).unwrap(), 4);
                let mut excess = [storage[0]; 2];
                assert_eq!(
                    session.pump_io(&[], Transport::default(), &mut excess),
                    Err(GraphStatus::Fault {
                        node: None,
                        error: ProcessError::OutputEventCapacity
                    })
                );
                assert_eq!(count.load(Ordering::Relaxed), 1);
                assert_eq!(
                    session
                        .pump_io(&[], Transport::default(), &mut storage)
                        .unwrap()
                        .output_event_count,
                    1
                );
                assert_eq!(
                    storage[0],
                    Event {
                        frame_offset: 7,
                        data: EventData::ParameterValue {
                            id: ParameterId(12),
                            value: 2.
                        }
                    }
                );
            });
            assert_eq!(counts.count_total, 0);
            assert_eq!(counts.count_current, 0);
        }
        (prepared, count)
    })
    .join()
    .unwrap();
    assert_eq!(count.load(Ordering::Relaxed), 2);
    drop(prepared.into_graph());
}

#[test]
fn worker_rejects_event_profiles_above_graph_limit_before_preparing_nodes() {
    use ravet_device::GraphOutputError;
    use ravet_graph::MAXIMUM_GRAPH_EVENTS;
    for output in [false, true] {
        let (observe, graph) = graph(false);
        let (producer, _consumer) = audio_ring(2, 4).unwrap();
        let config = if output {
            config(8).with_output_events(MAXIMUM_GRAPH_EVENTS + 1)
        } else {
            ProcessConfig::new(
                48000.,
                8,
                MAXIMUM_GRAPH_EVENTS + 1,
                AudioLayout::STEREO_INSTRUMENT,
            )
            .unwrap()
        };
        assert!(matches!(
            PreparedGraphOutput::new(graph, producer, config),
            Err(GraphOutputError::Configuration)
        ));
        assert_eq!(observe.calls.load(Ordering::Relaxed), 0);
        assert_eq!(observe.prepared.load(Ordering::Relaxed), 0);
        assert_eq!(observe.deactivated.load(Ordering::Relaxed), 1);
    }
}
