//! Fixed Delay8/Delay16 reference for explicit host timing transactions.
//!
//! Level/Bypass complete global state is independent from the bounded FIFO.
//! Bypass skips Level multiplication while retaining delay. Prepare/reset clear
//! history; parameter recall preserves it. Switching prepared cores has no generic
//! FIFO migration or seamless handoff promise. Native timing is a declaration,
//! not host compensation acknowledgement or a host tail API.
use ravet_foundation::{
    EventData, EventError, ParameterDescriptor, ParameterError, ParameterId, ParameterRange,
    ParameterScale, ParameterSchema, ParameterState, PrepareError, ProcessBlock, ProcessConfig,
    ProcessError, ProcessStatus, Processor, Tail,
};
use ravet_native::{NativeAudioParameterProcessor, NativeParameterProcessor, NativeParameterRoles};
pub const LEVEL_ID: ParameterId = ParameterId(1);
pub const BYPASS_ID: ParameterId = ParameterId(2);
static PARAMETERS: [ParameterDescriptor<'static>; 2] = [
    ParameterDescriptor {
        id: LEVEL_ID,
        name: "Level",
        unit: "x",
        range: match ParameterRange::new(0., 1., ParameterScale::Linear, None) {
            Ok(v) => v,
            Err(_) => panic!("fixed level"),
        },
        default: 1.,
        automatable: true,
    },
    ParameterDescriptor {
        id: BYPASS_ID,
        name: "Bypass",
        unit: "",
        range: match ParameterRange::new(0., 1., ParameterScale::Linear, Some(1)) {
            Ok(v) => v,
            Err(_) => panic!("fixed bypass"),
        },
        default: 0.,
        automatable: true,
    },
];
/// Matching mono/stereo main effect with fixed bounded delay and no auxiliaries.
/// Sample audio remains unsanitized; complete control/event preflight is atomic.
#[derive(Debug)]
pub struct FixedDelayRouting<S: Copy + Default, const DELAY: usize> {
    config: Option<ProcessConfig>,
    level: f32,
    bypass: bool,
    fifo: [[S; DELAY]; 2],
    cursor: usize,
}
impl<S: Copy + Default, const DELAY: usize> Default for FixedDelayRouting<S, DELAY> {
    fn default() -> Self {
        Self {
            config: None,
            level: 1.,
            bypass: false,
            fifo: [[S::default(); DELAY]; 2],
            cursor: 0,
        }
    }
}
impl<S: Copy + Default, const DELAY: usize> FixedDelayRouting<S, DELAY> {
    fn apply(&mut self, data: EventData) {
        match data {
            EventData::ParameterValue {
                id: LEVEL_ID,
                value,
            } => self.level = value,
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
        impl<const DELAY: usize> FixedDelayRouting<$sample, DELAY> {
            // Shared original FIFO/render kernel; only raw facade admission
            // accepts complete canonical MIDI. Legacy event policy is unchanged.
            pub(super) fn process_selected(
                &mut self,
                block: &mut ProcessBlock<'_, '_, '_, $sample>,
                allow_raw: bool,
            ) -> ProcessStatus {
                let check = (|| {
                    self.config
                        .ok_or(ProcessError::NotPrepared)?
                        .validate_block(block)?;
                    for (index, event) in block.events.iter().enumerate() {
                        match event.data {
                            EventData::ParameterValue {
                                id: LEVEL_ID,
                                value,
                            } if (0. ..=1.).contains(&value) => {}
                            EventData::ParameterValue {
                                id: BYPASS_ID,
                                value,
                            } if value == 0. || value == 1. => {}
                            EventData::Midi(_) if allow_raw => {}
                            _ => {
                                return Err(ProcessError::Events(EventError::InvalidValue {
                                    index,
                                }));
                            }
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = check {
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
                    let level = if self.bypass { 1. } else { self.level } as $sample;
                    for channel in 0..block.audio.output_channels() {
                        let sample = block
                            .audio
                            .input(channel)
                            .and_then(|v| v.get(frame))
                            .copied()
                            .unwrap_or(0.);
                        let delayed = self.fifo[channel][self.cursor];
                        self.fifo[channel][self.cursor] = sample;
                        if let Some(output) = block.audio.output_mut(channel) {
                            output[frame] = delayed * level;
                        }
                    }
                    self.cursor = (self.cursor + 1) % DELAY;
                }
                ProcessStatus::Continue
            }
        }
        impl<const DELAY: usize> Processor<$sample> for FixedDelayRouting<$sample, DELAY> {
            fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                self.config = None;
                if !(1..=1024).contains(&DELAY)
                    || config.layout().input_channels() != config.layout().output_channels()
                    || !(1..=2).contains(&config.layout().output_channels())
                    || !config.auxiliary_buses().is_empty()
                    || config.maximum_output_events() != 0
                    || config.maximum_frames() > 1024
                    || config.maximum_events() > 1024
                {
                    return Err(PrepareError::UnsupportedLayout);
                }
                self.reset();
                self.config = Some(config);
                Ok(())
            }
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                self.process_selected(block, false)
            }
            fn reset(&mut self) {
                self.fifo = [[0.; DELAY]; 2];
                self.cursor = 0;
            }
            fn deactivate(&mut self) {
                self.config = None;
            }
            fn latency_samples(&self) -> u32 {
                DELAY as u32
            }
            fn tail(&self) -> Tail {
                Tail::Samples(DELAY as u64)
            }
        }
    };
}
precision!(f32);
precision!(f64);
impl<S: Copy + Default, const DELAY: usize> NativeParameterProcessor<2>
    for FixedDelayRouting<S, DELAY>
{
    const STATE_PRODUCT: [u8; 8] = *b"RAVETDLY";
    const STATE_VERSION: u32 = 1;
    fn parameter_schema() -> ParameterSchema<'static, 2> {
        ParameterSchema::new(&PARAMETERS).expect("fixed complete schema")
    }
    fn parameter_roles(id: ParameterId) -> NativeParameterRoles {
        NativeParameterRoles {
            bypass: id == BYPASS_ID,
            read_only: false,
        }
    }
    fn parameter_snapshot(&self) -> ParameterState<'static, 2> {
        Self::parameter_schema()
            .state([self.level, if self.bypass { 1. } else { 0. }])
            .expect("validated globals")
    }
    fn restore_parameter_snapshot(
        &mut self,
        state: &ParameterState<'static, 2>,
    ) -> Result<(), ParameterError> {
        let level = state
            .get(LEVEL_ID)
            .ok_or(ParameterError::WrongParameter(LEVEL_ID))?;
        let bypass = state
            .get(BYPASS_ID)
            .ok_or(ParameterError::WrongParameter(BYPASS_ID))?;
        Self::parameter_schema().state([level, bypass])?;
        self.level = level;
        self.bypass = bypass >= 0.5;
        Ok(())
    }
}
impl<S: Copy + Default, const DELAY: usize> NativeAudioParameterProcessor<2>
    for FixedDelayRouting<S, DELAY>
{
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
macro_rules! exports {
    ($module:ident,$delay:literal,$core:ident,$param:ident,$cache:ident,$audio:ident,$state:ident)=>{
        #[allow(unsafe_code)] mod $module {use super::FixedDelayRouting;
            ravet_native::export_native_processor_v1!($core,FixedDelayRouting<f32,$delay>,FixedDelayRouting<f64,$delay>);
            ravet_native::export_native_parameters_v1!($param,FixedDelayRouting<f32,$delay>,FixedDelayRouting<f64,$delay>,2);
            ravet_native::export_native_cached_parameters_v1!($cache,FixedDelayRouting<f32,$delay>,FixedDelayRouting<f64,$delay>,2);
            ravet_native::export_native_audio_state_v1!($audio,FixedDelayRouting<f32,$delay>,FixedDelayRouting<f64,$delay>,2);
            ravet_native::export_native_state_v1!($state,FixedDelayRouting<f32,$delay>,FixedDelayRouting<f64,$delay>,2);
        }pub use $module::*;
    }
}
exports!(
    eight,
    8,
    ravet_delay8_processor_v1,
    ravet_delay8_parameters_v1,
    ravet_delay8_cached_parameters_v1,
    ravet_delay8_audio_state_v1,
    ravet_delay8_state_v1
);
exports!(
    sixteen,
    16,
    ravet_delay16_processor_v1,
    ravet_delay16_parameters_v1,
    ravet_delay16_cached_parameters_v1,
    ravet_delay16_audio_state_v1,
    ravet_delay16_state_v1
);

#[cfg(test)]
mod tests {
    use super::*;
    use ravet_foundation::{AudioBuffer, AudioLayout, Event, Transport};
    fn config() -> ProcessConfig {
        ProcessConfig::new(48000., 32, 8, AudioLayout::MONO).unwrap()
    }
    fn render<const D: usize>(
        p: &mut FixedDelayRouting<f32, D>,
        input: &[f32],
        events: &[Event],
    ) -> Vec<f32> {
        let mut out = vec![99.; input.len()];
        let inputs = [input];
        let mut outputs = [out.as_mut_slice()];
        let mut block = ProcessBlock::new(
            AudioBuffer::separate(&inputs, &mut outputs, input.len()).unwrap(),
            events,
            Transport::default(),
        );
        assert_eq!(p.process(&mut block), ProcessStatus::Continue);
        out
    }
    #[test]
    fn exact_impulse_crosses_blocks_and_recall_preserves_fifo() {
        let mut p = FixedDelayRouting::<f32, 8>::default();
        p.prepare(config()).unwrap();
        assert_eq!(render(&mut p, &[1., 0., 0.], &[]), [0.; 3]);
        let state = FixedDelayRouting::<f32, 8>::parameter_schema()
            .state([0.25, 0.])
            .unwrap();
        p.apply_audio_parameter_snapshot(&state).unwrap();
        assert_eq!(
            render(&mut p, &[0.; 7], &[]),
            [0., 0., 0., 0., 0., 0.25, 0.]
        );
        assert_eq!(p.latency_samples(), 8);
        assert_eq!(p.tail(), Tail::Samples(8));
        p.reset();
        assert_eq!(render(&mut p, &[0.; 12], &[]), [0.; 12]);
    }
    #[test]
    fn bypass_retains_delay_and_exact_event_source_order() {
        let mut p = FixedDelayRouting::<f32, 8>::default();
        p.prepare(config()).unwrap();
        let events = [
            Event {
                frame_offset: 0,
                data: EventData::ParameterValue {
                    id: LEVEL_ID,
                    value: 0.,
                },
            },
            Event {
                frame_offset: 8,
                data: EventData::ParameterValue {
                    id: BYPASS_ID,
                    value: 1.,
                },
            },
        ];
        let mut input = [0.; 10];
        input[0] = 0.75;
        let output = render(&mut p, &input, &events);
        assert_eq!(output[8], 0.75);
        assert_eq!(p.latency_samples(), 8);
    }
    #[test]
    fn late_fault_preserves_fifo_and_globals() {
        let mut p = FixedDelayRouting::<f32, 8>::default();
        p.prepare(config()).unwrap();
        render(&mut p, &[1., 0., 0., 0.], &[]);
        let input = [0.; 4];
        let inputs = [input.as_slice()];
        let mut out = [19.; 4];
        let mut outputs = [out.as_mut_slice()];
        let events = [
            Event {
                frame_offset: 0,
                data: EventData::ParameterValue {
                    id: LEVEL_ID,
                    value: 0.5,
                },
            },
            Event {
                frame_offset: 3,
                data: EventData::ParameterValue {
                    id: ParameterId(99),
                    value: 1.,
                },
            },
        ];
        let mut block = ProcessBlock::new(
            AudioBuffer::separate(&inputs, &mut outputs, input.len()).unwrap(),
            &events,
            Transport::default(),
        );
        assert!(matches!(p.process(&mut block), ProcessStatus::Fault(_)));
        assert_eq!(out, [0.; 4]);
        assert_eq!(p.parameter_snapshot().get(LEVEL_ID), Some(1.));
        assert_eq!(render(&mut p, &[0.; 5], &[]), [0., 0., 0., 0., 1.]);
    }
    #[test]
    fn f64_delay16_and_paired_metadata_are_distinct() {
        let mut p = FixedDelayRouting::<f64, 16>::default();
        p.prepare(config()).unwrap();
        let mut input = [0.; 17];
        input[0] = 1. + 2f64.powi(-40);
        let inputs = [input.as_slice()];
        let mut out = [0.; 17];
        let mut outputs = [out.as_mut_slice()];
        let mut block = ProcessBlock::new(
            AudioBuffer::separate(&inputs, &mut outputs, input.len()).unwrap(),
            &[],
            Transport::default(),
        );
        assert_eq!(p.process(&mut block), ProcessStatus::Continue);
        assert_eq!(out[16], input[0]);
        assert_eq!(p.latency_samples(), 16);
        for bits in [32, 64] {
            assert!(!ravet_delay8_processor_v1(bits).is_null());
            assert!(!ravet_delay16_state_v1(bits).is_null());
            assert_ne!(
                ravet_delay8_processor_v1(bits),
                ravet_delay16_processor_v1(bits)
            );
        }
        assert!(ravet_delay8_processor_v1(16).is_null());
    }
    #[test]
    fn wrong_profile_and_zero_delay_are_unprepared() {
        let mut p = FixedDelayRouting::<f32, 0>::default();
        assert!(p.prepare(config()).is_err());
        let mut p = FixedDelayRouting::<f32, 8>::default();
        assert!(
            p.prepare(
                ProcessConfig::new(48000., 4, 2, AudioLayout::STEREO)
                    .unwrap()
                    .with_output_events(1)
            )
            .is_err()
        );
    }
}
