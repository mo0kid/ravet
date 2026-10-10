use clack_host::prelude::*;
use clack_plugin::{
    events::{
        UnknownEvent,
        event_types::{MidiEvent, ParamValueEvent},
        io::{EventBuffer, OutputEventBuffer, TryPushError},
    },
    process::Audio,
};
use ravet_clap::{MainBusBridge, OutputRejection};
use ravet_foundation::{
    AudioLayout, Event, EventData, MidiMessage, ParameterId, ProcessBlock, ProcessConfig,
    ProcessStatus as Status, Processor,
};
struct Emits {
    config: Option<ProcessConfig>,
    callbacks: usize,
    reserved: bool,
    fault: bool,
}
impl Processor for Emits {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
        self.config = Some(config);
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.config = None;
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> Status {
        if let Err(error) = self.config.unwrap().validate_block(block) {
            block.clear_outputs();
            return Status::Fault(error);
        }
        block.audio.output_mut(0).unwrap().fill(0.25);
        self.callbacks += 1;
        let offset = if block.audio.frames() == 0 { 0 } else { 1 };
        let data = [
            EventData::ParameterValue {
                id: ParameterId(if self.reserved { u32::MAX } else { 7 }),
                value: 0.5,
            },
            EventData::NoteOn {
                channel: 2,
                key: 60,
                velocity: 0.001,
            },
            EventData::Midi(MidiMessage::from_bytes(&[0xc2, 10]).unwrap()),
            EventData::NoteOff {
                channel: 2,
                key: 60,
                velocity: 0.5,
            },
        ];
        for (index, data) in data.into_iter().enumerate() {
            block
                .output_events
                .as_mut()
                .unwrap()
                .push(Event {
                    frame_offset: if index == 0 { 0 } else { offset },
                    data,
                })
                .unwrap();
        }
        if self.fault {
            block.clear_outputs();
            Status::Fault(ravet_foundation::ProcessError::OutputEventCapacity)
        } else {
            Status::Continue
        }
    }
}
struct Queue {
    events: EventBuffer,
    limit: u32,
    attempts: usize,
}
impl OutputEventBuffer for Queue {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        self.attempts += 1;
        if self.events.len() == self.limit {
            return Err(TryPushError::new());
        }
        self.events.push(event);
        Ok(())
    }
}
fn make_bridge(port: Option<u16>, reserved: bool) -> MainBusBridge<Emits> {
    MainBusBridge::prepare_with_output(
        Emits {
            config: None,
            callbacks: 0,
            reserved,
            fault: false,
        },
        ProcessConfig::new(48000., 8, 4, AudioLayout::MONO_INSTRUMENT)
            .unwrap()
            .with_output_events(4),
        None,
        port,
    )
    .unwrap()
}
fn audio(samples: &mut [f32], run: impl FnOnce(&mut Audio<'_>)) {
    let mut ports = AudioPorts::with_capacity(1, 1);
    let buffers = ports.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_output_only([samples]),
    }]);
    run(&mut buffers.to_plugin_audio());
}

#[test]
fn generated_gestures_map_without_midi_ports_and_preserve_prefix_rejection() {
    use clack_plugin::events::event_types::{ParamGestureBeginEvent, ParamGestureEndEvent};
    use ravet_foundation::ParameterGesturePhase;
    struct Gestures(bool);
    impl Processor for Gestures {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
            Ok(())
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {}
        fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> Status {
            block.clear_outputs();
            let last = block.audio.frames().saturating_sub(1);
            let id = ParameterId(if self.0 { u32::MAX } else { 7 });
            for (frame_offset, data) in [
                (
                    0,
                    EventData::ParameterGesture {
                        id,
                        phase: ParameterGesturePhase::Begin,
                    },
                ),
                (last, EventData::ParameterValue { id, value: 0.5 }),
                (
                    last,
                    EventData::ParameterGesture {
                        id,
                        phase: ParameterGesturePhase::End,
                    },
                ),
            ] {
                block
                    .output_events
                    .as_mut()
                    .unwrap()
                    .push(Event { frame_offset, data })
                    .unwrap();
            }
            Status::Continue
        }
    }
    for (reserved, limit) in [(false, 3), (false, 2), (true, 3)] {
        let mut bridge = MainBusBridge::prepare_with_output(
            Gestures(reserved),
            ProcessConfig::new(48000., 8, 0, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_output_events(3),
            None,
            None,
        )
        .unwrap();
        let mut queue = Queue {
            events: EventBuffer::with_capacity(3),
            limit,
            attempts: 0,
        };
        let mut samples = [0.; 8];
        audio(&mut samples, |audio| {
            let report = bridge
                .process_with_output(
                    audio,
                    &InputEvents::empty(),
                    None,
                    &mut OutputEvents::from_buffer(&mut queue),
                )
                .unwrap();
            assert_eq!(
                report.delivery.delivered,
                if reserved { 0 } else { limit as usize }
            );
            assert_eq!(
                report.delivery.reason,
                if reserved {
                    Some(OutputRejection::Unrepresentable)
                } else if limit == 2 {
                    Some(OutputRejection::HostRejected)
                } else {
                    None
                }
            );
        });
        if !reserved {
            let begin = queue
                .events
                .get(0)
                .unwrap()
                .as_event::<ParamGestureBeginEvent>()
                .unwrap();
            assert_eq!(begin.param_id(), Some(ClapId::new(7)));
            assert_eq!(queue.events.get(0).unwrap().header().time(), 0);
            if limit == 3 {
                let end = queue
                    .events
                    .get(2)
                    .unwrap()
                    .as_event::<ParamGestureEndEvent>()
                    .unwrap();
                assert_eq!(end.param_id(), Some(ClapId::new(7)));
                assert_eq!(queue.events.get(2).unwrap().header().time(), 7);
            }
        } else {
            assert_eq!(queue.attempts, 0);
        }
    }
}
#[test]
fn timed_parameter_notes_and_short_midi_preserve_order_port_padding_and_quantization() {
    let mut bridge = make_bridge(Some(3), false);
    let mut queue = Queue {
        events: EventBuffer::with_capacity(4),
        limit: 4,
        attempts: 0,
    };
    let mut samples = [99.; 8];
    audio(&mut samples, |audio| {
        let report = bridge
            .process_with_output(
                audio,
                &InputEvents::empty(),
                None,
                &mut OutputEvents::from_buffer(&mut queue),
            )
            .unwrap();
        assert_eq!(
            (
                report.delivery.delivered,
                report.delivery.undelivered,
                report.delivery.reason
            ),
            (4, 0, None)
        );
    });
    assert_eq!(samples, [0.25; 8]);
    assert_eq!(bridge.processor().callbacks, 1);
    let value = queue
        .events
        .get(0)
        .unwrap()
        .as_event::<ParamValueEvent>()
        .unwrap();
    assert_eq!(value.param_id(), Some(ClapId::new(7)));
    assert_eq!(value.value(), 0.5);
    for (index, bytes) in [(1, [0x92, 60, 1]), (2, [0xc2, 10, 0]), (3, [0x82, 60, 64])] {
        let event = queue.events.get(index).unwrap();
        assert_eq!(event.header().time(), 1);
        let midi = event.as_event::<MidiEvent>().unwrap();
        assert_eq!(midi.port_index(), 3);
        assert_eq!(midi.data(), bytes);
    }
    queue.events.clear();
    queue.attempts = 0;
    let report = bridge
        .flush_with_output(
            &InputEvents::empty(),
            &mut OutputEvents::from_buffer(&mut queue),
        )
        .unwrap();
    assert_eq!(report.delivery.delivered, 4);
    for event in queue.events.as_input().iter() {
        assert_eq!(event.header().time(), 0);
    }
}
#[test]
fn host_prefix_rejection_and_unrepresentable_whole_stream_preserve_audio_without_retry() {
    let mut bridge = make_bridge(Some(0), false);
    let mut queue = Queue {
        events: EventBuffer::with_capacity(4),
        limit: 2,
        attempts: 0,
    };
    let mut samples = [99.; 8];
    audio(&mut samples, |audio| {
        let report = bridge
            .process_with_output(
                audio,
                &InputEvents::empty(),
                None,
                &mut OutputEvents::from_buffer(&mut queue),
            )
            .unwrap();
        assert_eq!(
            (
                report.delivery.delivered,
                report.delivery.undelivered,
                report.delivery.reason
            ),
            (2, 2, Some(OutputRejection::HostRejected))
        );
    });
    assert_eq!(queue.attempts, 3);
    assert_eq!(samples, [0.25; 8]);
    for (port, reserved) in [(None, false), (Some(0), true)] {
        let mut bridge = make_bridge(port, reserved);
        queue.events.clear();
        queue.attempts = 0;
        audio(&mut samples, |audio| {
            let report = bridge
                .process_with_output(
                    audio,
                    &InputEvents::empty(),
                    None,
                    &mut OutputEvents::from_buffer(&mut queue),
                )
                .unwrap();
            assert_eq!(
                (
                    report.delivery.delivered,
                    report.delivery.undelivered,
                    report.delivery.reason
                ),
                (0, 4, Some(OutputRejection::Unrepresentable))
            );
        });
        assert_eq!(queue.attempts, 0);
        assert_eq!(samples, [0.25; 8]);
        assert_eq!(bridge.processor().callbacks, 1);
    }
}
#[test]
fn rejected_input_emits_nothing_and_prepared_delivery_does_not_allocate() {
    let mut bridge = make_bridge(Some(0), false);
    let mut queue = Queue {
        events: EventBuffer::with_capacity(4),
        limit: 4,
        attempts: 0,
    };
    let mut samples = [99.; 8];
    audio(&mut samples, |audio| {
        assert!(
            bridge
                .process_with_output(
                    audio,
                    &InputEvents::from_buffer(&[MidiEvent::new(8, 0, [0x90, 60, 127])]),
                    None,
                    &mut OutputEvents::from_buffer(&mut queue)
                )
                .is_err()
        );
    });
    assert_eq!(samples, [0.; 8]);
    assert_eq!(bridge.processor().callbacks, 0);
    assert_eq!(queue.attempts, 0);
    let mut ports = AudioPorts::with_capacity(1, 1);
    let buffers = ports.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
    }]);
    let mut audio = buffers.to_plugin_audio();
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            queue.events.clear();
            let report = bridge
                .process_with_output(
                    &mut audio,
                    &InputEvents::empty(),
                    None,
                    &mut OutputEvents::from_buffer(&mut queue),
                )
                .unwrap();
            assert_eq!(report.delivery.delivered, 4);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}

#[test]
fn dsp_fault_discards_generated_events_and_old_api_rejects_nonzero_output_before_dsp() {
    let config = ProcessConfig::new(48000., 8, 4, AudioLayout::MONO_INSTRUMENT)
        .unwrap()
        .with_output_events(4);
    assert!(
        MainBusBridge::prepare(
            Emits {
                config: None,
                callbacks: 0,
                reserved: false,
                fault: false
            },
            config,
            None
        )
        .is_err()
    );
    let mut bridge = MainBusBridge::prepare_with_output(
        Emits {
            config: None,
            callbacks: 0,
            reserved: false,
            fault: true,
        },
        config,
        None,
        Some(0),
    )
    .unwrap();
    let mut queue = Queue {
        events: EventBuffer::with_capacity(4),
        limit: 4,
        attempts: 0,
    };
    let mut samples = [99.; 8];
    audio(&mut samples, |audio| {
        assert!(bridge.process(audio, &InputEvents::empty(), None).is_err())
    });
    assert_eq!(bridge.processor().callbacks, 0);
    assert_eq!(samples, [0.; 8]);
    audio(&mut samples, |audio| {
        assert!(
            bridge
                .process_with_output(
                    audio,
                    &InputEvents::empty(),
                    None,
                    &mut OutputEvents::from_buffer(&mut queue)
                )
                .is_err()
        )
    });
    assert_eq!(bridge.processor().callbacks, 1);
    assert_eq!(queue.attempts, 0);
    assert_eq!(samples, [0.; 8]);
}

struct EchoUmp(Option<ProcessConfig>);
impl Processor for EchoUmp {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
        self.0 = Some(config);
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.0 = None;
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> Status {
        if let Err(error) = self.0.unwrap().validate_block(block) {
            block.clear_outputs();
            return Status::Fault(error);
        }
        block.clear_outputs();
        for event in block.events {
            block.output_events.as_mut().unwrap().push(*event).unwrap();
        }
        Status::Continue
    }
}
#[test]
fn ump_all_packet_sizes_cross_the_prepared_bridge_without_quantization_or_allocation() {
    use clack_plugin::events::{Event as _, event_types::Midi2Event};
    use ravet_foundation::UmpPacket;
    let config = ProcessConfig::new(48000., 8, 16, AudioLayout::MONO_INSTRUMENT)
        .unwrap()
        .with_output_events(16);
    let mut bridge =
        MainBusBridge::prepare_with_output(EchoUmp(None), config, Some(2), Some(3)).unwrap();
    let input: Vec<_> = (0..16)
        .map(|kind| {
            Midi2Event::new(
                1,
                2,
                [kind << 28 | 0x0fabcdef, 0x12345678, 0x87654321, u32::MAX],
            )
        })
        .collect();
    let mut queue = Queue {
        events: EventBuffer::with_capacity(16),
        limit: 16,
        attempts: 0,
    };
    let mut samples = [99.; 8];
    let mut ports = AudioPorts::with_capacity(1, 1);
    let buffers = ports.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
    }]);
    let mut audio = buffers.to_plugin_audio();
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            queue.events.clear();
            let report = bridge
                .process_with_output(
                    &mut audio,
                    &InputEvents::from_buffer(&input),
                    None,
                    &mut OutputEvents::from_buffer(&mut queue),
                )
                .unwrap();
            assert_eq!(report.delivery.delivered, 16);
            for (index, original) in input.iter().enumerate() {
                let event = queue
                    .events
                    .get(index as u32)
                    .unwrap()
                    .as_event::<Midi2Event>()
                    .unwrap();
                assert_eq!(event.port_index(), 3);
                assert_eq!(event.header().time(), 1);
                let count = UmpPacket::word_count(original.data()[0]);
                assert_eq!(&event.data()[..count], &original.data()[..count]);
                assert!(event.data()[count..].iter().all(|word| *word == 0));
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    let mut translator = ravet_clap::ParameterEvents::with_capacity(16).unwrap();
    assert!(
        translator
            .translate_midi(&InputEvents::from_buffer(&input), 8, 2)
            .unwrap()
            .is_empty()
    );
    assert!(
        translator
            .translate_midi_ump(&InputEvents::from_buffer(&input), 8, 1)
            .unwrap()
            .is_empty()
    );
    assert!(
        translator
            .translate(&InputEvents::from_buffer(&input), 0, true)
            .unwrap()
            .is_empty()
    );
    assert!(
        translator
            .translate_midi_ump(
                &InputEvents::from_buffer(&[Midi2Event::new(8, 9, [0; 4])]),
                8,
                2
            )
            .is_err()
    );
}

#[test]
fn generated_global_modulation_maps_signed_amounts_and_host_prefix_without_midi_ports() {
    use clack_plugin::events::Event as _;
    use clack_plugin::events::event_types::ParamModEvent;
    struct Modulation;
    impl Processor for Modulation {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
            Ok(())
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {}
        fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> Status {
            block.clear_outputs();
            for (frame_offset, amount) in [(0, -3.), (7, 0.5)] {
                block
                    .output_events
                    .as_mut()
                    .unwrap()
                    .push(Event {
                        frame_offset,
                        data: EventData::ParameterModulation {
                            id: ParameterId(7),
                            amount,
                        },
                    })
                    .unwrap();
            }
            Status::Continue
        }
    }
    for limit in [1, 2] {
        let mut bridge = MainBusBridge::prepare_with_output(
            Modulation,
            ProcessConfig::new(48000., 8, 0, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_output_events(2),
            None,
            None,
        )
        .unwrap();
        let mut queue = Queue {
            events: EventBuffer::with_capacity(2),
            limit,
            attempts: 0,
        };
        let mut samples = [99.; 8];
        let mut ports = AudioPorts::with_capacity(1, 1);
        let buffers = ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
        }]);
        let mut audio = buffers.to_plugin_audio();
        let allocations = allocation_counter::measure(|| {
            for _ in 0..1000 {
                queue.events.clear();
                let report = bridge
                    .process_with_output(
                        &mut audio,
                        &InputEvents::empty(),
                        None,
                        &mut OutputEvents::from_buffer(&mut queue),
                    )
                    .unwrap();
                assert_eq!(report.delivery.delivered, limit as usize);
                assert_eq!(
                    report.delivery.reason,
                    if limit == 1 {
                        Some(OutputRejection::HostRejected)
                    } else {
                        None
                    }
                );
                let event = queue
                    .events
                    .get(0)
                    .unwrap()
                    .as_event::<ParamModEvent>()
                    .unwrap();
                assert_eq!(event.param_id(), Some(ClapId::new(7)));
                assert_eq!(event.pckn(), Pckn::match_all());
                assert_eq!(event.amount(), -3.);
                if limit == 2 {
                    let event = queue
                        .events
                        .get(1)
                        .unwrap()
                        .as_event::<ParamModEvent>()
                        .unwrap();
                    assert_eq!(event.amount(), 0.5);
                    assert_eq!(event.header().time(), 7);
                }
            }
        });
        assert_eq!(allocations.count_total, 0);
        assert_eq!(samples, [0.; 8]);
    }
}

#[test]
fn generated_midi_profile_never_discards_note_identity_scope_or_delivers_a_prefix() {
    struct Identified {
        id: u32,
        kind: u8,
        callbacks: usize,
    }
    impl Processor for Identified {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
            Ok(())
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {}
        fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> Status {
            block.audio.output_mut(0).unwrap().fill(0.375);
            self.callbacks += 1;
            let output = block.output_events.as_mut().unwrap();
            output
                .push(Event {
                    frame_offset: 0,
                    data: EventData::ParameterValue {
                        id: ParameterId(7),
                        value: 0.5,
                    },
                })
                .unwrap();
            output
                .push(Event {
                    frame_offset: 7,
                    data: match self.kind {
                        1 => EventData::NoteExpression {
                            target: ravet_foundation::NoteTarget::default(),
                            expression: ravet_foundation::NoteExpression::Pressure,
                            value: 0.12345678912345678,
                        },
                        2 => EventData::NoteOffScoped {
                            target: ravet_foundation::NoteTarget::default(),
                            velocity: 0.5,
                        },
                        3 => EventData::NoteChoke {
                            target: ravet_foundation::NoteTarget::default(),
                        },
                        4 | 5 => EventData::NoteExpression {
                            target: ravet_foundation::NoteTarget::default(),
                            expression: ravet_foundation::NoteExpression::Volume,
                            value: if self.kind == 4 { 0. } else { -0. },
                        },
                        _ => EventData::NoteWithId {
                            id: ravet_foundation::NoteId(self.id),
                            phase: ravet_foundation::NotePhase::On,
                            channel: 2,
                            key: 60,
                            velocity: 0.5,
                        },
                    },
                })
                .unwrap();
            Status::Continue
        }
    }
    for (id, kind) in [
        (0, 0),
        (123, 0),
        (u32::MAX, 0),
        (123, 1),
        (123, 2),
        (123, 3),
        (123, 4),
        (123, 5),
    ] {
        let mut bridge = MainBusBridge::prepare_with_output(
            Identified {
                id,
                kind,
                callbacks: 0,
            },
            ProcessConfig::new(48000., 8, 0, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_output_events(2),
            None,
            Some(0),
        )
        .unwrap()
        .with_clap_note_output_port((kind >= 4).then_some(1))
        .unwrap();
        let mut queue = Queue {
            events: EventBuffer::with_capacity(2),
            limit: 2,
            attempts: 0,
        };
        let mut samples = [0.; 8];
        audio(&mut samples, |audio| {
            let counts = allocation_counter::measure(|| {
                for _ in 0..1000 {
                    let report = bridge
                        .process_with_output(
                            audio,
                            &InputEvents::empty(),
                            None,
                            &mut OutputEvents::from_buffer(&mut queue),
                        )
                        .unwrap();
                    assert_eq!(
                        report.delivery,
                        ravet_clap::OutputDelivery {
                            delivered: 0,
                            undelivered: 2,
                            reason: Some(OutputRejection::Unrepresentable)
                        }
                    );
                }
            });
            assert_eq!(counts.count_total, 0);
        });
        assert_eq!(samples, [0.375; 8]);
        assert_eq!(bridge.processor().callbacks, 1000);
        assert_eq!(queue.attempts, 0);
    }
}
