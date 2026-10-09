#[path = "../examples/support/parameter_fixture.rs"]
mod fixture;
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, NoteExpression, NoteId, NotePhase, NoteTarget,
    OutputEvents, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor, Tail,
    Transport,
};
use ravet_host::{
    HostRequests, HostedGraphNodeBuilder, HostedGraphNodeOptions, HostedGraphNoteDialect,
    HostedGraphNoteInput, HostedGraphNoteOutput, RequestHost,
};
fn instance(mode: u8) -> PluginInstance<RequestHost> {
    fixture::set_output_mode(mode);
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<fixture::Fixture>>(c"").unwrap();
    PluginInstance::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.host-parameter-fixture",
        &HostInfo::new("Scoped expression fixture", "RAVET", "", "0.1").unwrap(),
    )
    .unwrap()
}
fn on() -> Event {
    Event {
        frame_offset: 0,
        data: EventData::NoteWithId {
            id: NoteId(123),
            phase: NotePhase::On,
            channel: 2,
            key: 60,
            velocity: 0.5,
        },
    }
}
fn expression(time: usize, id: u32, value: f64) -> Event {
    Event {
        frame_offset: time,
        data: EventData::NoteExpression {
            target: NoteTarget {
                channel: Some(2),
                key: Some(60),
                id: Some(NoteId(id)),
            },
            expression: NoteExpression::Pressure,
            value,
        },
    }
}
fn options(capacity: usize) -> HostedGraphNodeOptions {
    HostedGraphNodeOptions {
        configuration: ProcessConfig::new(48000., 8, 3, AudioLayout::STEREO)
            .unwrap()
            .with_output_events(capacity),
        declared_latency: 0,
        declared_tail: Tail::None,
    }
}
fn builder(capacity: usize) -> HostedGraphNodeBuilder {
    HostedGraphNodeBuilder::new(options(capacity))
        .with_note_input(HostedGraphNoteInput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Clap,
        })
        .with_note_output(HostedGraphNoteOutput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Clap,
        })
        .with_note_identity_input(true)
}
macro_rules! expression_profiles {
    ($name:ident,$sample:ty,$prepare:ident) => {
        #[test]
        fn $name() {
            for (input, output, capacity) in [
                (true, true, 2),
                (true, false, 2),
                (false, true, 2),
                (true, true, 1),
            ] {
                let opts = options(capacity);
                let (mut control, mut node) = builder(capacity)
                    .with_note_expression_input(input)
                    .with_note_expression_output(output)
                    .$prepare(instance(46), |_, _| ())
                    .unwrap();
                node.prepare(opts.configuration).unwrap();
                let mut node = std::thread::spawn(move || {
                    node.start_audio().unwrap();
                    let source = [0.25 as $sample; 8];
                    let inputs = [source.as_slice(); 2];
                    let mut audio_output = [[0. as $sample; 8]; 2];
                    let mut storage = [on(); 2];
                    let changes = [
                        on(),
                        expression(2, 123, 0.12345678912345678),
                        expression(5, 123, 0.8765432109876543),
                    ];
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            {
                                let mut outputs = audio_output.each_mut().map(|s| s.as_mut_slice());
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
                                    let observed = block.output_events.as_ref().unwrap().as_slice();
                                    if output {
                                        assert_eq!(observed, &changes[1..]);
                                    } else {
                                        assert!(observed.is_empty());
                                    }
                                }
                                if !input || (output && capacity == 1) {
                                    assert!(block.output_events.as_ref().unwrap().is_empty());
                                }
                            }
                            if input && (!output || capacity != 1) {
                                assert_eq!(&audio_output[0][..2], &[0.5 as $sample; 2]);
                                assert_eq!(
                                    &audio_output[0][2..5],
                                    &[(0.25 * (1. + 0.12345678912345678)) as $sample; 3]
                                );
                                assert_eq!(
                                    &audio_output[0][5..],
                                    &[(0.25 * (1. + 0.8765432109876543)) as $sample; 3]
                                );
                            }
                            if !input || (output && capacity == 1) {
                                assert_eq!(audio_output, [[0. as $sample; 8]; 2]);
                            }
                        }
                        // A valid On followed by a too-wide scoped ID cannot mutate native.
                        let invalid = [on(), expression(2, u32::MAX, 0.5)];
                        let mut outputs = audio_output.each_mut().map(|s| s.as_mut_slice());
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                        let mut block = ProcessBlock::new(audio, &invalid, Transport::default());
                        block.output_events = Some(OutputEvents::new(&mut storage[..capacity], 8));
                        assert!(matches!(
                            node.process(&mut block),
                            ProcessStatus::Fault(ProcessError::Events(_))
                        ));
                        let mut outputs = audio_output.each_mut().map(|s| s.as_mut_slice());
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                        let mut block = ProcessBlock::new(audio, &[], Transport::default());
                        block.output_events = Some(OutputEvents::new(&mut storage[..capacity], 8));
                        assert_eq!(node.process(&mut block), ProcessStatus::Continue);
                        assert_eq!(
                            audio_output,
                            [[if input {
                                (0.25 * (1. + 0.8765432109876543)) as $sample
                            } else {
                                0.25 as $sample
                            }; 8]; 2]
                        );
                        let inputs = [&[] as &[$sample]; 2];
                        let mut a = [];
                        let mut b = [];
                        let mut outputs = [a.as_mut_slice(), b.as_mut_slice()];
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 0).unwrap();
                        let changes = [expression(0, 123, 0.5)];
                        let mut block = ProcessBlock::new(audio, &changes, Transport::default());
                        block.output_events = Some(OutputEvents::new(&mut storage[..capacity], 0));
                        assert!(matches!(
                            node.process(&mut block),
                            ProcessStatus::Fault(ProcessError::Events(_))
                        ));
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
expression_profiles!(
    native_f32_expression_scope_precision_timing_filter_capacity_and_atomic_input,
    f32,
    prepare
);
expression_profiles!(
    native_f64_expression_scope_precision_timing_filter_capacity_and_atomic_input,
    f64,
    prepare64
);

#[test]
fn selected_output_faults_keep_native_history_while_unknown_or_other_port_filters() {
    for (mode, enabled) in [(47, true), (47, false), (48, true), (49, true)] {
        let opts = options(2);
        let (mut control, mut node) = builder(2)
            .with_note_expression_output(enabled)
            .prepare(instance(mode), |_, _| ())
            .unwrap();
        node.prepare(opts.configuration).unwrap();
        let mut node = std::thread::spawn(move || {
            node.start_audio().unwrap();
            let input = [0.25; 8];
            let inputs = [input.as_slice(); 2];
            let mut output = [[0.; 8]; 2];
            let mut storage = [on(); 2];
            let changes = [on()];
            let mut outputs = output.each_mut().map(|s| s.as_mut_slice());
            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
            let mut block = ProcessBlock::new(audio, &changes, Transport::default());
            block.output_events = Some(OutputEvents::new(&mut storage, 8));
            let status = node.process(&mut block);
            assert!(block.output_events.as_ref().unwrap().is_empty());
            if mode == 47 && enabled {
                assert_eq!(status, ProcessStatus::Fault(ProcessError::BackendFailure));
                assert_eq!(output, [[0.; 8]; 2]);
            } else {
                assert_eq!(status, ProcessStatus::Continue);
                assert_eq!(output, [[0.5; 8]; 2]);
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
            2.
        );
    }
}
#[test]
fn expression_direction_requires_explicit_clap_port_before_activation() {
    for input in [false, true] {
        let builder =
            HostedGraphNodeBuilder::new(options(2)).with_note_input(HostedGraphNoteInput {
                port_index: 1,
                dialect: HostedGraphNoteDialect::Midi,
            });
        let builder = if input {
            builder.with_note_expression_input(true)
        } else {
            builder.with_note_expression_output(true)
        };
        let error = builder
            .prepare(instance(46), |_, _| ())
            .err()
            .expect("selected CLAP direction");
        assert!(matches!(
            error.reason,
            ravet_host::HostedGraphNodeBuildFailure::Configuration
        ));
    }
}
