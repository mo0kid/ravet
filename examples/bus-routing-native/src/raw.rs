//! Explicit raw short-MIDI main/aux and fixed-delay consumers.
//!
//! Existing routing/FIFO kernels own DSP. Complete canonical raw messages are
//! observed without inventing MIDI-to-parameter or source-port routing. Distinct
//! complete globals and transient stopped observations follow native ownership.
use crate::{BusRouting, delay::FixedDelayRouting};
use ravet_foundation::{
    EventData, ParameterError, ParameterId, ParameterSchema, ParameterState, PrepareError,
    ProcessBlock, ProcessConfig, ProcessStatus, Processor, Sample, Tail,
};
use ravet_native::{
    NativeAudioParameterProcessor, NativeParameterProcessor, NativeParameterRoles,
    NativeTelemetryDescriptor, NativeTelemetryProcessor, NativeTelemetryValueV1,
};
const FIELDS: usize = 10;
const fn descriptor(id: u32, name: &'static str) -> NativeTelemetryDescriptor {
    NativeTelemetryDescriptor {
        id,
        name,
        unit: "",
        display_minimum: 0.,
        display_maximum: 1.,
    }
}
static TELEMETRY: [NativeTelemetryDescriptor; FIELDS] = [
    descriptor(1, "Processed frames"),
    descriptor(2, "Raw packets"),
    descriptor(3, "Last raw checksum"),
    descriptor(4, "Last raw classes"),
    descriptor(5, "Last raw count"),
    descriptor(6, "Acknowledged gain"),
    descriptor(7, "Acknowledged bypass"),
    descriptor(8, "Native transport flags"),
    descriptor(9, "Last mixed checksum"),
    descriptor(10, "Last mixed event count"),
];
#[derive(Default)]
struct Observation {
    frames: u64,
    packets: u64,
    checksum: u64,
    classes: u64,
    count: u64,
    transport: u64,
    mixed: u64,
    event_count: u64,
}
fn hash(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
}
impl Observation {
    fn observe<S: Sample>(&mut self, block: &ProcessBlock<'_, '_, '_, S>) {
        self.frames = self.frames.saturating_add(block.audio.frames() as u64);
        self.checksum = 0xcbf29ce484222325;
        self.classes = 0;
        self.count = 0;
        self.transport = u64::from(block.transport.sample_position.is_some())
            | (u64::from(block.transport.beat_position.is_some()) << 1)
            | (u64::from(block.transport.tempo.is_some()) << 2)
            | (u64::from(block.transport.playing.is_some()) << 3)
            | (u64::from(block.transport.discontinuity) << 4)
            | (u64::from(block.transport.details.is_some()) << 5);
        self.mixed = 0xcbf29ce484222325;
        self.event_count = block.events.len() as u64;
        for event in block.events {
            let offset = (event.frame_offset as u32).to_le_bytes();
            hash(&mut self.mixed, &offset);
            match event.data {
                EventData::Midi(message) => {
                    self.count += 1;
                    let bytes = message.as_bytes();
                    let status = bytes[0];
                    self.classes |= 1u64
                        << if status < 0xf0 {
                            status >> 4
                        } else {
                            16 + (status & 15)
                        };
                    hash(&mut self.checksum, &offset);
                    hash(&mut self.checksum, bytes);
                    hash(&mut self.mixed, &[14]);
                    hash(&mut self.mixed, bytes);
                }
                EventData::ParameterValue { id, value } => {
                    hash(&mut self.mixed, &[1]);
                    hash(&mut self.mixed, &id.0.to_le_bytes());
                    hash(&mut self.mixed, &value.to_bits().to_le_bytes());
                }
                _ => hash(&mut self.mixed, &[0]),
            }
        }
        self.packets = self.packets.saturating_add(self.count);
    }
    fn snapshot(&self, globals: &ParameterState<'_, 2>) -> [NativeTelemetryValueV1; FIELDS] {
        let samples = [
            self.frames,
            self.packets,
            self.checksum,
            self.classes,
            self.count,
            0,
            0,
            self.transport,
            self.mixed,
            self.event_count,
        ];
        std::array::from_fn(|index| NativeTelemetryValueV1 {
            id: 1 + index as u32,
            flags: 0,
            value: if index == 5 {
                f64::from(globals.values()[0])
            } else if index == 6 {
                f64::from(globals.values()[1])
            } else {
                0.
            },
            samples: samples[index],
        })
    }
}
/// Main mono/stereo, optionally the legacy detector plus monitor auxiliary pair.
/// Parameter DSP uses the existing routing kernel; MIDI is exact observation.
pub struct RawBusRouting<S> {
    kernel: BusRouting<S>,
    observation: Observation,
}
impl<S> Default for RawBusRouting<S> {
    fn default() -> Self {
        Self {
            kernel: BusRouting::default(),
            observation: Observation::default(),
        }
    }
}
/// Fixed mono/stereo delay with complete raw input observation and global state.
/// Legacy FIFO behavior remains; prepare/reset clear history, recall retains it.
pub struct RawDelayRouting<S: Copy + Default, const DELAY: usize> {
    kernel: FixedDelayRouting<S, DELAY>,
    observation: Observation,
}
impl<S: Copy + Default, const DELAY: usize> Default for RawDelayRouting<S, DELAY> {
    fn default() -> Self {
        Self {
            kernel: FixedDelayRouting::default(),
            observation: Observation::default(),
        }
    }
}
macro_rules! precision {
    ($sample:ty) => {
        impl Processor<$sample> for RawBusRouting<$sample> {
            fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                self.observation = Observation::default();
                self.kernel.prepare_selected(config, true)
            }
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                let result = self.kernel.process(block);
                if result == ProcessStatus::Continue {
                    self.observation.observe(block);
                }
                result
            }
            fn reset(&mut self) {
                self.kernel.reset();
                self.observation = Observation::default();
            }
            fn deactivate(&mut self) {
                self.kernel.deactivate();
            }
        }
        impl<const DELAY: usize> Processor<$sample> for RawDelayRouting<$sample, DELAY> {
            fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                self.observation = Observation::default();
                self.kernel.prepare(config)
            }
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                let result = self.kernel.process_selected(block, true);
                if result == ProcessStatus::Continue {
                    self.observation.observe(block);
                }
                result
            }
            fn reset(&mut self) {
                self.kernel.reset();
                self.observation = Observation::default();
            }
            fn deactivate(&mut self) {
                self.kernel.deactivate();
            }
            fn latency_samples(&self) -> u32 {
                self.kernel.latency_samples()
            }
            fn tail(&self) -> Tail {
                self.kernel.tail()
            }
        }
    };
}
precision!(f32);
precision!(f64);
macro_rules! companions {
    ($name:ident, [$($generic:tt)*], [$($argument:tt)*], $kernel:ty, $product:literal) => {
        impl<$($generic)*> NativeParameterProcessor<2> for $name<$($argument)*>
        where $kernel: NativeParameterProcessor<2> {
            const STATE_PRODUCT: [u8; 8] = *$product;
            const STATE_VERSION: u32 = 1;
            fn parameter_schema() -> ParameterSchema<'static, 2> { <$kernel>::parameter_schema() }
            fn parameter_roles(id: ParameterId) -> NativeParameterRoles { <$kernel>::parameter_roles(id) }
            fn parameter_snapshot(&self) -> ParameterState<'static, 2> { self.kernel.parameter_snapshot() }
            fn restore_parameter_snapshot(&mut self, state: &ParameterState<'static, 2>) -> Result<(), ParameterError> {
                self.kernel.restore_parameter_snapshot(state)
            }
        }
        impl<$($generic)*> NativeAudioParameterProcessor<2> for $name<$($argument)*>
        where $kernel: NativeAudioParameterProcessor<2> {
            fn audio_parameter_snapshot(&self) -> ParameterState<'static, 2> { self.kernel.audio_parameter_snapshot() }
            fn apply_audio_parameter_snapshot(&mut self, state: &ParameterState<'static, 2>) -> Result<(), ParameterError> {
                self.kernel.apply_audio_parameter_snapshot(state)
            }
        }
        impl<$($generic)*> NativeTelemetryProcessor<FIELDS> for $name<$($argument)*>
        where $kernel: NativeParameterProcessor<2> {
            fn telemetry_descriptors() -> &'static [NativeTelemetryDescriptor; FIELDS] { &TELEMETRY }
            fn audio_telemetry_snapshot(&self) -> [NativeTelemetryValueV1; FIELDS] {
                self.observation.snapshot(&self.kernel.parameter_snapshot())
            }
        }
    };
}
companions!(RawBusRouting, [S], [S], BusRouting<S>, b"RAVETRBS");
companions!(
    RawDelayRouting, [S: Copy + Default, const DELAY: usize], [S, DELAY],
    FixedDelayRouting<S, DELAY>, b"RAVETRDL"
);
#[allow(unsafe_code)]
mod exports {
    use super::{RawBusRouting, RawDelayRouting};
    macro_rules! exports {
        ($core:ident,$params:ident,$cache:ident,$audio:ident,$state:ident,$telemetry:ident,$p32:ty,$p64:ty) => {
            ravet_native::export_native_processor_v1!($core, $p32, $p64);
            ravet_native::export_native_parameters_v1!($params, $p32, $p64, 2);
            ravet_native::export_native_cached_parameters_v1!($cache, $p32, $p64, 2);
            ravet_native::export_native_audio_state_v1!($audio, $p32, $p64, 2);
            ravet_native::export_native_state_v1!($state, $p32, $p64, 2);
            ravet_native::export_native_telemetry_v1!($telemetry, $p32, $p64, 10);
        };
    }
    exports!(
        ravet_raw_bus_routing_processor_v1,
        ravet_raw_bus_routing_parameters_v1,
        ravet_raw_bus_routing_cached_parameters_v1,
        ravet_raw_bus_routing_audio_state_v1,
        ravet_raw_bus_routing_state_v1,
        ravet_raw_bus_routing_telemetry_v1,
        RawBusRouting<f32>,
        RawBusRouting<f64>
    );
    exports!(ravet_raw_delay8_processor_v1,ravet_raw_delay8_parameters_v1,ravet_raw_delay8_cached_parameters_v1,ravet_raw_delay8_audio_state_v1,ravet_raw_delay8_state_v1,ravet_raw_delay8_telemetry_v1,RawDelayRouting<f32,8>,RawDelayRouting<f64,8>);
    exports!(ravet_raw_delay16_processor_v1,ravet_raw_delay16_parameters_v1,ravet_raw_delay16_cached_parameters_v1,ravet_raw_delay16_audio_state_v1,ravet_raw_delay16_state_v1,ravet_raw_delay16_telemetry_v1,RawDelayRouting<f32,16>,RawDelayRouting<f64,16>);
}
pub use exports::*;

#[cfg(test)]
mod tests {
    use super::*;
    use ravet_foundation::{AudioBuffer, AudioLayout, Event, MidiMessage, ProcessError, Transport};
    fn midi(offset: usize, bytes: &[u8]) -> Event {
        Event {
            frame_offset: offset,
            data: EventData::Midi(MidiMessage::from_bytes(bytes).unwrap()),
        }
    }
    fn parameter(offset: usize, value: f32) -> Event {
        Event {
            frame_offset: offset,
            data: EventData::ParameterValue {
                id: ParameterId(1),
                value,
            },
        }
    }
    fn config(aux: bool, reversed: bool) -> ProcessConfig {
        let base = ProcessConfig::new(48000., 32, 1024, AudioLayout::MONO).unwrap();
        if !aux {
            return base;
        }
        let detector = AudioLayout::new(1, 0).unwrap();
        let output = AudioLayout::new(0, 2).unwrap();
        base.with_auxiliary_buses(&if reversed {
            [output, detector]
        } else {
            [detector, output]
        })
        .unwrap()
    }
    #[test]
    fn raw_main_f64_preserves_wide_samples_exact_source_order_and_legacy_layout_policy() {
        let mut legacy = BusRouting::<f64>::default();
        assert_eq!(
            legacy.prepare(config(false, false)),
            Err(PrepareError::UnsupportedLayout)
        );
        let mut raw = RawBusRouting::<f64>::default();
        raw.prepare(config(false, false)).unwrap();
        let input = [1. + f64::EPSILON; 5];
        let mut output = [77.; 5];
        let inputs = [&input[..]];
        let mut outputs = [&mut output[..]];
        let events = [parameter(0, 0.5), midi(0, &[0xc0, 1]), parameter(2, 2.)];
        let audio = AudioBuffer::separate(&inputs, &mut outputs, 4).unwrap();
        let mut block = ProcessBlock::new(
            audio,
            &events,
            Transport {
                sample_position: Some(-1),
                tempo: Some(120.),
                ..Transport::default()
            },
        );
        // The authoritative native adapter represents no aux as Some(empty).
        let mut empty_auxiliary = [];
        block.auxiliary = Some(&mut empty_auxiliary);
        assert_eq!(raw.process(&mut block), ProcessStatus::Continue);
        assert_eq!(
            output,
            [
                0.5 + f64::EPSILON / 2.,
                0.5 + f64::EPSILON / 2.,
                2. + 2. * f64::EPSILON,
                2. + 2. * f64::EPSILON,
                77.
            ]
        );
        assert_ne!(output[0], f64::from(output[0] as f32));
        let t = raw.audio_telemetry_snapshot();
        assert_eq!(t[0].samples, 4);
        assert_eq!(t[4].samples, 1);
        assert_eq!(t[7].samples, 5);
        assert_eq!(t[9].samples, 3);
        assert_eq!(t[5].value, 2.);
        assert_eq!(t[8].samples, 14211739903968617291);
    }
    macro_rules! auxiliary {
        ($name:ident,$sample:ty) => {
            #[test]
            fn $name() {
                for reversed in [false, true] {
                    let mut raw = RawBusRouting::<$sample>::default();
                    raw.prepare(config(true, reversed)).unwrap();
                    let main = [1., 2., 3., 4., 77.];
                    let detector = [0.25, 0.5, 0.75, 1., 77.];
                    let mut main_out = [77.; 5];
                    let mut aux_a = [77.; 5];
                    let mut aux_b = [77.; 5];
                    let inputs = [&main[..]];
                    let mut outputs = [&mut main_out[..]];
                    let detector_input = [&detector[..]];
                    let mut monitor_output = [&mut aux_a[..], &mut aux_b[..]];
                    let detector_bus = AudioBuffer::separate(&detector_input, &mut [], 4).unwrap();
                    let monitor_bus = AudioBuffer::separate(&[], &mut monitor_output, 4).unwrap();
                    let mut buses = if reversed {
                        [monitor_bus, detector_bus]
                    } else {
                        [detector_bus, monitor_bus]
                    };
                    let events = [midi(0, &[0xf7]), parameter(2, 2.), midi(2, &[0xb3, 74, 91])];
                    let audio = AudioBuffer::separate(&inputs, &mut outputs, 4).unwrap();
                    let mut block = ProcessBlock::new(audio, &events, Transport::default());
                    block.auxiliary = Some(&mut buses);
                    assert_eq!(raw.process(&mut block), ProcessStatus::Continue);
                    assert_eq!(main_out, [1., 2., 6., 8., 77.]);
                    assert_eq!(aux_a, [0.25, 0.5, 1.5, 2., 77.]);
                    assert_eq!(aux_a, aux_b);
                    assert_eq!(raw.audio_telemetry_snapshot()[4].samples, 2);
                }
            }
        };
    }
    auxiliary!(raw_aux32_both_directional_source_orders, f32);
    auxiliary!(raw_aux64_both_directional_source_orders, f64);
    fn render_delay<const D: usize>(
        raw: &mut RawDelayRouting<f64, D>,
        input: &[f64],
        output: &mut [f64],
        frames: usize,
        events: &[Event],
    ) -> ProcessStatus {
        let inputs = [input];
        let mut outputs = [output];
        let mut block = ProcessBlock::new(
            AudioBuffer::separate(&inputs, &mut outputs, frames).unwrap(),
            events,
            Transport::default(),
        );
        raw.process(&mut block)
    }
    #[test]
    fn late_raw_fault_preserves_authoritative_delay_fifo_globals_and_observation() {
        let mut raw = RawDelayRouting::<f64, 8>::default();
        raw.prepare(config(false, false)).unwrap();
        let mut output = [77.; 6];
        assert_eq!(
            render_delay(
                &mut raw,
                &[1. + f64::EPSILON, 0., 0., 0.],
                &mut output,
                4,
                &[midi(0, &[0xf8])]
            ),
            ProcessStatus::Continue
        );
        assert_eq!(output, [0., 0., 0., 0., 77., 77.]);
        let before = raw.audio_telemetry_snapshot();
        assert!(matches!(
            render_delay(
                &mut raw,
                &[0.; 4],
                &mut output,
                4,
                &[parameter(0, 0.5), midi(4, &[0xfa])]
            ),
            ProcessStatus::Fault(_)
        ));
        assert_eq!(raw.audio_telemetry_snapshot(), before);
        assert_eq!(raw.parameter_snapshot().values(), &[1., 0.]);
        assert_eq!(output[4], 77.);
        assert_eq!(
            render_delay(&mut raw, &[0.; 5], &mut output, 5, &[]),
            ProcessStatus::Continue
        );
        assert_eq!(output, [0., 0., 0., 0., 1. + f64::EPSILON, 77.]);
    }
    #[test]
    fn raw_zero_frame_applies_controls_and_all_defined_short_statuses_without_fifo_advance() {
        let mut raw = RawDelayRouting::<f64, 8>::default();
        raw.prepare(config(false, false)).unwrap();
        let mut out = [77.; 4];
        assert_eq!(
            render_delay(&mut raw, &[1. + f64::EPSILON, 0., 0., 0.], &mut out, 4, &[]),
            ProcessStatus::Continue
        );
        let mut events = vec![parameter(0, 0.5)];
        for status in 0x80u8..=0xff {
            let length = if status < 0xf0 {
                if status & 0xe0 == 0xc0 { 2 } else { 3 }
            } else {
                match status {
                    0xf1 | 0xf3 => 2,
                    0xf2 => 3,
                    0xf6 | 0xf7 | 0xf8 | 0xfa | 0xfb | 0xfc | 0xfe | 0xff => 1,
                    _ => 0,
                }
            };
            if length > 0 {
                events.push(midi(0, &[status, 69, 127][..length]));
            }
        }
        let old_frames = raw.audio_telemetry_snapshot()[0].samples;
        assert_eq!(
            render_delay(&mut raw, &[], &mut [], 0, &events),
            ProcessStatus::Continue
        );
        let snapshot = raw.audio_telemetry_snapshot();
        assert_eq!(snapshot[0].samples, old_frames);
        assert_eq!(snapshot[4].samples, 123);
        assert_eq!(snapshot[2].samples, 15738453561344363854);
        assert_eq!(snapshot[3].samples, 3721297664);
        assert_eq!(snapshot[5].value, 0.5);
        let mut output = [77.; 6];
        assert_eq!(
            render_delay(&mut raw, &[0.; 5], &mut output, 5, &[]),
            ProcessStatus::Continue
        );
        assert_eq!(output[4], 0.5 + f64::EPSILON / 2.);
        let mut legacy = FixedDelayRouting::<f64, 8>::default();
        legacy.prepare(config(false, false)).unwrap();
        let inputs = [&[0.; 4][..]];
        let mut output = [77.; 4];
        let mut outputs = [&mut output[..]];
        let messages = [midi(0, &[0xf8])];
        let mut block = ProcessBlock::new(
            AudioBuffer::separate(&inputs, &mut outputs, 4).unwrap(),
            &messages,
            Transport::default(),
        );
        assert!(matches!(
            legacy.process(&mut block),
            ProcessStatus::Fault(ProcessError::Events(_))
        ));
    }
    #[test]
    fn six_companion_exports_retain_exact_selected_core_and_distinct_products() {
        for bits in [32, 64] {
            let bus = ravet_raw_bus_routing_processor_v1(bits);
            let d8 = ravet_raw_delay8_processor_v1(bits);
            let d16 = ravet_raw_delay16_processor_v1(bits);
            assert!(!bus.is_null() && !d8.is_null() && !d16.is_null());
            assert_ne!(bus, d8);
            assert_ne!(d8, d16);
            #[allow(unsafe_code)]
            unsafe {
                for (core, audio, telemetry) in [
                    (
                        bus,
                        ravet_raw_bus_routing_audio_state_v1(bits),
                        ravet_raw_bus_routing_telemetry_v1(bits),
                    ),
                    (
                        d8,
                        ravet_raw_delay8_audio_state_v1(bits),
                        ravet_raw_delay8_telemetry_v1(bits),
                    ),
                    (
                        d16,
                        ravet_raw_delay16_audio_state_v1(bits),
                        ravet_raw_delay16_telemetry_v1(bits),
                    ),
                ] {
                    assert_eq!(((*telemetry).processor_entry)(), core);
                    let cache = ((*audio).cached_parameters_entry)();
                    let parameters = ((*cache).parameters_entry)();
                    assert_eq!(((*parameters).processor_entry)(), core);
                }
            }
        }
        assert!(ravet_raw_bus_routing_processor_v1(16).is_null());
    }
}
