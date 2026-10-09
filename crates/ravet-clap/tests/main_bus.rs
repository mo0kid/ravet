use clack_host::prelude::*;
use clack_plugin::{
    events::{
        event_types::{MidiEvent, ParamValueEvent},
        io::EventBuffer,
    },
    process::Audio,
};
use ravet_clap::{BridgeError, MainBusBridge, ParameterEvents};
use ravet_foundation::{
    AudioLayout, EventData, ProcessBlock, ProcessConfig, ProcessStatus as Status, Processor,
};
use ravet_instrument::{SineInstrument, ZoneSide};
#[test]
fn short_midi_lengths_port_filter_timing_validation_and_parameter_flush() {
    let mut translator = ParameterEvents::with_capacity(4).unwrap();
    let midi = [
        MidiEvent::new(2, 0, [0x91, 60, 100]),
        MidiEvent::new(3, 0, [0xd1, 50, 255]),
        MidiEvent::new(4, 1, [0x90, 255, 255]),
        MidiEvent::new(5, 0, [0xf8, 255, 255]),
    ];
    let translated = translator
        .translate_midi(&InputEvents::from_buffer(&midi), 8, 0)
        .unwrap();
    assert_eq!(translated.len(), 3);
    assert_eq!(translated[0].frame_offset, 2);
    if let EventData::Midi(message) = translated[1].data {
        assert_eq!(message.as_bytes(), &[0xd1, 50]);
    } else {
        panic!();
    }
    assert!(
        translator
            .translate(&InputEvents::from_buffer(&midi), 0, true)
            .unwrap()
            .is_empty()
    );
    for bad in [[0x90, 255, 0], [0xf0, 0, 0], [0xf9, 0, 0], [0, 60, 0]] {
        assert_eq!(
            translator.translate_midi(
                &InputEvents::from_buffer(&[MidiEvent::new(0, 0, bad)]),
                8,
                0
            ),
            Err(BridgeError::InvalidEvent)
        );
    }
    assert_eq!(
        translator.translate_midi(
            &InputEvents::from_buffer(&[MidiEvent::new(8, 1, [0, 0, 0])]),
            8,
            0
        ),
        Err(BridgeError::InvalidEvent)
    );
}
fn instrument_audio(samples: &mut [f32], operation: impl FnOnce(&mut Audio<'_>)) {
    let mut ports = AudioPorts::with_capacity(1, 1);
    let buffers = ports.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_output_only([samples]),
    }]);
    let mut audio = buffers.to_plugin_audio();
    operation(&mut audio);
}
#[test]
fn no_input_instrument_timing_whole_rejection_and_prepared_heap() {
    let mut bridge = MainBusBridge::prepare(
        SineInstrument::<1>::new(ZoneSide::Lower).unwrap(),
        ProcessConfig::new(48000., 32, 4, AudioLayout::MONO_INSTRUMENT).unwrap(),
        Some(0),
    )
    .unwrap();
    let mut samples = [99.; 32];
    let notes = [MidiEvent::new(3, 0, [0x90, 69, 127])];
    instrument_audio(&mut samples, |audio| {
        assert_eq!(
            bridge.process(audio, &InputEvents::from_buffer(&notes), None),
            Ok(Status::Continue)
        );
    });
    assert_eq!(samples[..4], [0.; 4]);
    assert!(samples[4..].iter().any(|s| *s != 0.));
    let mut invalid = EventBuffer::with_capacity(2);
    invalid.push(&ParamValueEvent::new(
        0,
        ClapId::new(1),
        Pckn::match_all(),
        1.,
    ));
    invalid.push(&MidiEvent::new(33, 0, [0x90, 69, 127]));
    instrument_audio(&mut samples, |audio| {
        assert_eq!(
            bridge.process(audio, &invalid.as_input(), None),
            Err(BridgeError::InvalidEvent)
        );
    });
    assert_eq!(samples, [0.; 32]);
    assert_eq!(bridge.processor().level(), 0.2);
    // Host-side buffer construction is prepared outside the measured callback.
    let mut ports = AudioPorts::with_capacity(1, 1);
    let buffers = ports.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_output_only([samples.as_mut_slice()]),
    }]);
    let mut audio = buffers.to_plugin_audio();
    let counts = allocation_counter::measure(|| {
        for _ in 0..10_000 {
            bridge.reset();
            assert!(
                bridge
                    .process(&mut audio, &InputEvents::from_buffer(&notes), None)
                    .is_ok()
            );
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
struct Wide {
    config: Option<ProcessConfig>,
}
impl Processor<f64> for Wide {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
        self.config = Some(config);
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.config = None;
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64>) -> Status {
        if let Err(error) = self.config.unwrap().validate_block(block) {
            block.clear_outputs();
            return Status::Fault(error);
        }
        for channel in 0..block.audio.output_channels() {
            for frame in 0..block.audio.frames() {
                let sample = block.audio.input(channel).unwrap()[frame];
                block.audio.output_mut(channel).unwrap()[frame] = sample * 2.;
            }
        }
        Status::Continue
    }
}
#[test]
fn native_f64_sixty_four_channels_preserve_precision_and_reject_wrong_shape() {
    let mut bridge = MainBusBridge::<_, f64>::prepare(
        Wide { config: None },
        ProcessConfig::new(48000., 8, 0, AudioLayout::new(64, 64).unwrap()).unwrap(),
        None,
    )
    .unwrap();
    let mut input = [[1.00000000000001; 8]; 64];
    let mut output = [[99.; 8]; 64];
    let mut in_ports = AudioPorts::with_capacity(64, 1);
    let mut out_ports = AudioPorts::with_capacity(64, 1);
    {
        let buffers = in_ports.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f64_input_only(
                input.iter_mut().map(InputChannel::variable),
            ),
        }]);
        let destinations = out_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f64_output_only(
                output.iter_mut().map(|c| c.as_mut_slice()),
            ),
        }]);
        let mut audio = destinations.to_plugin_audio_with_inputs(&buffers);
        assert_eq!(
            bridge.process(&mut audio, &InputEvents::empty(), None),
            Ok(Status::Continue)
        );
    }
    assert_eq!(output, [[2.00000000000002; 8]; 64]);
    let mut wrong = [9_f32; 8];
    instrument_audio(&mut wrong, |audio| {
        assert_eq!(
            bridge.process(audio, &InputEvents::empty(), None),
            Err(BridgeError::LayoutMismatch)
        );
    });
    assert_eq!(wrong, [0.; 8]);
}
