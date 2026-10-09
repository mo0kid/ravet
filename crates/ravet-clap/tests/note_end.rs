use clack_host::prelude::{AudioPortBuffer, AudioPortBufferType, AudioPorts};
use clack_plugin::events::{
    Event as ClapEvent, Pckn, UnknownEvent,
    event_types::{NoteEndEvent, ParamValueEvent},
    io::{EventBuffer, InputEvents, OutputEventBuffer, OutputEvents, TryPushError},
};
use ravet_clap::{MainBusBridge, MultiBusBridge, OutputRejection, ParameterEvents};
use ravet_foundation::{
    AudioLayout, Event, EventData, NoteId, NoteTarget, ParameterId, ProcessBlock, ProcessConfig,
    ProcessStatus, Processor,
};
struct Emits {
    wide: bool,
    calls: usize,
}
macro_rules! processor {
    ($sample:ty) => {
        impl Processor<$sample> for Emits {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
                Ok(())
            }
            fn reset(&mut self) {}
            fn deactivate(&mut self) {}
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                self.calls += 1;
                block.audio.output_mut(0).unwrap().fill(0.25 as $sample);
                for (offset, data) in [
                    (
                        0,
                        EventData::ParameterValue {
                            id: ParameterId(7),
                            value: 0.5,
                        },
                    ),
                    (
                        block.audio.frames().saturating_sub(1),
                        EventData::NoteEnd {
                            target: NoteTarget {
                                channel: Some(2),
                                key: Some(60),
                                id: Some(NoteId(if self.wide { u32::MAX } else { 123 })),
                            },
                        },
                    ),
                ] {
                    block
                        .output_events
                        .as_mut()
                        .unwrap()
                        .push(Event {
                            frame_offset: offset,
                            data,
                        })
                        .unwrap();
                }
                ProcessStatus::Continue
            }
        }
    };
}
processor!(f32);
processor!(f64);
struct Queue {
    events: EventBuffer,
    limit: u32,
    attempts: usize,
}
impl OutputEventBuffer for Queue {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        self.attempts += 1;
        if self.events.len() >= self.limit {
            return Err(TryPushError::new());
        }
        self.events.push(event);
        Ok(())
    }
}
macro_rules! profile {
    ($name:ident,$sample:ty,$channels:ident,$bridge:ident,$prepare:ident,$process:ident,$flush:ident) => {
        #[test]
        fn $name() {
            let config = ProcessConfig::new(48000., 8, 2, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_output_events(2);
            for (selected, wide, limit) in [
                (true, false, 2),
                (false, false, 2),
                (true, true, 2),
                (true, false, 1),
            ] {
                let mut bridge =
                    $bridge::<_, $sample>::$prepare(Emits { wide, calls: 0 }, config, None, None)
                        .unwrap()
                        .with_clap_note_input_port(Some(1))
                        .unwrap()
                        .with_clap_note_output_port(Some(2))
                        .unwrap()
                        .with_scoped_parameter_output_port(Some(4))
                        .unwrap()
                        .with_note_end_output_port(if selected { Some(3) } else { None })
                        .unwrap();
                let native = NoteEndEvent::new(0, Pckn::new(1u16, 2u16, 60u16, 123u32));
                let mut raw = *native.as_raw();
                raw.channel = -2;
                raw.velocity = f64::NAN;
                let native = [NoteEndEvent::from_raw(&raw)];
                let input = InputEvents::from_buffer(&native);
                let mut queue = Queue {
                    events: EventBuffer::with_capacity(2),
                    limit,
                    attempts: 0,
                };
                let mut samples = [0. as $sample; 8];
                let mut ports = AudioPorts::with_capacity(1, 1);
                {
                    let buffers = ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels:
                            AudioPortBufferType::<[&mut [f32]; 1], [&mut [f64]; 1]>::$channels([
                                samples.as_mut_slice(),
                            ]),
                    }]);
                    let mut audio = buffers.to_plugin_audio();
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            queue.events.clear();
                            queue.attempts = 0;
                            let report = bridge
                                .$process(
                                    &mut audio,
                                    &input,
                                    None,
                                    &mut OutputEvents::from_buffer(&mut queue),
                                )
                                .unwrap();
                            assert_eq!(report.status, ProcessStatus::Continue);
                            if !selected || wide {
                                assert_eq!(
                                    report.delivery.reason,
                                    Some(OutputRejection::Unrepresentable)
                                );
                                assert_eq!(report.delivery.delivered, 0);
                                assert_eq!(queue.attempts, 0);
                            } else if limit == 1 {
                                assert_eq!(
                                    report.delivery.reason,
                                    Some(OutputRejection::HostRejected)
                                );
                                assert_eq!(report.delivery.delivered, 1);
                                assert_eq!(queue.attempts, 2);
                            } else {
                                assert_eq!(report.delivery.delivered, 2);
                                assert!(
                                    queue
                                        .events
                                        .get(0)
                                        .unwrap()
                                        .as_event::<ParamValueEvent>()
                                        .is_some()
                                );
                                let end = queue
                                    .events
                                    .get(1)
                                    .unwrap()
                                    .as_event::<NoteEndEvent>()
                                    .unwrap();
                                assert_eq!(end.as_raw().port_index, 3);
                                assert_eq!(end.as_raw().channel, 2);
                                assert_eq!(end.as_raw().key, 60);
                                assert_eq!(end.as_raw().note_id, 123);
                                assert_eq!(end.header().time(), 7);
                            }
                            queue.events.clear();
                            queue.attempts = 0;
                            let report = bridge
                                .$flush(
                                    &InputEvents::empty(),
                                    &mut OutputEvents::from_buffer(&mut queue),
                                )
                                .unwrap();
                            assert_eq!(
                                report.delivery.reason,
                                Some(OutputRejection::Unrepresentable)
                            );
                            assert_eq!(report.delivery.delivered, 0);
                            assert_eq!(queue.attempts, 0);
                        }
                    });
                    assert_eq!(counts.count_total, 0);
                    assert_eq!(counts.count_current, 0);
                }
                assert_eq!(samples, [0.25 as $sample; 8]);
                assert_eq!(bridge.processor().calls, 2000);
            }
        }
    };
}
profile!(
    main_f32_completion_output_routes_preflight_flush_refusal_heap,
    f32,
    F32,
    MainBusBridge,
    prepare_with_output,
    process_with_output,
    flush_with_output
);
profile!(
    main_f64_completion_output_routes_preflight_flush_refusal_heap,
    f64,
    F64,
    MainBusBridge,
    prepare_with_output,
    process_with_output,
    flush_with_output
);
profile!(
    multi_f64_completion_output_routes_preflight_flush_refusal_heap,
    f64,
    F64,
    MultiBusBridge,
    prepare,
    process,
    flush
);
#[test]
fn completion_output_port_bounds_and_input_direction_filter() {
    let config = ProcessConfig::new(48000., 8, 1, AudioLayout::MONO_INSTRUMENT)
        .unwrap()
        .with_output_events(2);
    assert!(
        MainBusBridge::<_, f32>::prepare_with_output(
            Emits {
                wide: false,
                calls: 0
            },
            config,
            None,
            None
        )
        .unwrap()
        .with_note_end_output_port(Some(256))
        .is_err()
    );
    let mut translator = ParameterEvents::with_capacity(1)
        .unwrap()
        .with_clap_note_input_port(Some(1))
        .unwrap();
    let native = [NoteEndEvent::new(0, Pckn::new(1u16, 2u16, 60u16, 123u32))];
    assert!(
        translator
            .translate(&InputEvents::from_buffer(&native), 8, false)
            .unwrap()
            .is_empty()
    );
}
