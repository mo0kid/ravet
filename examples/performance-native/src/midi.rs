//! Distinct raw-MIDI synthesis reference. Program requests remain observable but
//! never implicitly replace acknowledged global Level. Complete state persists
//! only Level; transient voices/history and telemetry are not session state.
use ravet_foundation::{
    EventData, ParameterError, ParameterSchema, ParameterState, PrepareError, ProcessBlock,
    ProcessConfig, ProcessStatus, Processor, Tail,
};
use ravet_instrument::{
    INSTRUMENT_LEVEL_ID, SineInstrument, SineInstrument64, SineOptions, ZoneSide,
};
use ravet_native::{
    NativeAudioParameterProcessor, NativeParameterProcessor, NativeTelemetryDescriptor,
    NativeTelemetryProcessor, NativeTelemetryValueV1,
};

const FIELD_COUNT: usize = 23;
const fn descriptor(id: u32, name: &'static str) -> NativeTelemetryDescriptor {
    NativeTelemetryDescriptor {
        id,
        name,
        unit: "",
        display_minimum: 0.,
        display_maximum: 1.,
    }
}
static FIELDS: [NativeTelemetryDescriptor; FIELD_COUNT] = [
    descriptor(1, "Processed frames"),
    descriptor(2, "Raw packets"),
    descriptor(3, "Last raw checksum"),
    descriptor(4, "Last raw classes"),
    descriptor(5, "Last raw count"),
    descriptor(6, "Voice slot 0 lifecycle"),
    descriptor(7, "Acknowledged Level"),
    descriptor(8, "Program channel 0"),
    descriptor(9, "Program channel 1"),
    descriptor(10, "Program channel 2"),
    descriptor(11, "Program channel 3"),
    descriptor(12, "Program channel 4"),
    descriptor(13, "Program channel 5"),
    descriptor(14, "Program channel 6"),
    descriptor(15, "Program channel 7"),
    descriptor(16, "Program channel 8"),
    descriptor(17, "Program channel 9"),
    descriptor(18, "Program channel 10"),
    descriptor(19, "Program channel 11"),
    descriptor(20, "Program channel 12"),
    descriptor(21, "Program channel 13"),
    descriptor(22, "Program channel 14"),
    descriptor(23, "Program channel 15"),
];
#[derive(Default)]
struct Observation {
    frames: u64,
    packets: u64,
    checksum: u64,
    classes: u64,
    count: u64,
}
impl Observation {
    fn observe<S: ravet_foundation::Sample>(&mut self, block: &ProcessBlock<'_, '_, '_, S>) {
        self.frames = self.frames.saturating_add(block.audio.frames() as u64);
        self.checksum = 0xcbf29ce484222325;
        self.classes = 0;
        self.count = 0;
        for event in block.events {
            if let EventData::Midi(message) = event.data {
                self.count += 1;
                let status = message.as_bytes()[0];
                let class = if status < 0xf0 {
                    status >> 4
                } else {
                    16 + (status & 15)
                };
                self.classes |= 1u64 << class;
                for byte in (event.frame_offset as u32)
                    .to_le_bytes()
                    .iter()
                    .chain(message.as_bytes())
                {
                    self.checksum = (self.checksum ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
                }
            }
        }
        self.packets = self.packets.saturating_add(self.count);
    }
}
macro_rules! midi_facade {
    ($name:ident, $engine:ident, $sample:ty) => {
        /// Authoritative precision-specific raw-MIDI native consumer. CONTROL
        /// lifetime and prepared AUDIO use follow `ravet-native`; no host routing.
        pub struct $name {
            engine: $engine<16>,
            observation: Observation,
        }
        impl Default for $name {
            fn default() -> Self {
                Self {
                    engine: $engine::with_options(
                        ZoneSide::Lower,
                        SineOptions {
                            apply_midi_programs: false,
                        },
                    )
                    .expect("fixed voices"),
                    observation: Observation::default(),
                }
            }
        }
        impl Processor<$sample> for $name {
            fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                self.observation = Observation::default();
                self.engine.prepare(config)
            }
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                let result = self.engine.process(block);
                if result == ProcessStatus::Continue {
                    self.observation.observe(block);
                }
                result
            }
            fn reset(&mut self) {
                self.engine.reset();
                self.observation = Observation::default();
            }
            fn deactivate(&mut self) {
                self.engine.deactivate();
                self.observation = Observation::default();
            }
            fn tail(&self) -> Tail {
                Tail::Infinite
            }
        }
        impl NativeParameterProcessor<1> for $name {
            const STATE_PRODUCT: [u8; 8] = *b"RAVETMS\0";
            const STATE_VERSION: u32 = 1;
            fn parameter_schema() -> ParameterSchema<'static, 1> {
                $engine::<16>::parameter_schema()
            }
            fn parameter_snapshot(&self) -> ParameterState<'static, 1> {
                Self::parameter_schema()
                    .state([self.engine.level()])
                    .expect("valid Level")
            }
            fn restore_parameter_snapshot(
                &mut self,
                state: &ParameterState<'static, 1>,
            ) -> Result<(), ParameterError> {
                let value = state
                    .get(INSTRUMENT_LEVEL_ID)
                    .ok_or(ParameterError::WrongParameter(INSTRUMENT_LEVEL_ID))?;
                Self::parameter_schema().state([value])?;
                self.engine.set_level(value)
            }
        }
        impl NativeAudioParameterProcessor<1> for $name {
            fn audio_parameter_snapshot(&self) -> ParameterState<'static, 1> {
                self.parameter_snapshot()
            }
            fn apply_audio_parameter_snapshot(
                &mut self,
                state: &ParameterState<'static, 1>,
            ) -> Result<(), ParameterError> {
                self.restore_parameter_snapshot(state)
            }
        }
        impl NativeTelemetryProcessor<FIELD_COUNT> for $name {
            fn telemetry_descriptors() -> &'static [NativeTelemetryDescriptor; FIELD_COUNT] {
                &FIELDS
            }
            fn audio_telemetry_snapshot(&self) -> [NativeTelemetryValueV1; FIELD_COUNT] {
                let mut result = std::array::from_fn(|i| NativeTelemetryValueV1 {
                    id: (i + 1) as u32,
                    flags: 0,
                    value: 0.,
                    samples: 0,
                });
                for (slot, value) in result.iter_mut().zip([
                    self.observation.frames,
                    self.observation.packets,
                    self.observation.checksum,
                    self.observation.classes,
                    self.observation.count,
                ]) {
                    slot.samples = value;
                }
                let voice = self.engine.voices()[0];
                result[5].value = f64::from(voice.velocity);
                result[5].samples = (voice.phase as u64) << 16
                    | voice.note.map_or(0, |note| {
                        1 + (u64::from(note.channel) << 8 | u64::from(note.key))
                    });
                result[6].value = f64::from(self.engine.level());
                for channel in 0..16 {
                    result[7 + channel].samples = self
                        .engine
                        .last_program_request(channel as u8)
                        .map_or(0, |request| {
                            1 + (u64::from(request.bank) << 8 | u64::from(request.program))
                        });
                }
                result
            }
        }
    };
}
midi_facade!(MidiNative32, SineInstrument, f32);
midi_facade!(MidiNative64, SineInstrument64, f64);
#[allow(unsafe_code)]
mod exports {
    use super::{FIELD_COUNT, MidiNative32, MidiNative64};
    ravet_native::export_native_processor_v1!(
        ravet_midi_sine_processor_v1,
        MidiNative32,
        MidiNative64
    );
    ravet_native::export_native_parameters_v1!(
        ravet_midi_sine_parameters_v1,
        MidiNative32,
        MidiNative64,
        1
    );
    ravet_native::export_native_cached_parameters_v1!(
        ravet_midi_sine_cached_parameters_v1,
        MidiNative32,
        MidiNative64,
        1
    );
    ravet_native::export_native_audio_state_v1!(
        ravet_midi_sine_audio_state_v1,
        MidiNative32,
        MidiNative64,
        1
    );
    #[cfg(target_has_atomic = "64")]
    ravet_native::export_native_state_v1!(ravet_midi_sine_state_v1, MidiNative32, MidiNative64, 1);
    ravet_native::export_native_telemetry_v1!(
        ravet_midi_sine_telemetry_v1,
        MidiNative32,
        MidiNative64,
        FIELD_COUNT
    );
}
pub use exports::*;

#[cfg(test)]
mod tests {
    use super::*;
    use ravet_foundation::{AudioBuffer, AudioLayout, Event, MidiMessage, Transport};
    fn event(offset: usize, bytes: &[u8]) -> Event {
        Event {
            frame_offset: offset,
            data: EventData::Midi(MidiMessage::from_bytes(bytes).unwrap()),
        }
    }
    fn render<S: ravet_foundation::Sample, P: Processor<S>>(
        processor: &mut P,
        output: &mut [S],
        events: &[Event],
    ) -> ProcessStatus {
        let frames = output.len();
        let mut channels = [output];
        let audio = AudioBuffer::separate(&[], &mut channels, frames).unwrap();
        processor.process(&mut ProcessBlock::new(audio, events, Transport::default()))
    }
    fn config() -> ProcessConfig {
        ProcessConfig::new(48000., 1024, 1024, AudioLayout::MONO_INSTRUMENT).unwrap()
    }
    #[test]
    fn true_f64_shared_kernel_and_unchanged_f32_scalar_output() {
        // Unconstrained existing constructor still infers the original concrete type.
        let mut ordinary = SineInstrument::<16>::new(ZoneSide::Lower).unwrap();
        let mut raw32 = MidiNative32::default();
        let mut raw64 = MidiNative64::default();
        ordinary.prepare(config()).unwrap();
        raw32.prepare(config()).unwrap();
        raw64.prepare(config()).unwrap();
        let events = [event(3, &[0x90, 69, 127])];
        let mut a = [0.; 256];
        let mut b = [0.; 256];
        let mut c = [0.; 256];
        assert_eq!(
            render(&mut ordinary, &mut a, &events),
            ProcessStatus::Continue
        );
        assert_eq!(render(&mut raw32, &mut b, &events), ProcessStatus::Continue);
        assert_eq!(render(&mut raw64, &mut c, &events), ProcessStatus::Continue);
        assert_eq!(a, b);
        assert!(c.iter().any(|sample| *sample != f64::from(*sample as f32)));
        for (n, (&x, &y)) in b.iter().zip(&c).enumerate() {
            assert_eq!(x, y as f32);
            if (3..243).contains(&n) {
                let phase = (n - 3) as f64 * 440. / 48000.;
                let expected = (phase * std::f64::consts::TAU).sin() * (n - 2) as f64 / 240.
                    * f64::from(0.2f32)
                    / 16.;
                assert!((y - expected).abs() < 1e-15);
            }
        }
    }
    #[test]
    fn raw_program_bank_request_preserved_without_global_state_drift() {
        let mut raw = MidiNative32::default();
        raw.prepare(config()).unwrap();
        let state = MidiNative32::parameter_schema().state([0.4]).unwrap();
        raw.restore_parameter_snapshot(&state).unwrap();
        let events = [
            event(0, &[0x90, 69, 127]),
            event(1, &[0xb0, 0, 7]),
            event(2, &[0xb0, 32, 11]),
            event(3, &[0xc0, 2]),
        ];
        let mut output = [0.; 16];
        assert_eq!(
            render(&mut raw, &mut output, &events),
            ProcessStatus::Continue
        );
        assert_eq!(raw.engine.level(), 0.4);
        assert_eq!(raw.parameter_snapshot().values(), &[0.4]);
        let request = raw.engine.last_program_request(0).unwrap();
        assert_eq!((request.bank, request.program), (7 * 128 + 11, 2));
        let snapshot = raw.audio_telemetry_snapshot();
        assert_eq!(snapshot[7].samples, 1 + ((7 * 128 + 11) << 8 | 2));
        assert_eq!(snapshot[1].samples, 4);
        assert_eq!(snapshot[4].samples, 4);
        assert!(raw.engine.voices().iter().any(|voice| voice.note.is_some()));
        raw.restore_parameter_snapshot(&MidiNative32::parameter_schema().state([0.7]).unwrap())
            .unwrap();
        assert_eq!(raw.engine.last_program_request(0), Some(request));
        assert_eq!(raw.engine.level(), 0.7);
        let known = [
            event(0, &[0xb0, 0, 0]),
            event(0, &[0xb0, 32, 0]),
            event(0, &[0xc0, 2]),
        ];
        let mut empty = [];
        assert_eq!(
            render(&mut raw, &mut empty, &known),
            ProcessStatus::Continue
        );
        assert_eq!(
            raw.engine.level(),
            0.7,
            "known Full program must not replace acknowledged Level"
        );
        let known_request = raw.engine.last_program_request(0).unwrap();
        assert_eq!((known_request.bank, known_request.program), (0, 2));
        raw.reset();
        assert_eq!(raw.engine.last_program_request(0), None);
        assert_eq!(raw.engine.level(), 0.7);
        assert_eq!(raw.audio_telemetry_snapshot()[0].samples, 0);
    }
    #[test]
    fn all_defined_raw_classes_reach_observation_and_bad_late_offset_retains_history() {
        let bytes: &[&[u8]] = &[
            &[0x80, 69, 0],
            &[0x90, 69, 1],
            &[0xa0, 69, 17],
            &[0xb0, 64, 127],
            &[0xc0, 7],
            &[0xd0, 13],
            &[0xe0, 0, 64],
            &[0xf1, 7],
            &[0xf2, 3, 4],
            &[0xf3, 9],
            &[0xf6],
            &[0xf7],
            &[0xf8],
            &[0xfa],
            &[0xfb],
            &[0xfc],
            &[0xfe],
            &[0xff],
        ];
        let events: Vec<_> = bytes
            .iter()
            .enumerate()
            .map(|(i, bytes)| event(i, bytes))
            .collect();
        let mut raw = MidiNative64::default();
        let mut oracle = MidiNative64::default();
        raw.prepare(config()).unwrap();
        oracle.prepare(config()).unwrap();
        let mut output = [0.; 64];
        let mut expected = [0.; 64];
        assert_eq!(
            render(&mut raw, &mut output, &events),
            ProcessStatus::Continue
        );
        assert_eq!(
            render(&mut oracle, &mut expected, &events),
            ProcessStatus::Continue
        );
        assert_eq!(raw.audio_telemetry_snapshot()[4].samples, 18);
        let saved = raw.audio_telemetry_snapshot();
        let bad = [event(0, &[0x90, 72, 127]), event(64, &[0xff])];
        assert!(matches!(
            render(&mut raw, &mut output, &bad),
            ProcessStatus::Fault(_)
        ));
        assert_eq!(raw.audio_telemetry_snapshot(), saved);
        assert_eq!(render(&mut raw, &mut output, &[]), ProcessStatus::Continue);
        assert_eq!(
            render(&mut oracle, &mut expected, &[]),
            ProcessStatus::Continue
        );
        assert_eq!(output, expected);
    }
}
