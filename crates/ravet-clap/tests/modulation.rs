use clack_plugin::{
    events::{
        Match, Pckn,
        event_types::{ParamModEvent, ParamValueEvent},
        io::InputEvents,
    },
    utils::ClapId,
};
use ravet_clap::{BridgeError, ParameterEvents};
use ravet_foundation::{Event, EventData, ParameterId};
fn modulation(time: usize, amount: f32) -> Event {
    Event {
        frame_offset: time,
        data: EventData::ParameterModulation {
            id: ParameterId(7),
            amount,
        },
    }
}
#[test]
fn explicit_global_modulation_preserves_sign_order_flush_and_rejects_unrepresentable_amounts() {
    let native = [
        ParamModEvent::new(0, ClapId::new(7), Pckn::match_all(), -2.),
        ParamModEvent::new(3, ClapId::new(7), Pckn::match_all(), 0.75),
        ParamModEvent::new(
            3,
            ClapId::new(7),
            Pckn::new(Match::Specific(2), Match::All, Match::All, Match::All),
            f64::NAN,
        ),
    ];
    let mut disabled = ParameterEvents::with_capacity(3).unwrap();
    assert!(
        disabled
            .translate(&InputEvents::from_buffer(&native), 8, false)
            .unwrap()
            .is_empty()
    );
    let mut enabled = ParameterEvents::with_capacity(3)
        .unwrap()
        .with_global_parameter_modulation(true);
    let allocations = allocation_counter::measure(|| {
        for _ in 0..10000 {
            assert_eq!(
                enabled
                    .translate(&InputEvents::from_buffer(&native), 8, false)
                    .unwrap(),
                &[modulation(0, -2.), modulation(3, 0.75)]
            );
            assert_eq!(
                enabled
                    .translate(&InputEvents::from_buffer(&native), 0, true)
                    .unwrap(),
                &[modulation(0, -2.), modulation(0, 0.75)]
            );
        }
    });
    assert_eq!(allocations.count_total, 0);
    for amount in [f64::NAN, f64::INFINITY, -f64::INFINITY, f64::MAX] {
        assert_eq!(
            enabled.translate(
                &InputEvents::from_buffer(&[ParamModEvent::new(
                    0,
                    ClapId::new(7),
                    Pckn::match_all(),
                    amount
                )]),
                8,
                false
            ),
            Err(BridgeError::InvalidEvent)
        );
    }
    for times in [[4, 3], [0, 8]] {
        let events =
            times.map(|time| ParamModEvent::new(time, ClapId::new(7), Pckn::match_all(), 0.));
        assert_eq!(
            disabled.translate(&InputEvents::from_buffer(&events), 8, false),
            Err(BridgeError::InvalidEvent)
        );
    }
}

#[test]
fn prepared_modulation_process_and_flush_keep_base_value_and_reject_before_dsp() {
    use clack_host::prelude::{AudioPortBuffer, AudioPortBufferType, AudioPorts};
    use clack_plugin::events::io::EventBuffer;
    use ravet_clap::MainBusBridge;
    use ravet_foundation::{
        AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor,
    };
    struct Consumer {
        base: f32,
        amount: f32,
        configuration: Option<ProcessConfig>,
        calls: usize,
    }
    impl Consumer {
        fn apply(&mut self, event: Event) {
            match event.data {
                EventData::ParameterValue { value, .. } => self.base = value,
                EventData::ParameterModulation { amount, .. } => self.amount = amount,
                _ => {}
            }
        }
    }
    impl Processor for Consumer {
        fn prepare(&mut self, configuration: ProcessConfig) -> Result<(), PrepareError> {
            self.configuration = Some(configuration);
            Ok(())
        }
        fn reset(&mut self) {
            self.amount = 0.;
        }
        fn deactivate(&mut self) {
            self.configuration = None;
        }
        fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
            self.configuration.unwrap().validate_block(block).unwrap();
            self.calls += 1;
            if block.audio.frames() == 0 {
                for event in block.events {
                    self.apply(*event);
                }
            }
            let mut next = 0;
            for frame in 0..block.audio.frames() {
                while next < block.events.len() && block.events[next].frame_offset == frame {
                    self.apply(block.events[next]);
                    next += 1;
                }
                block.audio.output_mut(0).unwrap()[frame] = self.base + self.amount;
            }
            ProcessStatus::Continue
        }
    }
    let configuration = ProcessConfig::new(48000., 8, 3, AudioLayout::MONO_INSTRUMENT).unwrap();
    let mut bridge = MainBusBridge::prepare(
        Consumer {
            base: 1.,
            amount: 0.,
            configuration: None,
            calls: 0,
        },
        configuration,
        None,
    )
    .unwrap()
    .with_global_parameter_modulation(true);
    let mut input = EventBuffer::with_capacity(3);
    input.push(&ParamValueEvent::new(
        0,
        ClapId::new(7),
        Pckn::match_all(),
        2.,
    ));
    input.push(&ParamModEvent::new(
        2,
        ClapId::new(7),
        Pckn::match_all(),
        -0.5,
    ));
    input.push(&ParamModEvent::new(
        4,
        ClapId::new(7),
        Pckn::match_all(),
        0.75,
    ));
    let mut samples = [99.; 8];
    let mut ports = AudioPorts::with_capacity(1, 1);
    {
        let buffers = ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
        }]);
        assert_eq!(
            bridge.process(&mut buffers.to_plugin_audio(), &input.as_input(), None),
            Ok(ProcessStatus::Continue)
        );
    }
    assert_eq!(samples, [2., 2., 1.5, 1.5, 2.75, 2.75, 2.75, 2.75]);
    {
        let buffers = ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
        }]);
        let mut audio = buffers.to_plugin_audio();
        let allocations = allocation_counter::measure(|| {
            for _ in 0..1000 {
                assert_eq!(
                    bridge.process(&mut audio, &input.as_input(), None),
                    Ok(ProcessStatus::Continue)
                );
            }
        });
        assert_eq!(allocations.count_total, 0);
    }
    assert_eq!(bridge.processor().base, 2.);
    assert_eq!(bridge.processor().amount, 0.75);
    bridge
        .flush(&InputEvents::from_buffer(&[ParamModEvent::new(
            u32::MAX,
            ClapId::new(7),
            Pckn::match_all(),
            -1.,
        )]))
        .unwrap();
    assert_eq!(bridge.processor().base, 2.);
    assert_eq!(bridge.processor().amount, -1.);
    let calls = bridge.processor().calls;
    input.clear();
    input.push(&ParamValueEvent::new(
        0,
        ClapId::new(7),
        Pckn::match_all(),
        4.,
    ));
    input.push(&ParamModEvent::new(
        1,
        ClapId::new(7),
        Pckn::match_all(),
        f64::NAN,
    ));
    {
        let buffers = ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
        }]);
        assert_eq!(
            bridge.process(&mut buffers.to_plugin_audio(), &input.as_input(), None),
            Err(BridgeError::InvalidEvent)
        );
    }
    assert_eq!(bridge.processor().calls, calls);
    assert_eq!(bridge.processor().base, 2.);
    assert_eq!(bridge.processor().amount, -1.);
    assert_eq!(samples, [0.; 8]);
}
