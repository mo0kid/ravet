use clack_plugin::{
    events::{
        Event as ClapEvent, Match, Pckn,
        event_types::{ParamModEvent, ParamValueEvent},
        io::{EventBuffer, InputEvents, OutputEvents},
    },
    utils::ClapId,
};
use ravet_clap::{BridgeError, MainBusBridge, OutputRejection, ParameterEvents};
use ravet_foundation::{
    AudioLayout, Event, EventData, NoteId, NoteTarget, ParameterId, ProcessBlock, ProcessConfig,
    ProcessStatus, Processor,
};
fn native() -> EventBuffer {
    let mut buffer = EventBuffer::with_capacity(4);
    buffer.push(&ParamValueEvent::new(
        0,
        ClapId::new(7),
        Pckn::match_all(),
        0.5,
    ));
    buffer.push(&ParamValueEvent::new(
        2,
        ClapId::new(7),
        Pckn::new(1u16, 2u16, Match::All, 123u32),
        0.12345678912345678,
    ));
    buffer.push(&ParamModEvent::new(
        2,
        ClapId::new(7),
        Pckn::new(1u16, Match::All, Match::All, Match::All),
        -f64::MAX,
    ));
    buffer.push(&ParamModEvent::new(
        5,
        ClapId::new(7),
        Pckn::match_all(),
        -0.25,
    ));
    buffer
}
#[test]
fn global_and_scoped_translation_and_zero_time_flush_remain_independent_and_heap_free() {
    let native = native();
    let input = InputEvents::from_buffer(&native);
    let mut default = ParameterEvents::with_capacity(4)
        .unwrap()
        .with_global_parameter_modulation(true);
    assert_eq!(default.translate(&input, 8, false).unwrap().len(), 2);
    let mut selected = ParameterEvents::with_capacity(4)
        .unwrap()
        .with_scoped_parameter_input_port(Some(1))
        .unwrap();
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            let events = selected.translate(&input, 8, false).unwrap();
            assert_eq!(events.len(), 3);
            assert_eq!(
                events[1],
                Event {
                    frame_offset: 2,
                    data: EventData::ParameterValueScoped {
                        id: ParameterId(7),
                        target: NoteTarget {
                            channel: Some(2),
                            key: None,
                            id: Some(NoteId(123))
                        },
                        value: 0.12345678912345678
                    }
                }
            );
            assert_eq!(
                events[2],
                Event {
                    frame_offset: 2,
                    data: EventData::ParameterModulationScoped {
                        id: ParameterId(7),
                        target: NoteTarget::default(),
                        amount: -f64::MAX
                    }
                }
            );
            let events = selected.translate(&input, 0, true).unwrap();
            assert_eq!(events.len(), 3);
            assert!(events.iter().all(|event| event.frame_offset == 0));
        }
    });
    assert_eq!(counts.count_total, 0);
    let mut selected = selected.with_global_parameter_modulation(true);
    assert_eq!(selected.translate(&input, 8, false).unwrap().len(), 4);
    assert!(
        ParameterEvents::new()
            .with_scoped_parameter_input_port(Some(256))
            .is_err()
    );
}
#[test]
fn malformed_selected_scope_is_never_normalized_to_global_and_other_ports_filter() {
    let good = ParamValueEvent::new(
        2,
        ClapId::new(7),
        Pckn::new(1u16, Match::All, Match::All, Match::All),
        0.5,
    );
    let mut invalid = [*good.as_raw(); 6];
    invalid[0].channel = -2;
    invalid[1].key = 128;
    invalid[2].note_id = -2;
    invalid[3].value = f64::NAN;
    invalid[4].param_id = u32::MAX;
    invalid[5].channel = 16;
    let mut selected = ParameterEvents::with_capacity(2)
        .unwrap()
        .with_scoped_parameter_input_port(Some(1))
        .unwrap();
    for mut raw in invalid {
        let mut buffer = EventBuffer::with_capacity(2);
        buffer.push(&ParamValueEvent::new(
            0,
            ClapId::new(7),
            Pckn::match_all(),
            0.5,
        ));
        buffer.push(&ParamValueEvent::from_raw(&raw));
        assert_eq!(
            selected.translate(&InputEvents::from_buffer(&buffer), 8, false),
            Err(BridgeError::InvalidEvent)
        );
        raw.port_index = 0;
        let other = [ParamValueEvent::from_raw(&raw)];
        assert!(
            selected
                .translate(&InputEvents::from_buffer(&other), 8, false)
                .unwrap()
                .is_empty()
        );
    }
    let mut raw = *good.as_raw();
    raw.port_index = -1;
    raw.note_id = -2;
    let negative = [ParamValueEvent::from_raw(&raw)];
    assert!(
        selected
            .translate(&InputEvents::from_buffer(&negative), 8, false)
            .unwrap()
            .is_empty()
    );
    let mut default = ParameterEvents::with_capacity(2).unwrap();
    assert!(
        default
            .translate(&InputEvents::from_buffer(&negative), 8, false)
            .unwrap()
            .is_empty()
    );
    let out_of_order = [
        ParamValueEvent::new(4, ClapId::new(7), good.pckn(), 0.5),
        ParamValueEvent::new(3, ClapId::new(7), good.pckn(), 0.5),
    ];
    assert_eq!(
        default.translate(&InputEvents::from_buffer(&out_of_order), 8, false),
        Err(BridgeError::InvalidEvent)
    );
    let good_mod = ParamModEvent::new(2, ClapId::new(7), good.pckn(), 0.5);
    let mut raw = *good_mod.as_raw();
    raw.amount = f64::INFINITY;
    assert_eq!(
        selected.translate(
            &InputEvents::from_buffer(&[ParamModEvent::from_raw(&raw)]),
            8,
            false
        ),
        Err(BridgeError::InvalidEvent)
    );
}
struct Echo {
    calls: usize,
    forced: bool,
    wide: bool,
}
macro_rules! echo {
    ($sample:ty) => {
        impl Processor<$sample> for Echo {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
                Ok(())
            }
            fn reset(&mut self) {}
            fn deactivate(&mut self) {}
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                self.calls += 1;
                block.audio.output_mut(0).unwrap().fill(0.25 as $sample);
                if self.forced {
                    for data in [
                        EventData::ParameterValue {
                            id: ParameterId(7),
                            value: 0.5,
                        },
                        EventData::ParameterModulationScoped {
                            id: ParameterId(7),
                            target: NoteTarget {
                                id: Some(NoteId(if self.wide { u32::MAX } else { 123 })),
                                ..NoteTarget::default()
                            },
                            amount: -f64::MAX,
                        },
                    ] {
                        block
                            .output_events
                            .as_mut()
                            .unwrap()
                            .push(Event {
                                frame_offset: 0,
                                data,
                            })
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
echo!(f32);
echo!(f64);
macro_rules! profile {
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
            .with_global_parameter_modulation(true)
            .with_scoped_parameter_input_port(Some(1))
            .unwrap()
            .with_scoped_parameter_output_port(Some(2))
            .unwrap();
            let native = native();
            let input = InputEvents::from_buffer(&native);
            let mut samples = [0. as $sample; 8];
            {
                let mut ports = AudioPorts::with_capacity(1, 1);
                let buffers = ports.with_output_buffers([AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::$channels([samples.as_mut_slice()]),
                }]);
                let mut audio = buffers.to_plugin_audio();
                let mut output = EventBuffer::with_capacity(4);
                let counts = allocation_counter::measure(|| {
                    for _ in 0..1000 {
                        output.clear();
                        let report = bridge
                            .process_with_output(
                                &mut audio,
                                &input,
                                None,
                                &mut OutputEvents::from_buffer(&mut output),
                            )
                            .unwrap();
                        assert_eq!(report.delivery.delivered, 4);
                        let global = output
                            .get(0)
                            .unwrap()
                            .as_event::<ParamValueEvent>()
                            .unwrap();
                        assert_eq!(global.pckn(), Pckn::match_all());
                        let value = output
                            .get(1)
                            .unwrap()
                            .as_event::<ParamValueEvent>()
                            .unwrap();
                        assert_eq!(value.value().to_bits(), 0.12345678912345678_f64.to_bits());
                        assert_eq!(value.as_raw().port_index, 2);
                        assert_eq!(value.as_raw().channel, 2);
                        assert_eq!(value.as_raw().key, -1);
                        assert_eq!(value.as_raw().note_id, 123);
                        assert!(value.cookie().as_raw().is_null());
                        assert_eq!(value.header().time(), 2);
                        let modulation =
                            output.get(2).unwrap().as_event::<ParamModEvent>().unwrap();
                        assert_eq!(modulation.amount(), -f64::MAX);
                        assert_eq!(modulation.as_raw().port_index, 2);
                        assert_eq!(modulation.as_raw().note_id, -1);
                        output.clear();
                        let report = bridge
                            .flush_with_output(&input, &mut OutputEvents::from_buffer(&mut output))
                            .unwrap();
                        assert_eq!(report.delivery.delivered, 4);
                        for event in output.iter() {
                            assert_eq!(event.header().time(), 0);
                        }
                    }
                });
                assert_eq!(counts.count_total, 0);
                let mut raw = *ParamValueEvent::new(
                    2,
                    ClapId::new(7),
                    Pckn::new(1u16, Match::All, Match::All, 123u32),
                    0.5,
                )
                .as_raw();
                raw.note_id = -2;
                let mut bad = EventBuffer::with_capacity(2);
                bad.push(&ParamValueEvent::new(
                    0,
                    ClapId::new(7),
                    Pckn::match_all(),
                    0.5,
                ));
                bad.push(&ParamValueEvent::from_raw(&raw));
                output.clear();
                let calls = bridge.processor().calls;
                assert_eq!(
                    bridge.process_with_output(
                        &mut audio,
                        &InputEvents::from_buffer(&bad),
                        None,
                        &mut OutputEvents::from_buffer(&mut output)
                    ),
                    Err(BridgeError::InvalidEvent)
                );
                assert_eq!(bridge.processor().calls, calls);
                assert!(output.is_empty());
            }
            assert_eq!(samples, [0. as $sample; 8]);
        }
    };
}
profile!(
    native_f32_scoped_parameter_input_output_flush_and_atomic_rejection,
    f32,
    f32_output_only
);
profile!(
    native_f64_scoped_parameter_input_output_flush_and_atomic_rejection,
    f64,
    f64_output_only
);
#[test]
fn generated_missing_scope_route_and_native_id_width_never_deliver_global_prefix() {
    use clack_host::prelude::{AudioPortBuffer, AudioPortBufferType, AudioPorts};
    for (port, wide) in [(None, false), (Some(2), true), (Some(2), false)] {
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
            None,
        )
        .unwrap()
        .with_scoped_parameter_output_port(port)
        .unwrap();
        let mut samples = [0.; 8];
        {
            let mut ports = AudioPorts::with_capacity(1, 1);
            let buffers = ports.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
            }]);
            let mut audio = buffers.to_plugin_audio();
            let mut output = EventBuffer::with_capacity(2);
            let report = bridge
                .process_with_output(
                    &mut audio,
                    &InputEvents::empty(),
                    None,
                    &mut OutputEvents::from_buffer(&mut output),
                )
                .unwrap();
            assert_eq!(
                report.delivery.delivered,
                if port.is_some() && !wide { 2 } else { 0 }
            );
            assert_eq!(
                report.delivery.reason,
                if port.is_some() && !wide {
                    None
                } else {
                    Some(OutputRejection::Unrepresentable)
                }
            );
            assert_eq!(output.len(), report.delivery.delivered as u32);
        }
        assert_eq!(samples, [0.25; 8]);
        assert_eq!(bridge.processor().calls, 1);
    }
}

#[test]
fn multibus_scoped_parameters_keep_origin_input_endpoint_and_flush_precision() {
    use clack_host::prelude::{AudioPortBuffer, AudioPortBufferType, AudioPorts};
    let config = ProcessConfig::new(48000., 8, 4, AudioLayout::MONO_INSTRUMENT)
        .unwrap()
        .with_output_events(4)
        .with_auxiliary_buses(&[AudioLayout::MONO_INSTRUMENT])
        .unwrap();
    let mut bridge = ravet_clap::MultiBusBridge::<_, f64>::prepare(
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
    .with_scoped_parameter_input_port(Some(1))
    .unwrap()
    .with_scoped_parameter_output_port(Some(2))
    .unwrap();
    let native = native();
    let input = InputEvents::from_buffer(&native);
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
        let mut output = EventBuffer::with_capacity(4);
        let counts = allocation_counter::measure(|| {
            for _ in 0..1000 {
                output.clear();
                let report = bridge
                    .process(
                        &mut audio,
                        &input,
                        None,
                        &mut OutputEvents::from_buffer(&mut output),
                    )
                    .unwrap();
                assert_eq!(report.delivery.delivered, 3);
                let value = output
                    .get(1)
                    .unwrap()
                    .as_event::<ParamValueEvent>()
                    .unwrap();
                assert_eq!(value.as_raw().port_index, 2);
                assert_eq!(value.value().to_bits(), 0.12345678912345678_f64.to_bits());
                output.clear();
                let report = bridge
                    .flush(&input, &mut OutputEvents::from_buffer(&mut output))
                    .unwrap();
                assert_eq!(report.delivery.delivered, 3);
                let modulation = output.get(2).unwrap().as_event::<ParamModEvent>().unwrap();
                assert_eq!(modulation.amount(), -f64::MAX);
                assert_eq!(modulation.header().time(), 0);
            }
        });
        assert_eq!(counts.count_total, 0);
    }
    assert_eq!(main, [0.25; 8]);
    assert_eq!(aux, [0.; 8]);
}
