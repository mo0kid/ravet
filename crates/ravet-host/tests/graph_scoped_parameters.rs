#[path = "../examples/support/parameter_fixture.rs"]
mod fixture;
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, NoteId, NoteTarget, OutputEvents, ParameterId,
    ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor, Tail, Transport,
};
use ravet_host::{
    HostRequests, HostedGraphNodeBuilder, HostedGraphNodeOptions, HostedGraphNoteDialect,
    HostedGraphNoteInput, HostedGraphParameterMapping, RequestHost,
};
const VALUE: f64 = 0.12345678912345678;
const MOD: f64 = -0.02345678912345678;
fn instance(mode: u8) -> PluginInstance<RequestHost> {
    fixture::set_output_mode(mode);
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<fixture::Fixture>>(c"").unwrap();
    PluginInstance::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.host-parameter-fixture",
        &HostInfo::new("Scoped parameter fixture", "RAVET", "", "0.1").unwrap(),
    )
    .unwrap()
}
fn target() -> NoteTarget {
    NoteTarget {
        channel: Some(2),
        key: Some(60),
        id: Some(NoteId(123)),
    }
}
fn value(time: usize, target: NoteTarget, value: f64) -> Event {
    Event {
        frame_offset: time,
        data: EventData::ParameterValueScoped {
            id: ParameterId(17),
            target,
            value,
        },
    }
}
fn modulation(time: usize, target: NoteTarget, amount: f64) -> Event {
    Event {
        frame_offset: time,
        data: EventData::ParameterModulationScoped {
            id: ParameterId(17),
            target,
            amount,
        },
    }
}
fn base(value: f32) -> Event {
    Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: ParameterId(17),
            value,
        },
    }
}
fn options(capacity: usize) -> HostedGraphNodeOptions {
    HostedGraphNodeOptions {
        configuration: ProcessConfig::new(48000., 8, 5, AudioLayout::STEREO)
            .unwrap()
            .with_output_events(capacity),
        declared_latency: 0,
        declared_tail: Tail::None,
    }
}
fn mappings() -> Vec<HostedGraphParameterMapping> {
    vec![HostedGraphParameterMapping {
        graph_id: ParameterId(17),
        native_id: ClapId::new(1),
    }]
}
fn builder(capacity: usize) -> HostedGraphNodeBuilder {
    HostedGraphNodeBuilder::new(options(capacity))
        .with_parameter_input(mappings())
        .with_parameter_output(mappings())
        .with_note_input(HostedGraphNoteInput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Clap,
        })
}
macro_rules! profiles {
    ($name:ident,$sample:ty,$prepare:ident) => {
        #[test]
        fn $name() {
            for (input, output, capacity) in [
                (true, true, 4),
                (true, false, 4),
                (false, true, 4),
                (true, true, 1),
            ] {
                let opts = options(capacity);
                let (mut control, mut node) = builder(capacity)
                    .with_scoped_parameter_input(input)
                    .with_scoped_parameter_output(output)
                    .$prepare(instance(70), |_, _| ())
                    .unwrap();
                node.prepare(opts.configuration).unwrap();
                let mut node = std::thread::spawn(move || {
                    node.start_audio().unwrap();
                    let source = [0.25 as $sample; 8];
                    let inputs = [source.as_slice(); 2];
                    let mut samples = [[0. as $sample; 8]; 2];
                    let mut storage = [base(1.); 4];
                    let changes = [
                        base(2.),
                        value(0, target(), 2.),
                        modulation(0, target(), 0.),
                        value(2, target(), VALUE),
                        modulation(5, target(), MOD),
                    ];
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            {
                                let mut outputs = samples.each_mut().map(|s| s.as_mut_slice());
                                let audio =
                                    AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                                let mut block =
                                    ProcessBlock::new(audio, &changes, Transport::default());
                                block.output_events =
                                    Some(OutputEvents::new(&mut storage[..capacity], 8));
                                let status = node.process(&mut block);
                                if !input {
                                    assert!(matches!(
                                        status,
                                        ProcessStatus::Fault(ProcessError::Events(_))
                                    ));
                                } else if output && capacity == 1 {
                                    assert_eq!(
                                        status,
                                        ProcessStatus::Fault(ProcessError::BackendFailure)
                                    );
                                } else {
                                    assert_eq!(status, ProcessStatus::Continue);
                                    assert_eq!(
                                        block.output_events.as_ref().unwrap().as_slice(),
                                        if output { &changes[1..] } else { &[] }
                                    );
                                }
                                if !input || (output && capacity == 1) {
                                    assert!(block.output_events.as_ref().unwrap().is_empty());
                                }
                            }
                            if input && (!output || capacity != 1) {
                                assert_eq!(&samples[0][..2], &[0.5 as $sample; 2]);
                                assert_eq!(&samples[0][2..5], &[(0.25 * VALUE) as $sample; 3]);
                                assert_eq!(
                                    &samples[0][5..],
                                    &[(0.25 * (VALUE + MOD)) as $sample; 3]
                                );
                            } else {
                                assert_eq!(samples, [[0. as $sample; 8]; 2]);
                            }
                        }
                        let invalid = [
                            base(4.),
                            value(
                                2,
                                NoteTarget {
                                    id: Some(NoteId(u32::MAX)),
                                    ..target()
                                },
                                0.5,
                            ),
                        ];
                        {
                            let mut outputs = samples.each_mut().map(|s| s.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            let mut block =
                                ProcessBlock::new(audio, &invalid, Transport::default());
                            block.output_events =
                                Some(OutputEvents::new(&mut storage[..capacity], 8));
                            assert!(matches!(
                                node.process(&mut block),
                                ProcessStatus::Fault(ProcessError::Events(_))
                            ));
                        }
                        {
                            let mut outputs = samples.each_mut().map(|s| s.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            let mut block = ProcessBlock::new(audio, &[], Transport::default());
                            block.output_events =
                                Some(OutputEvents::new(&mut storage[..capacity], 8));
                            assert_eq!(node.process(&mut block), ProcessStatus::Continue);
                            assert!(block.output_events.as_ref().unwrap().is_empty());
                        }
                        assert_eq!(
                            samples,
                            [[if input {
                                (0.25 * (VALUE + MOD)) as $sample
                            } else {
                                0.25 as $sample
                            }; 8]; 2]
                        );
                        if input && capacity == 4 {
                            let inputs = [&[] as &[$sample]; 2];
                            let mut a = [];
                            let mut b = [];
                            let mut outputs = [a.as_mut_slice(), b.as_mut_slice()];
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 0).unwrap();
                            let flush = [value(0, target(), VALUE), modulation(0, target(), MOD)];
                            let mut block = ProcessBlock::new(audio, &flush, Transport::default());
                            block.output_events =
                                Some(OutputEvents::new(&mut storage[..capacity], 0));
                            assert_eq!(node.process(&mut block), ProcessStatus::Continue);
                            assert_eq!(
                                block.output_events.as_ref().unwrap().as_slice(),
                                if output { flush.as_slice() } else { &[] }
                            );
                        }
                    });
                    assert_eq!(counts.count_total, 0);
                    node.stop_audio();
                    node
                })
                .join()
                .unwrap();
                node.deactivate();
                assert!(control.reclaim());
                let mut instance = control.into_instance().ok().unwrap();
                assert_eq!(
                    ravet_host::HostedParameters::new(&mut instance, 4)
                        .unwrap()
                        .value(ClapId::new(1))
                        .unwrap(),
                    if input { 2. } else { 1. }
                );
            }
        }
    };
}
profiles!(
    native_f32_scoped_parameter_profile_precision_flush_base_and_prefix,
    f32,
    prepare
);
profiles!(
    native_f64_scoped_parameter_profile_precision_flush_base_and_prefix,
    f64,
    prepare64
);

#[test]
fn cached_scope_granularity_and_modulation_flags_reject_whole_input_before_base_changes() {
    for (mode, target, supported) in [
        (71, target(), false),
        (72, target(), false),
        (
            73,
            NoteTarget {
                id: None,
                ..target()
            },
            true,
        ),
        (
            74,
            NoteTarget {
                id: None,
                key: None,
                ..target()
            },
            true,
        ),
        (75, NoteTarget::default(), true),
        (76, target(), true),
    ] {
        let opts = options(2);
        let (mut control, mut node) = builder(2)
            .with_scoped_parameter_input(true)
            .with_scoped_parameter_output(true)
            .prepare(instance(mode), |_, _| ())
            .unwrap();
        node.prepare(opts.configuration).unwrap();
        let mut node = std::thread::spawn(move || {
            node.start_audio().unwrap();
            let source = [0.25; 8];
            let inputs = [source.as_slice(); 2];
            let changes = [
                base(2.),
                value(2, target, VALUE),
                modulation(5, target, MOD),
            ];
            let mut samples = [[0.; 8]; 2];
            let mut storage = [base(1.); 2];
            let counts = allocation_counter::measure(|| {
                for _ in 0..1000 {
                    let mut outputs = samples.each_mut().map(|s| s.as_mut_slice());
                    let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                    let mut block = ProcessBlock::new(audio, &changes, Transport::default());
                    block.output_events = Some(OutputEvents::new(&mut storage, 8));
                    let status = node.process(&mut block);
                    if supported {
                        assert_eq!(status, ProcessStatus::Continue);
                        assert_eq!(
                            block.output_events.as_ref().unwrap().as_slice(),
                            &changes[1..]
                        );
                    } else {
                        assert!(matches!(
                            status,
                            ProcessStatus::Fault(ProcessError::Events(_))
                        ));
                        assert!(block.output_events.as_ref().unwrap().is_empty());
                    }
                }
            });
            assert_eq!(counts.count_total, 0);
            if supported {
                assert_eq!(&samples[0][5..], &[(0.25 * (VALUE + MOD)) as f32; 3]);
            } else {
                assert_eq!(samples, [[0.; 8]; 2]);
            }
            node.stop_audio();
            node
        })
        .join()
        .unwrap();
        node.deactivate();
        assert!(control.reclaim());
        let mut instance = control.into_instance().ok().unwrap();
        assert_eq!(
            ravet_host::HostedParameters::new(&mut instance, 4)
                .unwrap()
                .value(ClapId::new(1))
                .unwrap(),
            if supported { 2. } else { 1. }
        );
    }
}

#[test]
fn selected_scope_output_metadata_and_scalars_fault_after_native_history_while_unselected_filters()
{
    for (mode, enabled) in [
        (71, true),
        (72, true),
        (77, true),
        (77, false),
        (78, true),
        (79, true),
    ] {
        let opts = options(3);
        let (mut control, mut node) = builder(3)
            .with_scoped_parameter_output(enabled)
            .prepare(instance(mode), |_, _| ())
            .unwrap();
        node.prepare(opts.configuration).unwrap();
        let mut node = std::thread::spawn(move || {
            node.start_audio().unwrap();
            let source = [0.25; 8];
            let inputs = [source.as_slice(); 2];
            let mut samples = [[0.; 8]; 2];
            let mut storage = [base(1.); 3];
            let changes = [base(2.)];
            {
                let mut outputs = samples.each_mut().map(|s| s.as_mut_slice());
                let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                let mut block = ProcessBlock::new(audio, &changes, Transport::default());
                block.output_events = Some(OutputEvents::new(&mut storage, 8));
                let status = node.process(&mut block);
                assert!(block.output_events.as_ref().unwrap().is_empty());
                if enabled && matches!(mode, 71 | 72 | 77) {
                    assert_eq!(status, ProcessStatus::Fault(ProcessError::BackendFailure));
                } else {
                    assert_eq!(status, ProcessStatus::Continue);
                }
            }
            assert_eq!(
                samples,
                [[if enabled && matches!(mode, 71 | 72 | 77) {
                    0.
                } else {
                    0.5
                }; 8]; 2]
            );
            node.stop_audio();
            node
        })
        .join()
        .unwrap();
        node.deactivate();
        assert!(control.reclaim());
        let mut instance = control.into_instance().ok().unwrap();
        assert_eq!(
            ravet_host::HostedParameters::new(&mut instance, 4)
                .unwrap()
                .value(ClapId::new(1))
                .unwrap(),
            2.
        );
    }
}

#[test]
fn scoped_directions_require_maps_and_selected_clap_input_endpoint_before_activation() {
    for builder in [
        HostedGraphNodeBuilder::new(options(4)).with_scoped_parameter_input(true),
        HostedGraphNodeBuilder::new(options(4))
            .with_parameter_output(mappings())
            .with_scoped_parameter_output(true),
        HostedGraphNodeBuilder::new(options(4))
            .with_parameter_input(mappings())
            .with_note_input(HostedGraphNoteInput {
                port_index: 1,
                dialect: HostedGraphNoteDialect::Midi,
            })
            .with_scoped_parameter_input(true),
        HostedGraphNodeBuilder::new(options(4))
            .with_note_input(HostedGraphNoteInput {
                port_index: 1,
                dialect: HostedGraphNoteDialect::Clap,
            })
            .with_scoped_parameter_output(true),
    ] {
        let error = builder
            .prepare(instance(70), |_, _| ())
            .err()
            .expect("selected input endpoint and mapping");
        assert!(matches!(
            error.reason,
            ravet_host::HostedGraphNodeBuildFailure::Configuration
        ));
    }
    let readonly = vec![HostedGraphParameterMapping {
        graph_id: ParameterId(17),
        native_id: ClapId::new(2),
    }];
    let error = HostedGraphNodeBuilder::new(options(4))
        .with_parameter_input(readonly)
        .with_parameter_output(mappings())
        .with_note_input(HostedGraphNoteInput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Clap,
        })
        .with_scoped_parameter_input(true)
        .prepare(instance(70), |_, _| ())
        .err()
        .expect("writable mapping");
    assert!(matches!(
        error.reason,
        ravet_host::HostedGraphNodeBuildFailure::Parameters(
            ravet_host::HostedParameterError::ReadOnly
        )
    ));
}

#[test]
fn scoped_value_ranges_steps_and_unknown_ids_reject_atomically_while_fractional_modulation_remains_valid()
 {
    for (mode, data) in [
        (70, value(2, target(), -0.1)),
        (70, value(2, target(), 4.01)),
        (80, value(2, target(), 0.5)),
        (
            70,
            Event {
                frame_offset: 2,
                data: EventData::ParameterValueScoped {
                    id: ParameterId(999),
                    target: target(),
                    value: 0.5,
                },
            },
        ),
    ] {
        let opts = options(2);
        let (mut control, mut node) = builder(2)
            .with_scoped_parameter_input(true)
            .with_scoped_parameter_output(true)
            .prepare(instance(mode), |_, _| ())
            .unwrap();
        node.prepare(opts.configuration).unwrap();
        let mut node = std::thread::spawn(move || {
            node.start_audio().unwrap();
            let source = [0.25; 8];
            let inputs = [source.as_slice(); 2];
            let mut samples = [[0.; 8]; 2];
            let mut storage = [base(1.); 2];
            let changes = [base(4.), data];
            let mut outputs = samples.each_mut().map(|s| s.as_mut_slice());
            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
            let mut block = ProcessBlock::new(audio, &changes, Transport::default());
            block.output_events = Some(OutputEvents::new(&mut storage, 8));
            assert!(matches!(
                node.process(&mut block),
                ProcessStatus::Fault(ProcessError::Events(_))
            ));
            assert!(block.output_events.as_ref().unwrap().is_empty());
            node.stop_audio();
            node
        })
        .join()
        .unwrap();
        node.deactivate();
        assert!(control.reclaim());
        let mut instance = control.into_instance().ok().unwrap();
        assert_eq!(
            ravet_host::HostedParameters::new(&mut instance, 4)
                .unwrap()
                .value(ClapId::new(1))
                .unwrap(),
            1.
        );
    }
    let opts = options(1);
    let (mut control, mut node) = builder(1)
        .with_scoped_parameter_input(true)
        .with_scoped_parameter_output(true)
        .prepare(instance(80), |_, _| ())
        .unwrap();
    node.prepare(opts.configuration).unwrap();
    let mut node = std::thread::spawn(move || {
        node.start_audio().unwrap();
        let source = [0.25; 8];
        let inputs = [source.as_slice(); 2];
        let mut samples = [[0.; 8]; 2];
        let mut storage = [base(1.); 1];
        let changes = [base(2.), modulation(2, target(), 0.5)];
        {
            let mut outputs = samples.each_mut().map(|s| s.as_mut_slice());
            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
            let mut block = ProcessBlock::new(audio, &changes, Transport::default());
            block.output_events = Some(OutputEvents::new(&mut storage, 8));
            assert_eq!(node.process(&mut block), ProcessStatus::Continue);
            assert_eq!(
                block.output_events.as_ref().unwrap().as_slice(),
                &changes[1..]
            );
        }
        assert_eq!(&samples[0][2..], &[0.625; 6]);
        node.stop_audio();
        node
    })
    .join()
    .unwrap();
    node.deactivate();
    assert!(control.reclaim());
}
