use clack_plugin::{
    events::{
        Event as ClapEvent, Match, Pckn, UnknownEvent,
        event_types::{
            NoteExpressionEvent, NoteExpressionType, NoteOffEvent, NoteOnEvent, ParamValueEvent,
        },
        io::{EventBuffer, InputEvents, OutputEventBuffer, OutputEvents, TryPushError},
    },
    utils::ClapId,
};
use ravet_clap::{BridgeError, MainBusBridge, MultiBusBridge, OutputRejection, ParameterEvents};
use ravet_foundation::{
    AudioLayout, Event, EventData, NoteExpression, NoteId, NotePhase, NoteTarget, ParameterId,
    ProcessBlock, ProcessConfig, ProcessStatus, Processor,
};
fn expr(time: u32, kind: NoteExpressionType, value: f64) -> NoteExpressionEvent {
    NoteExpressionEvent::new(time, Pckn::new(1u16, Match::All, 127u16, 0u32), kind, value)
}
#[test]
fn translator_preserves_all_expression_types_and_scopes_with_default_filtering() {
    let kinds = [
        (NoteExpressionType::Volume, NoteExpression::Volume, 4.),
        (
            NoteExpressionType::Pan,
            NoteExpression::Pan,
            0.12345678912345678,
        ),
        (
            NoteExpressionType::Tuning,
            NoteExpression::Tuning,
            -119.12345678912345,
        ),
        (NoteExpressionType::Vibrato, NoteExpression::Vibrato, 1.),
        (
            NoteExpressionType::Expression,
            NoteExpression::Expression,
            0.5,
        ),
        (
            NoteExpressionType::Brightness,
            NoteExpression::Brightness,
            0.,
        ),
        (
            NoteExpressionType::Pressure,
            NoteExpression::Pressure,
            0.8765432109876543,
        ),
    ];
    let mut native = EventBuffer::with_capacity(9);
    native.push(&NoteOnEvent::new(0, Pckn::new(1u16, 2u16, 60u16, 0u32), 0.));
    for (kind, _, value) in kinds {
        native.push(&expr(2, kind, value));
    }
    native.push(&NoteOffEvent::new(
        7,
        Pckn::new(1u16, 2u16, 60u16, i32::MAX as u32),
        0.5,
    ));
    let input = InputEvents::from_buffer(&native);
    let mut disabled = ParameterEvents::with_capacity(9).unwrap();
    assert!(disabled.translate(&input, 8, false).unwrap().is_empty());
    let mut enabled = ParameterEvents::with_capacity(9)
        .unwrap()
        .with_clap_note_input_port(Some(1))
        .unwrap();
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            let events = enabled.translate(&input, 8, false).unwrap();
            assert_eq!(
                events[0].data,
                EventData::NoteWithId {
                    id: NoteId(0),
                    phase: NotePhase::On,
                    channel: 2,
                    key: 60,
                    velocity: 0.
                }
            );
            for (event, (_, expression, value)) in events[1..8].iter().zip(kinds) {
                assert_eq!(
                    *event,
                    Event {
                        frame_offset: 2,
                        data: EventData::NoteExpression {
                            target: NoteTarget {
                                channel: None,
                                key: Some(127),
                                id: Some(NoteId(0))
                            },
                            expression,
                            value
                        }
                    }
                );
            }
            assert_eq!(events[8].frame_offset, 7);
            assert_eq!(
                events[8].data,
                EventData::NoteWithId {
                    id: NoteId(i32::MAX as u32),
                    phase: NotePhase::Off,
                    channel: 2,
                    key: 60,
                    velocity: 0.5
                }
            );
            assert!(enabled.translate(&input, 0, true).unwrap().is_empty());
        }
    });
    assert_eq!(counts.count_total, 0);
    assert!(
        ParameterEvents::new()
            .with_clap_note_input_port(Some(256))
            .is_err()
    );
}
#[test]
fn selected_invalid_metadata_timing_and_zero_frames_reject_unselected_kinds_filter() {
    let good = expr(0, NoteExpressionType::Pressure, 0.5);
    let mut invalid = [*good.as_raw(); 7];
    invalid[0].channel = -2;
    invalid[1].channel = 16;
    invalid[2].key = -2;
    invalid[3].key = 128;
    invalid[4].note_id = -2;
    invalid[5].value = f64::NAN;
    invalid[6].value = 1.01;
    let mut selected = ParameterEvents::with_capacity(2)
        .unwrap()
        .with_clap_note_input_port(Some(1))
        .unwrap();
    for mut raw in invalid {
        let mut native = EventBuffer::with_capacity(2);
        native.push(&ParamValueEvent::new(
            0,
            ClapId::new(7),
            Pckn::match_all(),
            0.5,
        ));
        native.push(&NoteExpressionEvent::from_raw(&raw));
        assert_eq!(
            selected.translate(&InputEvents::from_buffer(&native), 8, false),
            Err(BridgeError::InvalidEvent)
        );
        raw.port_index = 0;
        let other = [NoteExpressionEvent::from_raw(&raw)];
        assert!(
            selected
                .translate(&InputEvents::from_buffer(&other), 8, false)
                .unwrap()
                .is_empty()
        );
        raw.port_index = 1;
        raw.expression_id = 99;
        let unknown = [NoteExpressionEvent::from_raw(&raw)];
        assert!(
            selected
                .translate(&InputEvents::from_buffer(&unknown), 8, false)
                .unwrap()
                .is_empty()
        );
    }
    for times in [[4, 3], [0, 8]] {
        let native = times.map(|t| expr(t, NoteExpressionType::Pressure, 0.5));
        for translator in [
            &mut selected,
            &mut ParameterEvents::with_capacity(2).unwrap(),
        ] {
            assert_eq!(
                translator.translate(&InputEvents::from_buffer(&native), 8, false),
                Err(BridgeError::InvalidEvent)
            );
        }
    }
    let zero = [good];
    assert_eq!(
        selected.translate(&InputEvents::from_buffer(&zero), 0, false),
        Err(BridgeError::InvalidEvent)
    );
    let wildcard_off = [NoteOffEvent::new(
        0,
        Pckn::new(1u16, Match::All, Match::All, Match::All),
        0.5,
    )];
    assert_eq!(
        selected
            .translate(&InputEvents::from_buffer(&wildcard_off), 8, false)
            .unwrap(),
        &[Event {
            frame_offset: 0,
            data: EventData::NoteOffScoped {
                target: NoteTarget::default(),
                velocity: 0.5
            }
        }]
    );
    let wildcard_on = [NoteOnEvent::new(
        0,
        Pckn::new(1u16, Match::All, 60u16, Match::All),
        0.5,
    )];
    assert_eq!(
        selected.translate(&InputEvents::from_buffer(&wildcard_on), 8, false),
        Err(BridgeError::InvalidEvent)
    );
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
struct Echo {
    calls: usize,
    forced: bool,
    wide: bool,
}
macro_rules! processor {
    ($sample:ty) => {
        impl Processor<$sample> for Echo {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
                Ok(())
            }
            fn reset(&mut self) {}
            fn deactivate(&mut self) {}
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                self.calls += 1;
                block.audio.output_mut(0).unwrap().fill(0.375 as $sample);
                if let Some(buses) = block.auxiliary.as_deref_mut() {
                    for bus in buses {
                        for channel in 0..bus.output_channels() {
                            bus.output_mut(channel).unwrap().fill(0.);
                        }
                    }
                }
                if self.forced {
                    let last = block.audio.frames().saturating_sub(1);
                    for (frame_offset, data) in [
                        (
                            0,
                            EventData::ParameterValue {
                                id: ParameterId(7),
                                value: 0.5,
                            },
                        ),
                        (
                            last,
                            EventData::NoteWithId {
                                id: NoteId(if self.wide { u32::MAX } else { 0 }),
                                phase: NotePhase::On,
                                channel: 2,
                                key: 60,
                                velocity: 0.001,
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
                } else {
                    for event in block.events {
                        block.output_events.as_mut().unwrap().push(*event).unwrap();
                    }
                }
                ProcessStatus::Continue
            }
        }
    };
}
processor!(f32);
processor!(f64);
macro_rules! native_profile {
    ($name:ident,$sample:ty,$channels:ident) => {
        #[test]
        fn $name() {
            use clack_host::prelude::{AudioPortBuffer, AudioPortBufferType, AudioPorts};
            let mut bridge = MainBusBridge::<_, $sample>::prepare_with_output(
                Echo {
                    calls: 0,
                    forced: false,
                    wide: false,
                },
                ProcessConfig::new(48000., 8, 4, AudioLayout::MONO_INSTRUMENT)
                    .unwrap()
                    .with_output_events(4),
                None,
                None,
            )
            .unwrap()
            .with_clap_note_input_port(Some(1))
            .unwrap()
            .with_clap_note_output_port(Some(2))
            .unwrap();
            let mut native = EventBuffer::with_capacity(4);
            native.push(&ParamValueEvent::new(
                0,
                ClapId::new(7),
                Pckn::match_all(),
                0.5,
            ));
            native.push(&NoteOnEvent::new(
                1,
                Pckn::new(1u16, 2u16, 60u16, 0u32),
                0.001,
            ));
            native.push(&expr(
                2,
                NoteExpressionType::Expression,
                0.12345678912345678,
            ));
            native.push(&NoteOffEvent::new(
                7,
                Pckn::new(1u16, 2u16, 60u16, i32::MAX as u32),
                0.5,
            ));
            let mut samples = [0. as $sample; 8];
            {
                let mut ports = AudioPorts::with_capacity(1, 1);
                let buffers = ports.with_output_buffers([AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::$channels([samples.as_mut_slice()]),
                }]);
                let mut audio = buffers.to_plugin_audio();
                let mut queue = Queue {
                    events: EventBuffer::with_capacity(4),
                    limit: 4,
                    attempts: 0,
                };
                let counts = allocation_counter::measure(|| {
                    for _ in 0..1000 {
                        queue.events.clear();
                        let report = bridge
                            .process_with_output(
                                &mut audio,
                                &InputEvents::from_buffer(&native),
                                None,
                                &mut OutputEvents::from_buffer(&mut queue),
                            )
                            .unwrap();
                        assert_eq!(report.delivery.delivered, 4);
                        assert_eq!(report.delivery.reason, None);
                        let on = queue
                            .events
                            .get(1)
                            .unwrap()
                            .as_event::<NoteOnEvent>()
                            .unwrap();
                        assert_eq!(on.as_raw().port_index, 2);
                        assert_eq!(on.as_raw().note_id, 0);
                        assert_eq!(on.velocity(), f64::from(0.001_f32));
                        assert_eq!(on.header().time(), 1);
                        let ex = queue
                            .events
                            .get(2)
                            .unwrap()
                            .as_event::<NoteExpressionEvent>()
                            .unwrap();
                        assert_eq!(ex.as_raw().port_index, 2);
                        assert_eq!(ex.as_raw().channel, -1);
                        assert_eq!(ex.value().to_bits(), 0.12345678912345678_f64.to_bits());
                        assert_eq!(
                            ex.as_raw().expression_id,
                            NoteExpressionType::Expression.into_raw()
                        );
                        let off = queue
                            .events
                            .get(3)
                            .unwrap()
                            .as_event::<NoteOffEvent>()
                            .unwrap();
                        assert_eq!(off.as_raw().note_id, i32::MAX);
                        assert_eq!(off.header().time(), 7);
                    }
                });
                assert_eq!(counts.count_total, 0);
                // Malformed selected data after a valid parameter prefix cannot reach DSP.
                let mut bad = *expr(2, NoteExpressionType::Pressure, 0.5).as_raw();
                bad.note_id = -2;
                let mut rejected = EventBuffer::with_capacity(2);
                rejected.push(&ParamValueEvent::new(
                    0,
                    ClapId::new(7),
                    Pckn::match_all(),
                    0.75,
                ));
                rejected.push(&NoteExpressionEvent::from_raw(&bad));
                queue.events.clear();
                let calls = bridge.processor().calls;
                assert_eq!(
                    bridge.process_with_output(
                        &mut audio,
                        &InputEvents::from_buffer(&rejected),
                        None,
                        &mut OutputEvents::from_buffer(&mut queue)
                    ),
                    Err(BridgeError::InvalidEvent)
                );
                assert_eq!(bridge.processor().calls, calls);
                assert!(queue.events.is_empty());
                // A native-port queue refusal reports exactly one delivery, without replay.
                queue.limit = 1;
                queue.events.clear();
                let report = bridge
                    .process_with_output(
                        &mut audio,
                        &InputEvents::from_buffer(&native),
                        None,
                        &mut OutputEvents::from_buffer(&mut queue),
                    )
                    .unwrap();
                assert_eq!(report.delivery.delivered, 1);
                assert_eq!(report.delivery.undelivered, 3);
                assert_eq!(report.delivery.reason, Some(OutputRejection::HostRejected));
            }
            assert_eq!(samples, [0.375 as $sample; 8]);
        }
    };
}
native_profile!(
    native_f32_input_output_precision_scope_and_prefix_contract,
    f32,
    f32_output_only
);
native_profile!(
    native_f64_input_output_precision_scope_and_prefix_contract,
    f64,
    f64_output_only
);
#[test]
fn native_output_width_and_flush_reject_whole_delivery_without_losing_audio() {
    use clack_host::prelude::{AudioPortBuffer, AudioPortBufferType, AudioPorts};
    for wide in [false, true] {
        let mut bridge = MainBusBridge::<_, f32>::prepare_with_output(
            Echo {
                calls: 0,
                forced: true,
                wide,
            },
            ProcessConfig::new(48000., 8, 0, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_output_events(2),
            None,
            Some(0),
        )
        .unwrap()
        .with_clap_note_output_port(Some(2))
        .unwrap();
        let mut queue = Queue {
            events: EventBuffer::with_capacity(2),
            limit: 2,
            attempts: 0,
        };
        let mut samples = [0.; 8];
        {
            let mut ports = AudioPorts::with_capacity(1, 1);
            let buffers = ports.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
            }]);
            let mut audio = buffers.to_plugin_audio();
            let report = bridge
                .process_with_output(
                    &mut audio,
                    &InputEvents::empty(),
                    None,
                    &mut OutputEvents::from_buffer(&mut queue),
                )
                .unwrap();
            assert_eq!(report.delivery.delivered, if wide { 0 } else { 2 });
            assert_eq!(
                report.delivery.reason,
                wide.then_some(OutputRejection::Unrepresentable)
            );
        }
        assert_eq!(samples, [0.375; 8]);
        queue.events.clear();
        queue.attempts = 0;
        let report = bridge
            .flush_with_output(
                &InputEvents::empty(),
                &mut OutputEvents::from_buffer(&mut queue),
            )
            .unwrap();
        assert_eq!(report.delivery.delivered, 0);
        assert_eq!(report.delivery.undelivered, 2);
        assert_eq!(
            report.delivery.reason,
            Some(OutputRejection::Unrepresentable)
        );
        assert_eq!(queue.attempts, 0);
        assert_eq!(bridge.processor().calls, 2);
    }
    // Both adapter families validate the exact same explicit profile bounds.
    let config = ProcessConfig::new(48000., 8, 1, AudioLayout::MONO_INSTRUMENT).unwrap();
    assert!(
        MainBusBridge::<_, f32>::prepare(
            Echo {
                calls: 0,
                forced: false,
                wide: false
            },
            config,
            None
        )
        .unwrap()
        .with_clap_note_output_port(Some(256))
        .is_err()
    );
    assert!(
        MultiBusBridge::<_, f64>::prepare(
            Echo {
                calls: 0,
                forced: false,
                wide: false
            },
            config,
            None,
            None
        )
        .unwrap()
        .with_clap_note_input_port(Some(256))
        .is_err()
    );
}
#[test]
fn native_multibus_profile_and_legacy_notes_preserve_ports_without_heap() {
    use clack_host::prelude::{AudioPortBuffer, AudioPortBufferType, AudioPorts};
    let config = ProcessConfig::new(48000., 8, 3, AudioLayout::MONO_INSTRUMENT)
        .unwrap()
        .with_output_events(3)
        .with_auxiliary_buses(&[AudioLayout::MONO_INSTRUMENT])
        .unwrap();
    let mut bridge = MultiBusBridge::<_, f64>::prepare(
        Echo {
            calls: 0,
            forced: false,
            wide: false,
        },
        config,
        None,
        None,
    )
    .unwrap()
    .with_clap_note_input_port(Some(1))
    .unwrap()
    .with_clap_note_output_port(Some(2))
    .unwrap();
    let mut native = EventBuffer::with_capacity(3);
    native.push(&NoteOnEvent::new(
        0,
        Pckn::new(1u16, 2u16, 60u16, Match::All),
        0.001,
    ));
    native.push(&expr(2, NoteExpressionType::Pressure, 0.12345678912345678));
    native.push(&NoteOffEvent::new(
        7,
        Pckn::new(1u16, 2u16, 60u16, Match::All),
        0.5,
    ));
    let mut main = [0.; 8];
    let mut aux = [99.; 8];
    {
        let mut ports = AudioPorts::with_capacity(2, 2);
        let buffers = ports.with_output_buffers([
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f64_output_only([main.as_mut_slice()]),
            },
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f64_output_only([aux.as_mut_slice()]),
            },
        ]);
        let mut audio = buffers.to_plugin_audio();
        let mut queue = Queue {
            events: EventBuffer::with_capacity(3),
            limit: 3,
            attempts: 0,
        };
        let counts = allocation_counter::measure(|| {
            for _ in 0..1000 {
                queue.events.clear();
                let report = bridge
                    .process(
                        &mut audio,
                        &InputEvents::from_buffer(&native),
                        None,
                        &mut OutputEvents::from_buffer(&mut queue),
                    )
                    .unwrap();
                assert_eq!(report.delivery.delivered, 3);
                let on = queue
                    .events
                    .get(0)
                    .unwrap()
                    .as_event::<NoteOnEvent>()
                    .unwrap();
                assert_eq!(on.as_raw().note_id, -1);
                assert_eq!(on.as_raw().port_index, 2);
                assert_eq!(on.velocity(), f64::from(0.001_f32));
                let off = queue
                    .events
                    .get(2)
                    .unwrap()
                    .as_event::<NoteOffEvent>()
                    .unwrap();
                assert_eq!(off.as_raw().note_id, -1);
                assert_eq!(off.as_raw().port_index, 2);
            }
        });
        assert_eq!(counts.count_total, 0);
    }
    assert_eq!(main, [0.375; 8]);
    assert_eq!(aux, [0.; 8]);
}

#[test]
fn all_native_expression_outputs_and_raw_midi_ump_keep_independent_ports() {
    use clack_host::prelude::{AudioPortBuffer, AudioPortBufferType, AudioPorts};
    use clack_plugin::events::event_types::{Midi2Event, MidiEvent};
    let kinds = [
        (NoteExpressionType::Volume, 4.),
        (NoteExpressionType::Pan, 0.12345678912345678),
        (NoteExpressionType::Tuning, -119.12345678912345),
        (NoteExpressionType::Vibrato, 1.),
        (NoteExpressionType::Expression, 0.5),
        (NoteExpressionType::Brightness, 0.),
        (NoteExpressionType::Pressure, 0.8765432109876543),
    ];
    let mut native = EventBuffer::with_capacity(11);
    native.push(&NoteOnEvent::new(0, Pckn::new(1u16, 2u16, 60u16, 0u32), 0.));
    for (kind, value) in kinds {
        native.push(&expr(2, kind, value));
    }
    native.push(&MidiEvent::new(2, 4, [0xc2, 10, 99]));
    native.push(&Midi2Event::new(2, 4, [0x20903c7f, 0xdeadbeef, 0, 0]));
    native.push(&NoteOffEvent::new(
        7,
        Pckn::new(1u16, 2u16, 60u16, Match::All),
        0.5,
    ));
    let mut bridge = MainBusBridge::<_, f32>::prepare_with_output(
        Echo {
            calls: 0,
            forced: false,
            wide: false,
        },
        ProcessConfig::new(48000., 8, 11, AudioLayout::MONO_INSTRUMENT)
            .unwrap()
            .with_output_events(11),
        Some(4),
        Some(3),
    )
    .unwrap()
    .with_clap_note_input_port(Some(1))
    .unwrap()
    .with_clap_note_output_port(Some(2))
    .unwrap();
    let mut samples = [0.; 8];
    let mut ports = AudioPorts::with_capacity(1, 1);
    let buffers = ports.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
    }]);
    let mut audio = buffers.to_plugin_audio();
    let mut queue = Queue {
        events: EventBuffer::with_capacity(11),
        limit: 11,
        attempts: 0,
    };
    let counts = allocation_counter::measure(|| {
        for _ in 0..1000 {
            queue.events.clear();
            let report = bridge
                .process_with_output(
                    &mut audio,
                    &InputEvents::from_buffer(&native),
                    None,
                    &mut OutputEvents::from_buffer(&mut queue),
                )
                .unwrap();
            assert_eq!(report.delivery.delivered, 11);
            let on = queue
                .events
                .get(0)
                .unwrap()
                .as_event::<NoteOnEvent>()
                .unwrap();
            assert_eq!(on.as_raw().port_index, 2);
            assert_eq!(on.velocity(), 0.);
            for (index, (kind, value)) in kinds.into_iter().enumerate() {
                let ex = queue
                    .events
                    .get(index as u32 + 1)
                    .unwrap()
                    .as_event::<NoteExpressionEvent>()
                    .unwrap();
                assert_eq!(ex.as_raw().expression_id, kind.into_raw());
                assert_eq!(ex.value().to_bits(), value.to_bits());
                assert_eq!(ex.as_raw().port_index, 2);
            }
            let midi = queue
                .events
                .get(8)
                .unwrap()
                .as_event::<MidiEvent>()
                .unwrap();
            assert_eq!(midi.port_index(), 3);
            assert_eq!(midi.data(), [0xc2, 10, 0]);
            let ump = queue
                .events
                .get(9)
                .unwrap()
                .as_event::<Midi2Event>()
                .unwrap();
            assert_eq!(ump.port_index(), 3);
            assert_eq!(ump.data(), [0x20903c7f, 0, 0, 0]);
        }
    });
    assert_eq!(counts.count_total, 0);
}

#[test]
fn selected_scoped_off_and_choke_roundtrip_without_heap_or_velocity_inference() {
    use clack_host::prelude::{AudioPortBuffer, AudioPortBufferType, AudioPorts};
    use clack_plugin::events::event_types::NoteChokeEvent;
    let target = Pckn::new(1u16, Match::All, 60u16, i32::MAX as u32);
    let off = NoteOffEvent::new(2, target, 0.12345678912345678);
    let choke = NoteChokeEvent::new(5, Pckn::new(1u16, Match::All, Match::All, Match::All));
    let mut raw = *choke.as_raw();
    raw.velocity = f64::NAN; // Standard ignored choke velocity.
    let mut native = EventBuffer::with_capacity(3);
    native.push(&ParamValueEvent::new(
        0,
        ClapId::new(7),
        Pckn::match_all(),
        0.5,
    ));
    native.push(&off);
    native.push(&NoteChokeEvent::from_raw(&raw));
    let mut bridge = MainBusBridge::<_, f32>::prepare_with_output(
        Echo {
            calls: 0,
            forced: false,
            wide: false,
        },
        ProcessConfig::new(48000., 8, 3, AudioLayout::MONO_INSTRUMENT)
            .unwrap()
            .with_output_events(3),
        None,
        None,
    )
    .unwrap()
    .with_clap_note_input_port(Some(1))
    .unwrap()
    .with_clap_note_output_port(Some(2))
    .unwrap();
    let mut samples = [0.; 8];
    let mut ports = AudioPorts::with_capacity(1, 1);
    let buffers = ports.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
    }]);
    let mut audio = buffers.to_plugin_audio();
    let mut queue = Queue {
        events: EventBuffer::with_capacity(3),
        limit: 3,
        attempts: 0,
    };
    let counts = allocation_counter::measure(|| {
        for _ in 0..1000 {
            queue.events.clear();
            let report = bridge
                .process_with_output(
                    &mut audio,
                    &InputEvents::from_buffer(&native),
                    None,
                    &mut OutputEvents::from_buffer(&mut queue),
                )
                .unwrap();
            assert_eq!(report.delivery.delivered, 3);
            let off = queue
                .events
                .get(1)
                .unwrap()
                .as_event::<NoteOffEvent>()
                .unwrap();
            assert_eq!(off.as_raw().port_index, 2);
            assert_eq!(off.as_raw().channel, -1);
            assert_eq!(off.as_raw().key, 60);
            assert_eq!(off.as_raw().note_id, i32::MAX);
            assert_eq!(off.velocity(), f64::from(0.12345678912345678_f64 as f32));
            let choke = queue
                .events
                .get(2)
                .unwrap()
                .as_event::<NoteChokeEvent>()
                .unwrap();
            assert_eq!(choke.header().time(), 5);
            assert_eq!(choke.as_raw().port_index, 2);
            assert_eq!(choke.as_raw().channel, -1);
            assert_eq!(choke.as_raw().key, -1);
            assert_eq!(choke.as_raw().note_id, -1);
        }
    });
    assert_eq!(counts.count_total, 0);
    let mut raw = *choke.as_raw();
    raw.note_id = -2;
    native.clear();
    native.push(&ParamValueEvent::new(
        0,
        ClapId::new(7),
        Pckn::match_all(),
        0.75,
    ));
    native.push(&NoteChokeEvent::from_raw(&raw));
    queue.events.clear();
    let calls = bridge.processor().calls;
    assert_eq!(
        bridge.process_with_output(
            &mut audio,
            &InputEvents::from_buffer(&native),
            None,
            &mut OutputEvents::from_buffer(&mut queue)
        ),
        Err(BridgeError::InvalidEvent)
    );
    assert_eq!(bridge.processor().calls, calls);
    assert!(queue.events.is_empty());
}
