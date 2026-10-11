//! Main mono/stereo gain and mono detector-to-aux monitoring reference.
//!
//! Exactly one input-only mono detector and one output-only mono/stereo auxiliary
//! are accepted in either source order. Global Gain/Bypass state is separate from
//! a transient exact processed-frame telemetry counter. Prepare/reset clear the
//! counter; complete parameter recall preserves it. No audible history marker,
//! generated events, inferred host routing, SDK dependency or deadline is supplied.
//!
//! The separate [`raw`] consumers admit main-only or these auxiliary views with
//! canonical short MIDI observations and distinct complete state products. The
//! ordinary routing and fixed-delay policies keep their existing event contracts.
use ravet_foundation::{
    EventData, ParameterDescriptor, ParameterError, ParameterId, ParameterRange, ParameterScale,
    ParameterSchema, ParameterState, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor,
};
use ravet_native::{
    NativeAudioParameterProcessor, NativeParameterProcessor, NativeParameterRoles,
    NativeTelemetryDescriptor, NativeTelemetryProcessor, NativeTelemetryValueV1,
};
use std::marker::PhantomData;

pub const GAIN_ID: ParameterId = ParameterId(1);
pub const BYPASS_ID: ParameterId = ParameterId(2);
static PARAMETERS: [ParameterDescriptor<'static>; 2] = [
    ParameterDescriptor {
        id: GAIN_ID,
        name: "Gain",
        unit: "x",
        range: match ParameterRange::new(0., 4., ParameterScale::Linear, None) {
            Ok(range) => range,
            Err(_) => panic!("fixed gain range"),
        },
        default: 1.,
        automatable: true,
    },
    ParameterDescriptor {
        id: BYPASS_ID,
        name: "Bypass",
        unit: "",
        range: match ParameterRange::new(0., 1., ParameterScale::Linear, Some(1)) {
            Ok(range) => range,
            Err(_) => panic!("fixed bypass range"),
        },
        default: 0.,
        automatable: true,
    },
];
static TELEMETRY: [NativeTelemetryDescriptor; 1] = [NativeTelemetryDescriptor {
    id: 1,
    name: "Processed frames",
    unit: "frames",
    display_minimum: 0.,
    display_maximum: 1.,
}];

/// Portable routing reference, selected sample precision and explicit layout.
/// Sample values pass through unsanitized; finite global parameters are validated.
/// The exact frame counter saturates at u64::MAX, is transient, and is not a clock.
#[derive(Debug)]
pub struct BusRouting<S> {
    config: Option<ProcessConfig>,
    gain: f32,
    bypass: bool,
    detector: usize,
    auxiliary: usize,
    frames: u64,
    precision: PhantomData<S>,
}
impl<S> Default for BusRouting<S> {
    fn default() -> Self {
        Self {
            config: None,
            gain: 1.,
            bypass: false,
            detector: 0,
            auxiliary: 1,
            frames: 0,
            precision: PhantomData,
        }
    }
}
impl<S> BusRouting<S> {
    /// Exact transient frames processed since the last successful prepare/reset.
    /// Complete parameter recall preserves this saturating counter.
    pub fn processed_frames(&self) -> u64 {
        self.frames
    }
    fn apply(&mut self, data: EventData) {
        match data {
            EventData::ParameterValue { id: GAIN_ID, value } => self.gain = value.clamp(0., 4.),
            EventData::ParameterValue {
                id: BYPASS_ID,
                value,
            } => self.bypass = value >= 0.5,
            _ => {}
        }
    }
}
macro_rules! precision {
    ($sample:ty) => {
        impl BusRouting<$sample> {
            // Only the raw facade opts into main-only. Legacy prepare remains
            // strictly the original detector+auxiliary layout contract.
            fn prepare_selected(
                &mut self,
                config: ProcessConfig,
                allow_main: bool,
            ) -> Result<(), PrepareError> {
                self.config = None;
                let main = config.layout();
                if main.input_channels() != main.output_channels()
                    || !(1..=2).contains(&main.output_channels())
                    || config.maximum_output_events() != 0
                    || !(config.auxiliary_buses().len() == 2
                        || (allow_main && config.auxiliary_buses().is_empty()))
                {
                    return Err(PrepareError::UnsupportedLayout);
                }
                let mut detector = None;
                let mut output = None;
                for (index, bus) in config.auxiliary_buses().iter().enumerate() {
                    match (bus.input_channels(), bus.output_channels()) {
                        (1, 0) if detector.is_none() => detector = Some(index),
                        (0, 1..=2) if output.is_none() => output = Some(index),
                        _ => return Err(PrepareError::UnsupportedLayout),
                    }
                }
                if config.auxiliary_buses().is_empty() {
                    self.detector = 0;
                    self.auxiliary = 0;
                } else {
                    self.detector = detector.ok_or(PrepareError::UnsupportedLayout)?;
                    self.auxiliary = output.ok_or(PrepareError::UnsupportedLayout)?;
                }
                self.frames = 0;
                self.config = Some(config);
                Ok(())
            }
        }
        impl Processor<$sample> for BusRouting<$sample> {
            fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                self.prepare_selected(config, false)
            }
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                let validation = self
                    .config
                    .ok_or(ProcessError::NotPrepared)
                    .and_then(|config| config.validate_block(block));
                if let Err(error) = validation {
                    block.clear_outputs();
                    return ProcessStatus::Fault(error);
                }
                let frames = block.audio.frames();
                let mut next = 0;
                if frames == 0 {
                    for event in block.events {
                        self.apply(event.data);
                    }
                }
                for frame in 0..frames {
                    while let Some(event) = block.events.get(next) {
                        if event.frame_offset != frame {
                            break;
                        }
                        self.apply(event.data);
                        next += 1;
                    }
                    let gain = if self.bypass { 1. } else { self.gain } as $sample;
                    let detector = block
                        .auxiliary
                        .as_deref()
                        .and_then(|buses| buses.get(self.detector))
                        .and_then(|bus| bus.input(0))
                        .and_then(|samples| samples.get(frame))
                        .copied()
                        .unwrap_or(0.);
                    for channel in 0..block.audio.output_channels() {
                        let sample = block
                            .audio
                            .input(channel)
                            .and_then(|samples| samples.get(frame))
                            .copied()
                            .unwrap_or(0.);
                        if let Some(output) = block.audio.output_mut(channel) {
                            output[frame] = sample * gain;
                        }
                    }
                    if let Some(auxiliary) = block
                        .auxiliary
                        .as_deref_mut()
                        .and_then(|buses| buses.get_mut(self.auxiliary))
                    {
                        for channel in 0..auxiliary.output_channels() {
                            if let Some(output) = auxiliary.output_mut(channel) {
                                output[frame] = detector * gain;
                            }
                        }
                    }
                }
                self.frames = self.frames.saturating_add(frames as u64);
                ProcessStatus::Continue
            }
            fn reset(&mut self) {
                self.frames = 0;
            }
            fn deactivate(&mut self) {
                self.config = None;
            }
        }
        impl NativeParameterProcessor<2> for BusRouting<$sample> {
            const STATE_PRODUCT: [u8; 8] = *b"RAVETBUS";
            const STATE_VERSION: u32 = 1;
            fn parameter_schema() -> ParameterSchema<'static, 2> {
                ParameterSchema::new(&PARAMETERS).expect("fixed schema")
            }
            fn parameter_roles(id: ParameterId) -> NativeParameterRoles {
                NativeParameterRoles {
                    bypass: id == BYPASS_ID,
                    read_only: false,
                }
            }
            fn parameter_snapshot(&self) -> ParameterState<'static, 2> {
                Self::parameter_schema()
                    .state([self.gain, if self.bypass { 1. } else { 0. }])
                    .expect("valid retained globals")
            }
            fn restore_parameter_snapshot(
                &mut self,
                state: &ParameterState<'static, 2>,
            ) -> Result<(), ParameterError> {
                let gain = state
                    .get(GAIN_ID)
                    .ok_or(ParameterError::WrongParameter(GAIN_ID))?;
                let bypass = state
                    .get(BYPASS_ID)
                    .ok_or(ParameterError::WrongParameter(BYPASS_ID))?;
                Self::parameter_schema().state([gain, bypass])?;
                self.gain = gain;
                self.bypass = bypass >= 0.5;
                Ok(())
            }
        }
        impl NativeAudioParameterProcessor<2> for BusRouting<$sample> {
            fn audio_parameter_snapshot(&self) -> ParameterState<'static, 2> {
                self.parameter_snapshot()
            }
            fn apply_audio_parameter_snapshot(
                &mut self,
                state: &ParameterState<'static, 2>,
            ) -> Result<(), ParameterError> {
                self.restore_parameter_snapshot(state)
            }
        }
        impl NativeTelemetryProcessor<1> for BusRouting<$sample> {
            fn telemetry_descriptors() -> &'static [NativeTelemetryDescriptor; 1] {
                &TELEMETRY
            }
            fn audio_telemetry_snapshot(&self) -> [NativeTelemetryValueV1; 1] {
                [NativeTelemetryValueV1 {
                    id: 1,
                    flags: 0,
                    value: 0.,
                    samples: self.frames,
                }]
            }
        }
    };
}
precision!(f32);
precision!(f64);

#[allow(unsafe_code)]
mod exports {
    use super::BusRouting;
    ravet_native::export_native_processor_v1!(
        ravet_bus_routing_processor_v1,
        BusRouting<f32>,
        BusRouting<f64>
    );
    ravet_native::export_native_parameters_v1!(
        ravet_bus_routing_parameters_v1,
        BusRouting<f32>,
        BusRouting<f64>,
        2
    );
    ravet_native::export_native_cached_parameters_v1!(
        ravet_bus_routing_cached_parameters_v1,
        BusRouting<f32>,
        BusRouting<f64>,
        2
    );
    ravet_native::export_native_audio_state_v1!(
        ravet_bus_routing_audio_state_v1,
        BusRouting<f32>,
        BusRouting<f64>,
        2
    );
    ravet_native::export_native_state_v1!(
        ravet_bus_routing_state_v1,
        BusRouting<f32>,
        BusRouting<f64>,
        2
    );
    ravet_native::export_native_telemetry_v1!(
        ravet_bus_routing_telemetry_v1,
        BusRouting<f32>,
        BusRouting<f64>,
        1
    );
}
pub use exports::*;

#[cfg(test)]
mod tests {
    use super::*;
    use ravet_foundation::{AudioBuffer, AudioLayout, Event, Transport};
    fn config(reversed: bool) -> ProcessConfig {
        let detector = AudioLayout::new(1, 0).unwrap();
        let auxiliary = AudioLayout::new(0, 2).unwrap();
        let buses = if reversed {
            [auxiliary, detector]
        } else {
            [detector, auxiliary]
        };
        ProcessConfig::new(48000., 4, 4, AudioLayout::MONO)
            .unwrap()
            .with_auxiliary_buses(&buses)
            .unwrap()
    }
    #[test]
    fn paired_native_exports_include_valid_transient_telemetry_metadata() {
        for bits in [32, 64] {
            assert!(!ravet_bus_routing_processor_v1(bits).is_null());
            assert!(!ravet_bus_routing_parameters_v1(bits).is_null());
            assert!(!ravet_bus_routing_cached_parameters_v1(bits).is_null());
            assert!(!ravet_bus_routing_audio_state_v1(bits).is_null());
            assert!(!ravet_bus_routing_state_v1(bits).is_null());
            assert!(!ravet_bus_routing_telemetry_v1(bits).is_null());
        }
        assert!(ravet_bus_routing_telemetry_v1(16).is_null());
    }
    #[test]
    fn prepared_layout_accepts_both_source_orders_and_rejects_wrong_aux_profiles() {
        for reversed in [false, true] {
            let mut processor = BusRouting::<f32>::default();
            assert!(processor.prepare(config(reversed)).is_ok());
            let mut precise = BusRouting::<f64>::default();
            assert!(precise.prepare(config(reversed)).is_ok());
        }
        let base = ProcessConfig::new(48000., 4, 4, AudioLayout::MONO).unwrap();
        for auxiliary in [
            vec![],
            vec![AudioLayout::new(1, 0).unwrap()],
            vec![
                AudioLayout::new(0, 1).unwrap(),
                AudioLayout::new(0, 2).unwrap(),
            ],
            vec![AudioLayout::MONO, AudioLayout::new(0, 1).unwrap()],
        ] {
            let mut processor = BusRouting::<f32>::default();
            assert_eq!(
                processor.prepare(base.with_auxiliary_buses(&auxiliary).unwrap()),
                Err(PrepareError::UnsupportedLayout)
            );
        }
    }
    #[test]
    fn timed_main_detector_monitoring_is_source_order_independent() {
        for reversed in [false, true] {
            let mut processor = BusRouting::<f32>::default();
            processor.prepare(config(reversed)).unwrap();
            let main = [1., 2., 3., 4., 77.];
            let detector = [0.25, 0.5, 0.75, 1., 77.];
            let mut main_out = [77.; 5];
            let mut aux_a = [77.; 5];
            let mut aux_b = [77.; 5];
            let input = [&main[..]];
            let mut output = [&mut main_out[..]];
            let detector_input = [&detector[..]];
            let mut aux_output = [&mut aux_a[..], &mut aux_b[..]];
            let detector_bus = AudioBuffer::separate(&detector_input, &mut [], 4).unwrap();
            let aux_bus = AudioBuffer::separate(&[], &mut aux_output, 4).unwrap();
            let mut buses = if reversed {
                [aux_bus, detector_bus]
            } else {
                [detector_bus, aux_bus]
            };
            let events = [
                Event {
                    frame_offset: 2,
                    data: EventData::ParameterValue {
                        id: GAIN_ID,
                        value: 2.,
                    },
                },
                Event {
                    frame_offset: 3,
                    data: EventData::ParameterValue {
                        id: BYPASS_ID,
                        value: 1.,
                    },
                },
            ];
            let audio = AudioBuffer::separate(&input, &mut output, 4).unwrap();
            let mut block = ProcessBlock::new(audio, &events, Transport::default());
            block.auxiliary = Some(&mut buses);
            assert_eq!(processor.process(&mut block), ProcessStatus::Continue);
            assert_eq!(main_out, [1., 2., 6., 4., 77.]);
            assert_eq!(aux_a, [0.25, 0.5, 1.5, 1., 77.]);
            assert_eq!(aux_b, aux_a);
            assert_eq!(processor.processed_frames(), 4);
            let telemetry = processor.audio_telemetry_snapshot();
            assert_eq!(telemetry[0].samples, 4);
            assert_eq!(telemetry[0].value, 0.);
            let state = <BusRouting<f32> as NativeParameterProcessor<2>>::parameter_schema()
                .state([0.5, 0.])
                .unwrap();
            processor.apply_audio_parameter_snapshot(&state).unwrap();
            assert_eq!(processor.processed_frames(), 4);
            assert_eq!(processor.audio_parameter_snapshot().get(GAIN_ID), Some(0.5));
            processor.reset();
            assert_eq!(processor.processed_frames(), 0);
        }
    }
    #[test]
    fn malformed_whole_layout_precedes_events_outputs_and_frame_history() {
        let mut processor = BusRouting::<f32>::default();
        processor.prepare(config(false)).unwrap();
        let main = [1.; 5];
        let mut output = [77.; 5];
        let inputs = [&main[..]];
        let mut outputs = [&mut output[..]];
        let events = [Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: GAIN_ID,
                value: 2.,
            },
        }];
        let audio = AudioBuffer::separate(&inputs, &mut outputs, 4).unwrap();
        let mut block = ProcessBlock::new(audio, &events, Transport::default());
        assert!(matches!(
            processor.process(&mut block),
            ProcessStatus::Fault(ProcessError::LayoutMismatch)
        ));
        assert_eq!(output, [0., 0., 0., 0., 77.]);
        assert_eq!(processor.processed_frames(), 0);
        assert_eq!(processor.parameter_snapshot().get(GAIN_ID), Some(1.));
    }
    #[test]
    fn f64_routes_without_audio_narrowing_and_preserves_tails() {
        let mut processor = BusRouting::<f64>::default();
        processor.prepare(config(true)).unwrap();
        let main = [1. + f64::EPSILON; 5];
        let detector = [2. + 2. * f64::EPSILON; 5];
        let mut main_out = [77.; 5];
        let mut aux_a = [77.; 5];
        let mut aux_b = [77.; 5];
        let inputs = [&main[..]];
        let mut outputs = [&mut main_out[..]];
        let detector_input = [&detector[..]];
        let mut aux_output = [&mut aux_a[..], &mut aux_b[..]];
        let aux = AudioBuffer::separate(&[], &mut aux_output, 4).unwrap();
        let side = AudioBuffer::separate(&detector_input, &mut [], 4).unwrap();
        let mut buses = [aux, side];
        let audio = AudioBuffer::separate(&inputs, &mut outputs, 4).unwrap();
        let mut block = ProcessBlock::new(audio, &[], Transport::default());
        block.auxiliary = Some(&mut buses);
        assert_eq!(processor.process(&mut block), ProcessStatus::Continue);
        assert_eq!(main_out, [main[0], main[0], main[0], main[0], 77.]);
        assert_eq!(
            aux_a,
            [detector[0], detector[0], detector[0], detector[0], 77.]
        );
        assert_eq!(aux_a, aux_b);
        assert_eq!(processor.processed_frames(), 4);
    }
}

/// Explicit ordinary short-note routing reference and independent native exports.
pub mod note;

/// Fixed-delay timing/state reference with distinct Delay8/Delay16 native origins.
pub mod delay;

/// Distinct raw-input main/auxiliary and timed-delay native consumers.
pub mod raw;
