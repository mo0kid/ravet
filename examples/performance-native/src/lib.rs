//! Public native 16-voice identified-performance sine reference.
//! Exact originating-library tables/handles and exclusive foreign ownership follow
//! `ravet-native`. Control acquisition/prepare/drop is separate from prepared audio.
//! The consumer selects mono/stereo instruments, f32/f64 and bounded completion
//! output. Optional global level state/cache/audio companions are supplied; there
//! is no codec for voices, implicit live recall, editor or format factory here.
use ravet_foundation::{
    ParameterError, ParameterSchema, ParameterState, PrepareError, ProcessBlock, ProcessConfig,
    ProcessStatus, Processor, Tail,
};
use ravet_instrument::{INSTRUMENT_LEVEL_ID, PerformanceSine};
use ravet_native::{NativeAudioParameterProcessor, NativeParameterProcessor};

macro_rules! facade {
    ($name:ident, $sample:ty) => {
        /// Precision-specific public reference owner; lifecycle forwarding adds no
        /// implicit processing/session/state scheduling.
        pub struct $name(PerformanceSine<16>);
        impl Default for $name {
            fn default() -> Self {
                Self(PerformanceSine::new().expect("fixed valid voice capacity"))
            }
        }
        impl Processor<$sample> for $name {
            fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                <PerformanceSine<16> as Processor<$sample>>::prepare(&mut self.0, config)
            }
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                <PerformanceSine<16> as Processor<$sample>>::process(&mut self.0, block)
            }
            fn reset(&mut self) {
                <PerformanceSine<16> as Processor<$sample>>::reset(&mut self.0);
            }
            fn deactivate(&mut self) {
                <PerformanceSine<16> as Processor<$sample>>::deactivate(&mut self.0);
            }
            fn tail(&self) -> Tail {
                Tail::Infinite
            }
        }
        impl NativeParameterProcessor<1> for $name {
            const STATE_PRODUCT: [u8; 8] = *b"RAVETPS\0";
            const STATE_VERSION: u32 = 1;
            fn parameter_schema() -> ParameterSchema<'static, 1> {
                PerformanceSine::<16>::parameter_schema()
            }
            fn parameter_snapshot(&self) -> ParameterState<'static, 1> {
                Self::parameter_schema()
                    .state([self.0.level()])
                    .expect("valid live level")
            }
            fn restore_parameter_snapshot(
                &mut self,
                state: &ParameterState<'static, 1>,
            ) -> Result<(), ParameterError> {
                let value = state
                    .get(INSTRUMENT_LEVEL_ID)
                    .ok_or(ParameterError::WrongParameter(INSTRUMENT_LEVEL_ID))?;
                Self::parameter_schema().state([value])?;
                self.0
                    .set_level(value)
                    .map_err(|_| ParameterError::InvalidValue(INSTRUMENT_LEVEL_ID))
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
    };
}
facade!(PerformanceNative32, f32);
facade!(PerformanceNative64, f64);

#[allow(unsafe_code)]
mod exports {
    use super::{PerformanceNative32, PerformanceNative64};
    ravet_native::export_native_processor_v1!(
        ravet_performance_processor_v1,
        PerformanceNative32,
        PerformanceNative64
    );
    ravet_native::export_native_parameters_v1!(
        ravet_performance_parameters_v1,
        PerformanceNative32,
        PerformanceNative64,
        1
    );
    ravet_native::export_native_cached_parameters_v1!(
        ravet_performance_cached_parameters_v1,
        PerformanceNative32,
        PerformanceNative64,
        1
    );
    ravet_native::export_native_audio_state_v1!(
        ravet_performance_audio_state_v1,
        PerformanceNative32,
        PerformanceNative64,
        1
    );
    #[cfg(target_has_atomic = "64")]
    ravet_native::export_native_state_v1!(
        ravet_performance_state_v1,
        PerformanceNative32,
        PerformanceNative64,
        1
    );
}
#[cfg(target_has_atomic = "64")]
pub use exports::ravet_performance_state_v1;
pub use exports::{
    ravet_performance_audio_state_v1, ravet_performance_cached_parameters_v1,
    ravet_performance_parameters_v1, ravet_performance_processor_v1,
};
