use clack_host::prelude::*;
use clack_plugin::events::{event_types::MidiEvent, io::InputEvents};
use ravet_clap::{BridgeError, MultiBusBridge};
use ravet_foundation::{
    AudioLayout, Event, EventData, ParameterId, ProcessBlock, ProcessConfig, ProcessStatus,
    Processor, Sample,
};
struct Routes<S: Sample> {
    config: Option<ProcessConfig>,
    calls: usize,
    value: S,
    fault: bool,
}
impl<S: Sample> Processor<S> for Routes<S> {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
        self.config = Some(c);
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.config = None;
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        if let Err(e) = self.config.unwrap().validate_block(b) {
            b.clear_outputs();
            return ProcessStatus::Fault(e);
        }
        self.calls += 1;
        let aux = b.auxiliary.as_deref_mut().unwrap();
        let source = aux[0].input(0).unwrap();
        for c in 0..b.audio.output_channels() {
            b.audio.output_mut(c).unwrap().copy_from_slice(source);
        }
        for bus in &mut aux[1..] {
            for c in 0..bus.output_channels() {
                bus.output_mut(c).unwrap().fill(self.value);
            }
        }
        if let Some(output) = &mut b.output_events {
            output
                .push(Event {
                    frame_offset: 0,
                    data: EventData::ParameterValue {
                        id: ParameterId(5),
                        value: 0.5,
                    },
                })
                .unwrap();
        }
        if self.fault {
            ProcessStatus::Fault(ravet_foundation::ProcessError::OutputEventCapacity)
        } else {
            ProcessStatus::Continue
        }
    }
}
fn routes<S: Sample>(value: S) -> Routes<S> {
    Routes {
        config: None,
        calls: 0,
        value,
        fault: false,
    }
}
fn config() -> ProcessConfig {
    ProcessConfig::new(48000., 8, 4, AudioLayout::STEREO_INSTRUMENT)
        .unwrap()
        .with_auxiliary_buses(&[
            AudioLayout::new(1, 0).unwrap(),
            AudioLayout::MONO_INSTRUMENT,
        ])
        .unwrap()
        .with_output_events(1)
}
#[test]
fn compact_input_only_sidechain_output_only_auxiliary_and_flush_have_no_heap() {
    let mut bridge = MultiBusBridge::prepare(routes(0.75f32), config(), Some(2), None).unwrap();
    let mut source = [0.125; 8];
    let mut left = [99.; 8];
    let mut right = [99.; 8];
    let mut extra = [99.; 8];
    let mut ins = AudioPorts::with_capacity(1, 1);
    let mut outs = AudioPorts::with_capacity(3, 2);
    {
        let inputs = ins.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only([InputChannel::variable(&mut source)]),
        }]);
        let outputs = outs.with_output_buffers([
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only(
                    vec![left.as_mut_slice(), right.as_mut_slice()].into_iter(),
                ),
            },
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only(
                    vec![extra.as_mut_slice()].into_iter(),
                ),
            },
        ]);
        let mut audio = outputs.to_plugin_audio_with_inputs(&inputs);
        let mut events = clack_plugin::events::io::EventBuffer::with_capacity(1);
        let report = bridge
            .process(
                &mut audio,
                &InputEvents::empty(),
                None,
                &mut OutputEvents::from_buffer(&mut events),
            )
            .unwrap();
        assert_eq!(report.delivery.delivered, 1);
        let allocation = allocation_counter::measure(|| {
            for _ in 0..10_000 {
                bridge.reset();
                bridge
                    .process(
                        &mut audio,
                        &InputEvents::empty(),
                        None,
                        &mut OutputEvents::void(),
                    )
                    .unwrap();
                bridge
                    .flush(&InputEvents::empty(), &mut OutputEvents::void())
                    .unwrap();
            }
        });
        assert_eq!(allocation.count_total, 0);
        assert_eq!(allocation.count_current, 0);
    }
    assert_eq!(left, source);
    assert_eq!(right, source);
    assert_eq!(extra, [0.75; 8]);
    assert_eq!(bridge.processor().calls, 20_001);
}
#[test]
fn native_f64_and_malformed_event_output_precision_fault_silence_all_ports() {
    let mut bridge =
        MultiBusBridge::prepare(routes(1. + 2f64.powi(-40)), config(), Some(2), None).unwrap();
    let mut source = [1. + 2f64.powi(-45); 8];
    let mut left = [99.; 8];
    let mut right = [99.; 8];
    let mut extra = [99.; 8];
    let mut ins = AudioPorts::with_capacity(1, 1);
    let mut outs = AudioPorts::with_capacity(3, 2);
    {
        let inputs = ins.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f64_input_only([InputChannel::variable(&mut source)]),
        }]);
        let outputs = outs.with_output_buffers([
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f64_output_only(
                    vec![left.as_mut_slice(), right.as_mut_slice()].into_iter(),
                ),
            },
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f64_output_only(
                    vec![extra.as_mut_slice()].into_iter(),
                ),
            },
        ]);
        bridge
            .process(
                &mut outputs.to_plugin_audio_with_inputs(&inputs),
                &InputEvents::empty(),
                None,
                &mut OutputEvents::void(),
            )
            .unwrap();
    }
    assert_eq!(left, source);
    assert_eq!(right, source);
    assert_eq!(extra, [1. + 2f64.powi(-40); 8]);
    {
        let inputs = ins.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f64_input_only([InputChannel::variable(&mut source)]),
        }]);
        let outputs = outs.with_output_buffers([
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f64_output_only(
                    vec![left.as_mut_slice(), right.as_mut_slice()].into_iter(),
                ),
            },
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f64_output_only(
                    vec![extra.as_mut_slice()].into_iter(),
                ),
            },
        ]);
        let bad = [MidiEvent::new(8, 99, [0, 0, 0])];
        assert_eq!(
            bridge.process(
                &mut outputs.to_plugin_audio_with_inputs(&inputs),
                &InputEvents::from_buffer(&bad),
                None,
                &mut OutputEvents::void()
            ),
            Err(BridgeError::InvalidEvent)
        );
    }
    assert_eq!(bridge.processor().calls, 1);
    assert_eq!(left, [0.; 8]);
    assert_eq!(extra, [0.; 8]);
    let mut wrong = [99f32; 8];
    {
        let inputs = ins.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f64_input_only([InputChannel::variable(&mut source)]),
        }]);
        let outputs = outs.with_output_buffers([
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::F64(
                    vec![left.as_mut_slice(), right.as_mut_slice()].into_iter(),
                ),
            },
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::F32(vec![wrong.as_mut_slice()].into_iter()),
            },
        ]);
        assert_eq!(
            bridge.process(
                &mut outputs.to_plugin_audio_with_inputs(&inputs),
                &InputEvents::empty(),
                None,
                &mut OutputEvents::void()
            ),
            Err(BridgeError::UnsupportedSampleType)
        );
    }
    assert_eq!(bridge.processor().calls, 1);
    assert_eq!(wrong, [0.; 8]);
    let mut faulty = routes(0.7f64);
    faulty.fault = true;
    let mut bridge = MultiBusBridge::prepare(faulty, config(), None, None).unwrap();
    let mut queue = clack_plugin::events::io::EventBuffer::with_capacity(1);
    {
        let inputs = ins.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f64_input_only([InputChannel::variable(&mut source)]),
        }]);
        let outputs = outs.with_output_buffers([
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f64_output_only(
                    vec![left.as_mut_slice(), right.as_mut_slice()].into_iter(),
                ),
            },
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f64_output_only(
                    vec![extra.as_mut_slice()].into_iter(),
                ),
            },
        ]);
        assert!(matches!(
            bridge.process(
                &mut outputs.to_plugin_audio_with_inputs(&inputs),
                &InputEvents::empty(),
                None,
                &mut OutputEvents::from_buffer(&mut queue)
            ),
            Err(BridgeError::Processor(_))
        ));
    }
    assert_eq!(left, [0.; 8]);
    assert_eq!(right, [0.; 8]);
    assert_eq!(extra, [0.; 8]);
    assert_eq!(queue.len(), 0);
}
#[test]
fn aggregate_scratch_limit_and_wrong_native_port_count_precede_dsp() {
    let large = ProcessConfig::new(48000., 65536, 0, AudioLayout::new(64, 64).unwrap())
        .unwrap()
        .with_auxiliary_buses(&[AudioLayout::new(64, 64).unwrap()])
        .unwrap();
    assert!(matches!(
        MultiBusBridge::prepare(routes(0f64), large, None, None),
        Err(BridgeError::ScratchCapacity)
    ));
    let mut bridge = MultiBusBridge::prepare(routes(0f32), config(), None, None).unwrap();
    let mut samples = [99f32; 8];
    let mut ports = AudioPorts::with_capacity(1, 1);
    {
        let outputs = ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
        }]);
        assert_eq!(
            bridge.process(
                &mut outputs.to_plugin_audio(),
                &InputEvents::empty(),
                None,
                &mut OutputEvents::void()
            ),
            Err(BridgeError::LayoutMismatch)
        );
    }
    assert_eq!(samples, [0.; 8]);
    assert_eq!(bridge.processor().calls, 0);
}
