#[path = "../examples/support/parameter_fixture.rs"]
mod fixture;
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, EventError, ParameterId, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Tail, Transport,
};
use ravet_host::{
    HostRequests, HostedGraphNodeOptions, HostedGraphParameterMapping, HostedGraphPortMode,
    HostedParameterError, HostedParameters, RequestHost,
};
fn instance() -> PluginInstance<RequestHost> {
    instance_mode(0)
}
fn instance_mode(mode: u8) -> PluginInstance<RequestHost> {
    fixture::set_output_mode(mode);
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<fixture::Fixture>>(c"").unwrap();
    let info = HostInfo::new("Synthetic parameter graph host", "RAVET", "", "0.1").unwrap();
    PluginInstance::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.host-parameter-fixture",
        &info,
    )
    .unwrap()
}
fn mapping(native: u32, graph: u32) -> HostedGraphParameterMapping {
    HostedGraphParameterMapping {
        graph_id: ParameterId(graph),
        native_id: ClapId::new(native),
    }
}
fn options(events: usize) -> HostedGraphNodeOptions {
    HostedGraphNodeOptions {
        configuration: ProcessConfig::new(48000., 8, events, AudioLayout::STEREO).unwrap(),
        declared_latency: 0,
        declared_tail: Tail::None,
    }
}
fn event(offset: usize, id: u32, value: f32) -> Event {
    Event {
        frame_offset: offset,
        data: EventData::ParameterValue {
            id: ParameterId(id),
            value,
        },
    }
}
#[test]
fn mapping_rejection_retains_inactive_instance_before_audio_activation() {
    for (events, mappings, expected) in [
        (0, vec![mapping(1, 9001)], HostedParameterError::Capacity),
        (4097, vec![mapping(1, 9001)], HostedParameterError::Capacity),
        (4, vec![], HostedParameterError::Capacity),
        (4, vec![mapping(2, 9001)], HostedParameterError::ReadOnly),
        (4, vec![mapping(3, 9001)], HostedParameterError::Unknown),
        (
            4,
            vec![mapping(1, 9001), mapping(1, 9002)],
            HostedParameterError::Metadata,
        ),
        (
            4,
            vec![mapping(1, 9001), mapping(2, 9001)],
            HostedParameterError::Metadata,
        ),
    ] {
        let error = ravet_host::prepare_hosted_parameter_graph_node(
            instance(),
            options(events),
            HostedGraphPortMode::MainOnly,
            mappings,
            |_, _| panic!("before activation"),
        )
        .err()
        .unwrap();
        let expected = if events == 4097 {
            ravet_host::HostedGraphNodeBuildFailure::Configuration
        } else {
            ravet_host::HostedGraphNodeBuildFailure::Parameters(expected)
        };
        assert_eq!(format!("{:?}", error.reason), format!("{expected:?}"));
        assert!(!error.instance.is_active());
    }
}
macro_rules! parameter_node_test {
    ($test:ident, $sample:ty, $prepare:ident, $value:expr) => {
        #[test]
        fn $test() {
            let opts = options(4);
            let (mut control, mut node) = ravet_host::$prepare(
                instance(),
                opts,
                HostedGraphPortMode::MainOnly,
                vec![mapping(1, 9001)],
                |_, _| (),
            )
            .unwrap();
            node.prepare(opts.configuration).unwrap();
            let mut node = std::thread::spawn(move || {
                node.start_audio().unwrap();
                let input = [$value as $sample; 8];
                let inputs = [input.as_slice(), input.as_slice()];
                let mut output = [[0. as $sample; 8]; 2];
                let valid = [
                    event(0, 9001, 2.),
                    event(3, 9001, 3.),
                    event(3, 9001, 1.),
                    event(7, 9001, 0.),
                ];
                let counts = allocation_counter::measure(|| {
                    for block in 0..1000 {
                        for invalid in [
                            event(3, 9002, 2.),
                            event(3, 9001, 0.5),
                            event(3, 9001, 5.),
                            Event {
                                frame_offset: 3,
                                data: EventData::NoteOn {
                                    channel: 0,
                                    key: 60,
                                    velocity: 1.,
                                },
                            },
                        ] {
                            let rejected = [event(0, 9001, 2.), invalid];
                            let mut outputs =
                                output.each_mut().map(|samples| samples.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            assert_eq!(
                                node.process(&mut ProcessBlock::new(
                                    audio,
                                    &rejected,
                                    Transport::default()
                                )),
                                ProcessStatus::Fault(ProcessError::Events(
                                    EventError::InvalidValue { index: 1 }
                                ))
                            );
                            assert_eq!(output, [[0. as $sample; 8]; 2]);
                            let mut outputs =
                                output.each_mut().map(|samples| samples.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            assert_eq!(
                                node.process(&mut ProcessBlock::new(
                                    audio,
                                    &[],
                                    Transport::default()
                                )),
                                ProcessStatus::Continue
                            );
                            assert_eq!(
                                output,
                                [[($value * if block == 0 { 1. } else { 4. }) as $sample; 8]; 2]
                            );
                        }
                        let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                        assert_eq!(
                            node.process(&mut ProcessBlock::new(
                                audio,
                                &valid,
                                Transport::default()
                            )),
                            ProcessStatus::Continue
                        );
                        for channel in &output {
                            for frame in 0..8 {
                                assert_eq!(
                                    channel[frame],
                                    input[frame]
                                        * if frame < 3 {
                                            2.
                                        } else if frame < 7 {
                                            1.
                                        } else {
                                            0.
                                        }
                                );
                            }
                        }
                        let empty_inputs = [&[] as &[$sample]; 2];
                        let mut empty_a = [];
                        let mut empty_b = [];
                        let mut empty_outputs = [empty_a.as_mut_slice(), empty_b.as_mut_slice()];
                        let audio =
                            AudioBuffer::separate(&empty_inputs, &mut empty_outputs, 0).unwrap();
                        assert_eq!(
                            node.process(&mut ProcessBlock::new(
                                audio,
                                &[event(0, 9001, 4.)],
                                Transport::default()
                            )),
                            ProcessStatus::Continue
                        );
                        let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                        assert_eq!(
                            node.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                            ProcessStatus::Continue
                        );
                        assert_eq!(output, [[($value * 4.) as $sample; 8]; 2]);
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
            let stats = control.statistics();
            assert_eq!(stats.processed_blocks, 6000);
            assert_eq!(stats.processed_frames, 48000);
            assert_eq!(stats.parameter_flushes, 1000);
            assert_eq!(stats.backend_failures, 0);
            drop(node);
            let mut instance = control.into_instance().ok().unwrap();
            assert_eq!(
                HostedParameters::new(&mut instance, 2)
                    .unwrap()
                    .value(ClapId::new(1))
                    .unwrap(),
                4.
            );
        }
    };
}
parameter_node_test!(
    native_f32_parameter_identity_timing_rejection_and_empty_flush_are_heap_free,
    f32,
    prepare_hosted_parameter_graph_node,
    0.25
);
parameter_node_test!(
    native_f64_parameter_identity_timing_rejection_and_empty_flush_are_heap_free,
    f64,
    prepare_hosted_parameter_graph_node64,
    1e200
);

#[test]
fn linked_real_gain_uses_plain_aliases_sample_timing_and_active_empty_flush() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ravet_gain_clap::GainPlugin>>(c"")
        .unwrap();
    let info = HostInfo::new("Synthetic real gain graph parameters", "RAVET", "", "0.1").unwrap();
    let instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.gain",
        &info,
    )
    .unwrap();
    let opts = HostedGraphNodeOptions {
        configuration: ProcessConfig::new(48000., 256, 4, AudioLayout::STEREO).unwrap(),
        declared_latency: 0,
        declared_tail: Tail::None,
    };
    let (mut control, mut node) = ravet_host::prepare_hosted_parameter_graph_node(
        instance,
        opts,
        HostedGraphPortMode::MainOnly,
        vec![mapping(1, 9001), mapping(2, 9002)],
        |_, _| (),
    )
    .unwrap();
    node.prepare(opts.configuration).unwrap();
    let mut node = std::thread::spawn(move || {
        node.start_audio().unwrap();
        let input = [0.25; 256];
        let inputs = [input.as_slice(), input.as_slice()];
        let mut output = [[0.; 256]; 2];
        let changes = [event(0, 9001, 2.), event(128, 9002, 1.)];
        let counts = allocation_counter::measure(|| {
            let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
            let audio = AudioBuffer::separate(&inputs, &mut outputs, 256).unwrap();
            assert_eq!(
                node.process(&mut ProcessBlock::new(
                    audio,
                    &changes,
                    Transport::default()
                )),
                ProcessStatus::Continue
            );
            for channel in &output {
                for (frame, sample) in channel.iter().enumerate() {
                    let expected = if frame < 128 {
                        0.25 * (1. + (frame + 1) as f32 / 240.)
                    } else {
                        0.25
                    };
                    assert!(
                        (sample - expected).abs() < 1e-6,
                        "frame {frame}: {sample} versus {expected}"
                    );
                }
            }
            let empty_inputs = [&[] as &[f32]; 2];
            let mut a = [];
            let mut b = [];
            let mut outputs = [a.as_mut_slice(), b.as_mut_slice()];
            let audio = AudioBuffer::separate(&empty_inputs, &mut outputs, 0).unwrap();
            assert_eq!(
                node.process(&mut ProcessBlock::new(
                    audio,
                    &[event(0, 9001, 0.5), event(0, 9002, 0.)],
                    Transport::default()
                )),
                ProcessStatus::Continue
            );
            let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
            let audio = AudioBuffer::separate(&inputs, &mut outputs, 256).unwrap();
            assert_eq!(
                node.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                ProcessStatus::Continue
            );
            for channel in &output {
                for (frame, sample) in channel.iter().enumerate() {
                    let expected = 0.25 * (2. - 1.5 * ((frame + 1) as f32 / 240.).min(1.));
                    assert!(
                        (sample - expected).abs() < 1e-6,
                        "after flush frame {frame}: {sample} versus {expected}"
                    );
                }
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
    assert_eq!(control.statistics().parameter_flushes, 1);
    drop(node);
    let mut instance = control.into_instance().ok().unwrap();
    let mut parameters = HostedParameters::new(&mut instance, 2).unwrap();
    assert_eq!(parameters.value(ClapId::new(1)).unwrap(), 0.5);
    assert_eq!(parameters.value(ClapId::new(2)).unwrap(), 0.);
}

macro_rules! output_parameter_test {
    ($test:ident,$sample:ty,$prepare:ident,$value:expr) => {
        #[test]
        fn $test() {
            for mode in [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12] {
                let mut opts = options(4);
                opts.configuration =
                    opts.configuration
                        .with_output_events(if mode == 4 { 2 } else { 1 });
                let (mut control, mut node) = ravet_host::HostedGraphNodeBuilder::new(opts)
                    .with_parameter_input(vec![mapping(1, 9001)])
                    .with_parameter_output(vec![mapping(2, 9100)])
                    .$prepare(instance_mode(mode), |_, _| ())
                    .unwrap();
                node.prepare(opts.configuration).unwrap();
                let mut node = std::thread::spawn(move || {
                    node.start_audio().unwrap();
                    let input = [$value as $sample; 8];
                    let inputs = [input.as_slice(), input.as_slice()];
                    let mut output = [[0. as $sample; 8]; 2];
                    let mut storage = [event(0, 0, 0.); 2];
                    let fault = matches!(mode, 1 | 2 | 3 | 4 | 7 | 10 | 11);
                    let count = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            let mut outputs =
                                output.each_mut().map(|samples| samples.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            let changes = [event(0, 9001, 2.)];
                            let mut block =
                                ProcessBlock::new(audio, &changes, Transport::default());
                            block.output_events = Some(ravet_foundation::OutputEvents::new(
                                &mut storage[..opts.configuration.maximum_output_events()],
                                8,
                            ));
                            let status = node.process(&mut block);
                            if fault {
                                assert_eq!(
                                    status,
                                    ProcessStatus::Fault(ProcessError::BackendFailure)
                                );
                                assert!(block.output_events.as_ref().unwrap().is_empty());
                                assert_eq!(output, [[0. as $sample; 8]; 2]);
                            } else {
                                assert_eq!(status, ProcessStatus::Continue);
                                if mode == 12 {
                                    assert!(block.output_events.as_ref().unwrap().is_empty());
                                } else {
                                    assert_eq!(
                                        block.output_events.as_ref().unwrap().as_slice(),
                                        &[event(7, 9100, if mode == 8 { 0.1 } else { 2. })]
                                    );
                                }
                                assert_eq!(output, [[($value * 2.) as $sample; 8]; 2]);
                            }
                        }
                    });
                    assert_eq!(count.count_total, 0);
                    assert_eq!(count.count_current, 0);
                    if !fault {
                        let inputs = [&[] as &[$sample]; 2];
                        let mut a = [];
                        let mut b = [];
                        let mut outputs = [a.as_mut_slice(), b.as_mut_slice()];
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 0).unwrap();
                        let changes = [event(0, 9001, 4.)];
                        let mut block = ProcessBlock::new(audio, &changes, Transport::default());
                        block.output_events =
                            Some(ravet_foundation::OutputEvents::new(&mut storage[..1], 0));
                        assert_eq!(node.process(&mut block), ProcessStatus::Continue);
                        if mode == 12 {
                            assert!(block.output_events.as_ref().unwrap().is_empty());
                        } else {
                            assert_eq!(
                                block.output_events.as_ref().unwrap().as_slice(),
                                &[event(0, 9100, if mode == 8 { 0.1 } else { 4. })]
                            );
                        }
                    }
                    node.stop_audio();
                    node
                })
                .join()
                .unwrap();
                node.deactivate();
                assert!(control.reclaim());
                let stats = control.statistics();
                assert_eq!(
                    stats.backend_failures,
                    if matches!(mode, 1 | 2 | 3 | 4 | 7 | 10 | 11) {
                        1000
                    } else {
                        0
                    }
                );
                assert_eq!(
                    stats.rejected_native_events,
                    if matches!(mode, 1 | 2 | 3 | 4 | 7 | 10 | 11) {
                        1000
                    } else {
                        0
                    }
                );
                assert_eq!(
                    stats.filtered_native_events,
                    if mode == 5 || mode == 6 || mode == 12 {
                        1001
                    } else {
                        0
                    }
                );
                drop(node);
                let mut instance = control.into_instance().ok().unwrap();
                assert_eq!(
                    HostedParameters::new(&mut instance, 2)
                        .unwrap()
                        .value(ClapId::new(1))
                        .unwrap(),
                    if matches!(mode, 1 | 2 | 3 | 4 | 7 | 10 | 11) {
                        2.
                    } else {
                        4.
                    }
                );
            }
        }
    };
}
output_parameter_test!(
    native_f32_output_alias_readonly_flush_filter_and_nonrollback_faults,
    f32,
    prepare,
    0.25
);
output_parameter_test!(
    native_f64_output_alias_readonly_flush_filter_and_nonrollback_faults,
    f64,
    prepare64,
    1e200
);

#[test]
fn output_only_readonly_mapping_flushes_without_input_and_requires_exact_outer_capacity() {
    let mut opts = options(0);
    opts.configuration = opts.configuration.with_output_events(1);
    let (mut control, mut node) = ravet_host::HostedGraphNodeBuilder::new(opts)
        .with_parameter_output(vec![mapping(2, 9100)])
        .prepare(instance(), |_, _| ())
        .unwrap();
    node.prepare(opts.configuration).unwrap();
    let mut node = std::thread::spawn(move || {
        node.start_audio().unwrap();
        let inputs = [&[] as &[f32]; 2];
        let mut a = [];
        let mut b = [];
        let mut outputs = [a.as_mut_slice(), b.as_mut_slice()];
        let audio = AudioBuffer::separate(&inputs, &mut outputs, 0).unwrap();
        let mut block = ProcessBlock::new(audio, &[], Transport::default());
        assert_eq!(
            node.process(&mut block),
            ProcessStatus::Fault(ProcessError::OutputEventCapacity)
        );
        let mut storage = [event(0, 0, 0.)];
        block.output_events = Some(ravet_foundation::OutputEvents::new(&mut storage, 0));
        assert_eq!(node.process(&mut block), ProcessStatus::Continue);
        assert_eq!(
            block.output_events.as_ref().unwrap().as_slice(),
            &[event(0, 9100, 1.)]
        );
        node.stop_audio();
        node
    })
    .join()
    .unwrap();
    node.deactivate();
    assert!(control.reclaim());
    assert_eq!(control.statistics().parameter_flushes, 1);
    drop(node);
    drop(control);
}

#[test]
fn builder_output_metadata_and_capacity_fail_before_activation_with_instance_retained() {
    for (capacity, mappings, expected) in [
        (0, vec![mapping(2, 9100)], HostedParameterError::Capacity),
        (1, vec![], HostedParameterError::Capacity),
        (1, vec![mapping(3, 9100)], HostedParameterError::Unknown),
        (
            1,
            vec![mapping(1, 9100), mapping(2, 9100)],
            HostedParameterError::Metadata,
        ),
        (
            1,
            vec![mapping(2, 9100), mapping(2, 9101)],
            HostedParameterError::Metadata,
        ),
    ] {
        let mut opts = options(0);
        opts.configuration = opts.configuration.with_output_events(capacity);
        let error = ravet_host::HostedGraphNodeBuilder::new(opts)
            .with_parameter_output(mappings)
            .prepare(instance(), |_, _| {
                panic!("output metadata before activation")
            })
            .err()
            .unwrap();
        assert!(
            matches!(error.reason,ravet_host::HostedGraphNodeBuildFailure::Parameters(reason) if reason==expected)
        );
        assert!(!error.instance.is_active());
    }
    let mut opts = options(0);
    opts.configuration = opts.configuration.with_output_events(1);
    let error = ravet_host::HostedGraphNodeBuilder::new(opts)
        .prepare(instance(), |_, _| panic!("unconfigured output capacity"))
        .err()
        .unwrap();
    assert!(matches!(
        error.reason,
        ravet_host::HostedGraphNodeBuildFailure::Configuration
    ));
    assert!(!error.instance.is_active());
}
