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
fn instance(mode: u8) -> PluginInstance<RequestHost> {
    fixture::set_output_mode(mode);
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<fixture::Fixture>>(c"").unwrap();
    PluginInstance::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.host-parameter-fixture",
        &HostInfo::new("Completion fixture", "RAVET", "", "0.1").unwrap(),
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
        configuration: ProcessConfig::new(48000., 8, 2, AudioLayout::STEREO)
            .unwrap()
            .with_output_events(capacity),
        declared_latency: 0,
        declared_tail: Tail::None,
    }
}
fn builder(capacity: usize, enabled: bool, mapped: bool) -> HostedGraphNodeBuilder {
    let b = HostedGraphNodeBuilder::new(options(capacity))
        .with_note_input(HostedGraphNoteInput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Clap,
        })
        .with_parameter_input(vec![HostedGraphParameterMapping {
            graph_id: ParameterId(17),
            native_id: ClapId::new(1),
        }])
        .with_note_end_output(enabled);
    if mapped {
        b.with_parameter_output(vec![HostedGraphParameterMapping {
            graph_id: ParameterId(18),
            native_id: ClapId::new(2),
        }])
    } else {
        b
    }
}
macro_rules! profiles {
    ($name:ident,$sample:ty,$prepare:ident) => {
        #[test]
        fn $name() {
            for (enabled, mapped, capacity) in [
                (true, true, 3),
                (false, true, 3),
                (true, false, 3),
                (true, false, 1),
            ] {
                let config = options(capacity).configuration;
                let (mut control, mut node) = builder(capacity, enabled, mapped)
                    .$prepare(instance(81), |_, _| ())
                    .unwrap();
                node.prepare(config).unwrap();
                let mut node = std::thread::spawn(move || {
                    node.start_audio().unwrap();
                    let input = [0.25 as $sample; 8];
                    let inputs = [input.as_slice(); 2];
                    let mut audio_output = [[0. as $sample; 8]; 2];
                    let seed = base(2.);
                    let mut storage = [seed; 3];
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            let mut outputs = audio_output.each_mut().map(|s| s.as_mut_slice());
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                            let changes = [seed];
                            let mut block =
                                ProcessBlock::new(audio, &changes, Transport::default());
                            block.output_events =
                                Some(OutputEvents::new(&mut storage[..capacity], 8));
                            let result = node.process(&mut block);
                            if capacity == 1 {
                                assert_eq!(
                                    result,
                                    ProcessStatus::Fault(ProcessError::BackendFailure)
                                );
                                assert!(block.output_events.as_ref().unwrap().is_empty());
                                assert_eq!(audio_output, [[0. as $sample; 8]; 2]);
                            } else {
                                assert_eq!(result, ProcessStatus::Continue);
                                let seen = block.output_events.as_ref().unwrap().as_slice();
                                let end = Event {
                                    frame_offset: 0,
                                    data: EventData::NoteEnd { target: target() },
                                };
                                let value = Event {
                                    frame_offset: 7,
                                    data: EventData::ParameterValue {
                                        id: ParameterId(18),
                                        value: 2.,
                                    },
                                };
                                if !enabled {
                                    assert_eq!(seen, &[value]);
                                } else if mapped {
                                    assert_eq!(
                                        seen,
                                        &[
                                            end,
                                            value,
                                            Event {
                                                frame_offset: 7,
                                                ..end
                                            }
                                        ]
                                    );
                                } else {
                                    assert_eq!(seen, &[end, end]);
                                }
                                assert_eq!(audio_output, [[0.5 as $sample; 8]; 2]);
                            }
                        }
                        // A controller cannot send completion back to the plug-in;
                        // the valid global prefix must not change saved state.
                        let invalid = [
                            base(4.),
                            Event {
                                frame_offset: 2,
                                data: EventData::NoteEnd { target: target() },
                            },
                        ];
                        let mut outputs = audio_output.each_mut().map(|s| s.as_mut_slice());
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                        let mut block = ProcessBlock::new(audio, &invalid, Transport::default());
                        block.output_events = Some(OutputEvents::new(&mut storage[..capacity], 8));
                        assert!(matches!(
                            node.process(&mut block),
                            ProcessStatus::Fault(ProcessError::Events(_))
                        ));
                        assert!(block.output_events.as_ref().unwrap().is_empty());
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
                let mut native = control.into_instance().ok().unwrap();
                assert_eq!(
                    ravet_host::HostedParameters::new(&mut native, 4)
                        .unwrap()
                        .value(ClapId::new(1))
                        .unwrap(),
                    2.
                );
            }
        }
    };
}
profiles!(
    f32_completion_input_endpoint_ignored_time_filter_capacity_and_heap,
    f32,
    prepare
);
profiles!(
    f64_completion_input_endpoint_ignored_time_filter_capacity_and_heap,
    f64,
    prepare64
);
#[test]
fn malformed_selected_completion_keeps_native_history_and_unselected_filters() {
    for (mode, enabled) in [(82, true), (82, false), (83, true), (84, true)] {
        let config = options(3).configuration;
        let (mut control, mut node) = builder(3, enabled, true)
            .prepare(instance(mode), |_, _| ())
            .unwrap();
        node.prepare(config).unwrap();
        let mut node = std::thread::spawn(move || {
            node.start_audio().unwrap();
            let x = [0.25; 8];
            let inputs = [x.as_slice(); 2];
            let mut y = [[0.; 8]; 2];
            let mut outputs = y.each_mut().map(|s| s.as_mut_slice());
            let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
            let events = [base(2.)];
            let mut storage = [base(0.); 3];
            let mut block = ProcessBlock::new(audio, &events, Transport::default());
            block.output_events = Some(OutputEvents::new(&mut storage, 8));
            let status = node.process(&mut block);
            if mode == 82 && enabled {
                assert_eq!(status, ProcessStatus::Fault(ProcessError::BackendFailure));
                assert!(block.output_events.as_ref().unwrap().is_empty());
                assert_eq!(y, [[0.; 8]; 2]);
            } else {
                assert_eq!(status, ProcessStatus::Continue);
                assert_eq!(block.output_events.as_ref().unwrap().len(), 1);
                assert_eq!(y, [[0.5; 8]; 2]);
            }
            node.stop_audio();
            node
        })
        .join()
        .unwrap();
        node.deactivate();
        assert!(control.reclaim());
        let mut native = control.into_instance().ok().unwrap();
        assert_eq!(
            ravet_host::HostedParameters::new(&mut native, 4)
                .unwrap()
                .value(ClapId::new(1))
                .unwrap(),
            2.
        );
    }
}
#[test]
fn completion_requires_clap_input_and_nonzero_output_capacity_before_activation() {
    for route in [
        None,
        Some(HostedGraphNoteInput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Midi,
        }),
    ] {
        let mut b = HostedGraphNodeBuilder::new(options(3)).with_note_end_output(true);
        if let Some(route) = route {
            b = b.with_note_input(route);
        }
        let error = b
            .prepare(instance(81), |_, _| ())
            .err()
            .expect("originating CLAP input required");
        assert!(matches!(
            error.reason,
            ravet_host::HostedGraphNodeBuildFailure::Configuration
        ));
    }
    assert!(
        builder(0, true, false)
            .prepare(instance(81), |_, _| ())
            .is_err()
    );
}
