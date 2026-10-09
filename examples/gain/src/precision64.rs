//! Native f64 reference audio, sharing the existing gain control/state policy.
use crate::GainProcessor;
use ravet::foundation::{
    ParameterState, PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus,
    Processor,
};
use ravet::signal::{MeterReading64, PeakRmsMeter64, SmoothingError};
/// Native f64 levels from the last successful nonempty block.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GainMeters64 {
    pub input: [MeterReading64; 2],
    pub output: [MeterReading64; 2],
}
/// Mono/stereo f64 reference with the existing f32 gain/bypass parameter and
/// smoother semantics. Audio copying/multiplication and meters remain native f64;
/// the smoother's per-frame gain is promoted exactly. State bytes/IDs match f32.
/// Nonfinite audio propagates through DSP and counts as silence in telemetry.
/// Prepared processing/reset has no framework heap, lock or I/O operations.
#[derive(Debug)]
pub struct GainProcessor64 {
    inner: GainProcessor,
    meters: GainMeters64,
}
impl Default for GainProcessor64 {
    fn default() -> Self {
        Self::new(5.).expect("valid default smoothing")
    }
}
impl GainProcessor64 {
    pub fn new(smoothing_ms: f64) -> Result<Self, SmoothingError> {
        Ok(Self {
            inner: GainProcessor::new(smoothing_ms)?,
            meters: GainMeters64::default(),
        })
    }
    pub fn requested_gain(&self) -> f32 {
        self.inner.requested_gain()
    }
    pub fn effective_gain(&self) -> f32 {
        self.inner.effective_gain()
    }
    pub fn bypassed(&self) -> bool {
        self.inner.bypassed()
    }
    /// Rejected/zero-frame blocks retain the previous complete meter snapshot.
    pub fn meters(&self) -> GainMeters64 {
        self.meters
    }
    pub fn parameter_state(&self) -> ParameterState<'static, 2> {
        self.inner.parameter_state()
    }
    pub fn restore_parameters(
        &mut self,
        state: &ParameterState<'_, 2>,
    ) -> Result<(), ravet::foundation::ParameterError> {
        self.inner.restore_parameters(state)
    }
}
impl Processor<f64> for GainProcessor64 {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.inner.prepare(config)?;
        self.meters = GainMeters64::default();
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64>) -> ProcessStatus {
        let validation = match self.inner.config {
            Some(config) => config.validate_block(block),
            None => Err(ProcessError::NotPrepared),
        };
        if let Err(error) = validation {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let frames = block.audio.frames();
        let mut next_event = 0;
        let mut input_meters = [PeakRmsMeter64::default(); 2];
        let mut output_meters = [PeakRmsMeter64::default(); 2];
        if frames == 0 {
            for event in block.events {
                self.inner.apply_event(event.data);
            }
        }
        for frame in 0..frames {
            while let Some(event) = block.events.get(next_event) {
                if event.frame_offset != frame {
                    break;
                }
                self.inner.apply_event(event.data);
                next_event += 1;
            }
            let gain = f64::from(self.inner.smoother.next_value());
            for channel in 0..block.audio.output_channels() {
                let input = block
                    .audio
                    .input(channel)
                    .and_then(|samples| samples.get(frame))
                    .copied()
                    .unwrap_or(0.);
                if let Some(output) = block.audio.output_mut(channel) {
                    output[frame] = if self.inner.bypassed {
                        input
                    } else {
                        input * gain
                    };
                    input_meters[channel].push(input);
                    output_meters[channel].push(output[frame]);
                }
            }
        }
        if frames != 0 {
            self.meters = GainMeters64 {
                input: input_meters.map(|m| m.reading()),
                output: output_meters.map(|m| m.reading()),
            };
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.inner.reset();
        self.meters = GainMeters64::default();
    }
    fn deactivate(&mut self) {
        self.inner.deactivate();
    }
}
