#[path = "../examples/support/parameter_fixture.rs"]
mod fixture;
#[path = "../examples/support/sine_graph_probe.rs"]
mod sine_probe;
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, EventError, ParameterId, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Tail, Transport, UmpPacket,
};
use ravet_host::{
    HostRequests, HostedGraphNodeBuilder, HostedGraphNodeOptions, HostedGraphNoteDialect,
    HostedGraphNoteError, HostedGraphNoteInput, HostedGraphParameterMapping, RequestHost,
};
fn instance() -> PluginInstance<RequestHost> {
    instance_mode(0)
}
fn instance_mode(mode: u8) -> PluginInstance<RequestHost> {
    fixture::set_output_mode(mode);
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<fixture::Fixture>>(c"").unwrap();
    PluginInstance::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.host-parameter-fixture",
        &HostInfo::new("Synthetic note graph host", "RAVET", "", "0.1").unwrap(),
    )
    .unwrap()
}
fn options(capacity: usize) -> HostedGraphNodeOptions {
    HostedGraphNodeOptions {
        configuration: ProcessConfig::new(48000., 8, capacity, AudioLayout::STEREO).unwrap(),
        declared_latency: 0,
        declared_tail: Tail::None,
    }
}
fn parameter(frame_offset: usize, value: f32) -> Event {
    Event {
        frame_offset,
        data: EventData::ParameterValue {
            id: ParameterId(9001),
            value,
        },
    }
}
fn note(frame_offset: usize, on: bool, dialect: HostedGraphNoteDialect) -> Event {
    let data = if dialect == HostedGraphNoteDialect::Midi2 {
        EventData::Ump(
            UmpPacket::from_words(&[if on { 0x42903C00 } else { 0x42803C00 }, 0x80000000]).unwrap(),
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
    Event { frame_offset, data }
}
#[test]
fn note_metadata_rejection_retains_inactive_instance_and_uses_index_not_id() {
    for (port_index, dialect, expected) in [
        (
            0,
            HostedGraphNoteDialect::Clap,
            HostedGraphNoteError::Dialect,
        ),
        (
            0,
            HostedGraphNoteDialect::Midi2,
            HostedGraphNoteError::Dialect,
        ),
        (
            2,
            HostedGraphNoteDialect::Midi,
            HostedGraphNoteError::Unknown,
        ),
        (
            101,
            HostedGraphNoteDialect::Clap,
            HostedGraphNoteError::Unknown,
        ),
        (
            256,
            HostedGraphNoteDialect::Clap,
            HostedGraphNoteError::Capacity,
        ),
    ] {
        let error = HostedGraphNodeBuilder::new(options(4))
            .with_note_input(HostedGraphNoteInput {
                port_index,
                dialect,
            })
            .prepare(instance(), |_, _| panic!("before activation"))
            .err()
            .unwrap();
        assert_eq!(
            format!("{:?}", error.reason),
            format!("Notes({expected:?})")
        );
        assert!(!error.instance.is_active());
    }
    let error = HostedGraphNodeBuilder::new(options(0))
        .with_note_input(HostedGraphNoteInput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Clap,
        })
        .prepare(instance(), |_, _| panic!("before activation"))
        .err()
        .unwrap();
    assert!(matches!(
        error.reason,
        ravet_host::HostedGraphNodeBuildFailure::Parameters(
            ravet_host::HostedParameterError::Capacity
        )
    ));
    assert!(!error.instance.is_active());
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ravet_gain_clap::GainPlugin>>(c"")
        .unwrap();
    let instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.gain",
        &HostInfo::new("Missing note extension probe", "RAVET", "", "0.1").unwrap(),
    )
    .unwrap();
    let error = HostedGraphNodeBuilder::new(options(4))
        .with_note_input(HostedGraphNoteInput {
            port_index: 0,
            dialect: HostedGraphNoteDialect::Midi,
        })
        .prepare(instance, |_, _| panic!("before activation"))
        .err()
        .unwrap();
    assert!(matches!(
        error.reason,
        ravet_host::HostedGraphNodeBuildFailure::Notes(HostedGraphNoteError::Unavailable)
    ));
    assert!(!error.instance.is_active());
}
macro_rules! note_stream {
    ($name:ident, $sample:ty, $prepare:ident, $value:expr) => {
        #[test]
        fn $name() {
            for dialect in [
                HostedGraphNoteDialect::Clap,
                HostedGraphNoteDialect::Midi,
                HostedGraphNoteDialect::MidiMpe,
                HostedGraphNoteDialect::Midi2,
            ] {
                let opts = options(4);
                let (mut control, mut node) = HostedGraphNodeBuilder::new(opts)
                    .with_note_input(HostedGraphNoteInput {
                        port_index: 1,
                        dialect,
                    })
                    .with_parameter_input(vec![HostedGraphParameterMapping {
                        graph_id: ParameterId(9001),
                        native_id: ClapId::new(1),
                    }])
                    .$prepare(instance(), |_, _| ())
                    .unwrap();
                node.prepare(opts.configuration).unwrap();
                let mut node = std::thread::spawn(move || {
                    node.start_audio().unwrap();
                    let input = [$value as $sample; 8];
                    let inputs = [input.as_slice(); 2];
                    let mut output = [[0. as $sample; 8]; 2];
                    // Same-frame order: the note wins over the preceding parameter.
                    let stream = [
                        parameter(0, 3.),
                        parameter(2, 4.),
                        note(2, true, dialect),
                        note(6, false, dialect),
                    ];
                    let mismatch = note(
                        3,
                        true,
                        if dialect == HostedGraphNoteDialect::Midi2 {
                            HostedGraphNoteDialect::Clap
                        } else {
                            HostedGraphNoteDialect::Midi2
                        },
                    );
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            let mut outputs =
                                output.each_mut().map(|samples| samples.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            assert_eq!(
                                node.process(&mut ProcessBlock::new(
                                    audio,
                                    &stream,
                                    Transport::default()
                                )),
                                ProcessStatus::Continue
                            );
                            for channel in &output {
                                for (frame, sample) in channel.iter().enumerate() {
                                    assert_eq!(
                                        *sample,
                                        input[frame]
                                            * if frame < 2 {
                                                3.
                                            } else if frame < 6 {
                                                2.
                                            } else {
                                                0.
                                            }
                                    );
                                }
                            }
                            let rejected = [parameter(0, 4.), mismatch];
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
                            let empty_inputs = [&[] as &[$sample]; 2];
                            let mut a = [];
                            let mut b = [];
                            let mut empty_outputs = [a.as_mut_slice(), b.as_mut_slice()];
                            let audio = AudioBuffer::separate(&empty_inputs, &mut empty_outputs, 0)
                                .unwrap();
                            let rejected = [parameter(0, 4.), note(0, true, dialect)];
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
                            // Both rejects precede native mutation, including zero-frame flush.
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
                            assert_eq!(output, [[0. as $sample; 8]; 2]);
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
                assert_eq!(control.statistics().processed_blocks, 2000);
                assert_eq!(control.statistics().parameter_flushes, 0);
                drop(node);
                let mut instance = control.into_instance().ok().unwrap();
                assert_eq!(
                    ravet_host::HostedParameters::new(&mut instance, 2)
                        .unwrap()
                        .value(ClapId::new(1))
                        .unwrap(),
                    0.
                );
            }
        }
    };
}
note_stream!(
    mixed_f32_notes_and_parameters_reject_before_mutation_without_heap,
    f32,
    prepare,
    0.25
);
note_stream!(
    mixed_f64_notes_and_parameters_reject_before_mutation_without_heap,
    f64,
    prepare64,
    1e200
);

#[test]
fn linked_sine_note_only_node_has_timed_sound_and_release_silence_without_heap() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ravet_sine_clap::SinePlugin>>(c"")
        .unwrap();
    let instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.sine",
        &HostInfo::new("Linked sine note-only probe", "RAVET", "", "0.1").unwrap(),
    )
    .unwrap();
    sine_probe::run(instance).unwrap();
}

macro_rules! output_faults {
    ($name:ident, $sample:ty, $prepare:ident, $value:expr) => {
        #[test]
        fn $name() {
            for (mode, capacity) in [
                (30, 2),
                (30, 1),
                (31, 2),
                (32, 2),
                (33, 2),
                (34, 2),
                (35, 2),
            ] {
                let mut opts = options(1);
                opts.configuration = opts.configuration.with_output_events(capacity);
                let dialect = if matches!(mode, 33 | 34) {
                    HostedGraphNoteDialect::Clap
                } else {
                    HostedGraphNoteDialect::Midi
                };
                let (mut control, mut node) = HostedGraphNodeBuilder::new(opts)
                    .with_parameter_input(vec![HostedGraphParameterMapping {
                        graph_id: ParameterId(9001),
                        native_id: ClapId::new(1),
                    }])
                    .with_parameter_output(vec![HostedGraphParameterMapping {
                        graph_id: ParameterId(9100),
                        native_id: ClapId::new(2),
                    }])
                    .with_note_output(ravet_host::HostedGraphNoteOutput {
                        port_index: 1,
                        dialect,
                    })
                    .$prepare(instance_mode(mode), |_, _| ())
                    .unwrap();
                node.prepare(opts.configuration).unwrap();
                let mut node = std::thread::spawn(move || {
                    node.start_audio().unwrap();
                    let input = [$value as $sample; 8];
                    let inputs = [input.as_slice(); 2];
                    let mut output = [[0. as $sample; 8]; 2];
                    let mut storage = [parameter(0, 0.); 2];
                    let bad = capacity == 1 || matches!(mode, 31 | 34);
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            let mut outputs =
                                output.each_mut().map(|samples| samples.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            let changes = [parameter(0, 2.)];
                            let mut block =
                                ProcessBlock::new(audio, &changes, Transport::default());
                            block.output_events = Some(ravet_foundation::OutputEvents::new(
                                &mut storage[..capacity],
                                8,
                            ));
                            let status = node.process(&mut block);
                            if bad {
                                assert_eq!(
                                    status,
                                    ProcessStatus::Fault(ProcessError::BackendFailure)
                                );
                                assert!(block.output_events.as_ref().unwrap().is_empty());
                                assert_eq!(output, [[0. as $sample; 8]; 2]);
                            } else {
                                assert_eq!(status, ProcessStatus::Continue);
                                let events = block.output_events.as_ref().unwrap().as_slice();
                                assert_eq!(
                                    events[0],
                                    Event {
                                        frame_offset: 7,
                                        data: EventData::ParameterValue {
                                            id: ParameterId(9100),
                                            value: 2.,
                                        }
                                    }
                                );
                                assert_eq!(
                                    events.len(),
                                    if matches!(mode, 32 | 33) { 1 } else { 2 }
                                );
                                if events.len() == 2 {
                                    assert_eq!(
                                        events[1],
                                        Event {
                                            frame_offset: 7,
                                            data: EventData::Midi(
                                                ravet_foundation::MidiMessage::from_bytes(&[
                                                    0x92, 60, 64
                                                ])
                                                .unwrap()
                                            )
                                        }
                                    );
                                }
                                assert_eq!(output, [[($value * 2.) as $sample; 8]; 2]);
                            }
                        }
                        let inputs = [&[] as &[$sample]; 2];
                        let mut a = [];
                        let mut b = [];
                        let mut outputs = [a.as_mut_slice(), b.as_mut_slice()];
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 0).unwrap();
                        let changes = [parameter(0, 4.)];
                        let mut block = ProcessBlock::new(audio, &changes, Transport::default());
                        block.output_events = Some(ravet_foundation::OutputEvents::new(
                            &mut storage[..capacity],
                            0,
                        ));
                        let status = node.process(&mut block);
                        assert_eq!(
                            status,
                            if matches!(mode, 32 | 33) {
                                ProcessStatus::Continue
                            } else {
                                ProcessStatus::Fault(ProcessError::BackendFailure)
                            }
                        );
                        if matches!(mode, 32 | 33) {
                            assert_eq!(block.output_events.as_ref().unwrap().len(), 1);
                        } else {
                            assert!(block.output_events.as_ref().unwrap().is_empty());
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
                assert_eq!(stats.parameter_flushes, 1);
                assert_eq!(
                    stats.filtered_native_events,
                    if matches!(mode, 32 | 33) { 1001 } else { 0 }
                );
                assert_eq!(
                    stats.rejected_native_events,
                    if capacity == 1 || matches!(mode, 31 | 34) {
                        1001
                    } else if matches!(mode, 32 | 33) {
                        0
                    } else {
                        1
                    }
                );
                drop(node);
                let mut instance = control.into_instance().ok().unwrap();
                // Accepted native calls advance history even when output poisons the block.
                assert_eq!(
                    ravet_host::HostedParameters::new(&mut instance, 2)
                        .unwrap()
                        .value(ClapId::new(1))
                        .unwrap(),
                    4.
                );
            }
        }
    };
}
output_faults!(
    native_f32_performance_output_filters_faults_and_zero_flush_without_heap,
    f32,
    prepare,
    0.25
);
output_faults!(
    native_f64_performance_output_filters_faults_and_zero_flush_without_heap,
    f64,
    prepare64,
    1e200
);

#[test]
fn output_only_note_profile_validates_direction_and_does_not_require_parameter_mappings() {
    for (port_index, dialect, expected) in [
        (
            0,
            HostedGraphNoteDialect::Clap,
            HostedGraphNoteError::Dialect,
        ),
        (
            2,
            HostedGraphNoteDialect::Midi,
            HostedGraphNoteError::Unknown,
        ),
        (
            201,
            HostedGraphNoteDialect::Clap,
            HostedGraphNoteError::Unknown,
        ),
        (
            256,
            HostedGraphNoteDialect::Clap,
            HostedGraphNoteError::Capacity,
        ),
    ] {
        let mut opts = options(0);
        opts.configuration = opts.configuration.with_output_events(1);
        let error = HostedGraphNodeBuilder::new(opts)
            .with_note_output(ravet_host::HostedGraphNoteOutput {
                port_index,
                dialect,
            })
            .prepare(instance(), |_, _| panic!("before activation"))
            .err()
            .unwrap();
        assert_eq!(
            format!("{:?}", error.reason),
            format!("Notes({expected:?})")
        );
        assert!(!error.instance.is_active());
    }
    let mut opts = options(0);
    opts.configuration = opts.configuration.with_output_events(1);
    let (mut control, mut node) = HostedGraphNodeBuilder::new(opts)
        .with_note_output(ravet_host::HostedGraphNoteOutput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Midi,
        })
        .prepare(instance_mode(30), |_, _| ())
        .unwrap();
    node.prepare(opts.configuration).unwrap();
    let mut node = std::thread::spawn(move || {
        node.start_audio().unwrap();
        let input = [0.25; 8];
        let inputs = [input.as_slice(); 2];
        let mut output = [[0.; 8]; 2];
        let mut storage = [parameter(0, 0.)];
        let counts = allocation_counter::measure(|| {
            for _ in 0..1000 {
                let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
                let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                let mut block = ProcessBlock::new(audio, &[], Transport::default());
                block.output_events = Some(ravet_foundation::OutputEvents::new(&mut storage, 8));
                assert_eq!(node.process(&mut block), ProcessStatus::Continue);
                assert_eq!(
                    block.output_events.as_ref().unwrap().as_slice(),
                    &[Event {
                        frame_offset: 7,
                        data: EventData::Midi(
                            ravet_foundation::MidiMessage::from_bytes(&[0x92, 60, 64]).unwrap()
                        )
                    }]
                );
                assert_eq!(output, [[0.25; 8]; 2]);
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
    assert_eq!(control.statistics().filtered_native_events, 1000);
    assert_eq!(control.statistics().rejected_native_events, 0);
}

fn identified(offset: usize, phase: ravet_foundation::NotePhase, id: u32) -> Event {
    Event {
        frame_offset: offset,
        data: EventData::NoteWithId {
            id: ravet_foundation::NoteId(id),
            phase,
            channel: 2,
            key: 60,
            velocity: 0.5,
        },
    }
}
macro_rules! native_note_id {
    ($name:ident,$sample:ty,$prepare:ident) => {
        #[test]
        fn $name() {
            for (input_enabled, output_enabled) in [(false, false), (true, false), (true, true)] {
                let opts = HostedGraphNodeOptions {
                    configuration: options(2).configuration.with_output_events(2),
                    ..options(2)
                };
                let (mut control, mut node) = HostedGraphNodeBuilder::new(opts)
                    .with_note_input(HostedGraphNoteInput {
                        port_index: 1,
                        dialect: HostedGraphNoteDialect::Clap,
                    })
                    .with_note_output(ravet_host::HostedGraphNoteOutput {
                        port_index: 1,
                        dialect: HostedGraphNoteDialect::Clap,
                    })
                    .with_note_identity_input(input_enabled)
                    .with_note_identity_output(output_enabled)
                    .$prepare(instance_mode(45), |_, _| ())
                    .unwrap();
                node.prepare(opts.configuration).unwrap();
                let mut node = std::thread::spawn(move || {
                    node.start_audio().unwrap();
                    let source = [0.25 as $sample; 8];
                    let inputs = [source.as_slice(); 2];
                    let mut audio_output = [[0. as $sample; 8]; 2];
                    let mut storage = [parameter(0, 0.); 2];
                    let changes = [
                        identified(1, ravet_foundation::NotePhase::On, 123),
                        identified(6, ravet_foundation::NotePhase::Off, 123),
                    ];
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            let mut outputs = audio_output
                                .each_mut()
                                .map(|samples| samples.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            let mut block =
                                ProcessBlock::new(audio, &changes, Transport::default());
                            block.output_events =
                                Some(ravet_foundation::OutputEvents::new(&mut storage, 8));
                            if input_enabled {
                                assert_eq!(node.process(&mut block), ProcessStatus::Continue);
                                let events = block.output_events.as_ref().unwrap().as_slice();
                                if output_enabled {
                                    assert_eq!(events, &changes);
                                } else {
                                    assert!(events.is_empty());
                                }
                                assert_eq!(&audio_output[0][1..6], &[0.5 as $sample; 5]);
                                assert_eq!(&audio_output[0][6..], &[0. as $sample; 2]);
                            } else {
                                assert!(matches!(
                                    node.process(&mut block),
                                    ProcessStatus::Fault(ProcessError::Events(_))
                                ));
                                assert!(block.output_events.as_ref().unwrap().is_empty());
                                assert_eq!(audio_output, [[0. as $sample; 8]; 2]);
                            }
                        }
                        // A valid On prefix followed by an unrepresentable ID cannot start a voice.
                        let invalid = [
                            changes[0],
                            identified(6, ravet_foundation::NotePhase::Off, u32::MAX),
                        ];
                        let mut outputs = audio_output
                            .each_mut()
                            .map(|samples| samples.as_mut_slice());
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                        let mut block = ProcessBlock::new(audio, &invalid, Transport::default());
                        block.output_events =
                            Some(ravet_foundation::OutputEvents::new(&mut storage, 8));
                        assert!(matches!(
                            node.process(&mut block),
                            ProcessStatus::Fault(ProcessError::Events(_))
                        ));
                        // Empty followup retains the last native state, not the rejected On prefix.
                        let mut outputs = audio_output
                            .each_mut()
                            .map(|samples| samples.as_mut_slice());
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                        let mut block = ProcessBlock::new(audio, &[], Transport::default());
                        block.output_events =
                            Some(ravet_foundation::OutputEvents::new(&mut storage, 8));
                        assert_eq!(node.process(&mut block), ProcessStatus::Continue);
                        assert_eq!(
                            audio_output,
                            [[if input_enabled { 0. } else { 0.25 } as $sample; 8]; 2]
                        );
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
            }
        }
    };
}
native_note_id!(
    native_f32_identified_notes_keep_ids_and_pre_native_rejection,
    f32,
    prepare
);
native_note_id!(
    native_f64_identified_notes_keep_ids_and_pre_native_rejection,
    f64,
    prepare64
);

#[test]
fn identity_options_require_selected_clap_direction_before_activation() {
    for output in [false, true] {
        let builder =
            HostedGraphNodeBuilder::new(options(2)).with_note_input(HostedGraphNoteInput {
                port_index: 1,
                dialect: HostedGraphNoteDialect::Midi,
            });
        let builder = if output {
            builder.with_note_identity_output(true)
        } else {
            builder.with_note_identity_input(true)
        };
        let error = builder
            .prepare(instance(), |_, _| ())
            .err()
            .expect("explicit CLAP direction required");
        assert!(matches!(
            error.reason,
            ravet_host::HostedGraphNodeBuildFailure::Configuration
        ));
    }
}
