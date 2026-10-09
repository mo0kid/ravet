//! The first RAVET reference product: mono/stereo gain with sample-timed events.
//!
//! This is portable processor code, not a plug-in or an audio-device backend.
//! Gain uses a linear amplitude multiplier in 0..=4 (unity = 1); finite values
//! outside that range are clamped. Audio samples themselves are not sanitized.
//! Unknown parameter IDs and note events are ignored. Smoothing defaults to
//! 5 ms, is shared between channels, and advances once per frame. Reset and
//! reprepare snap to the latest requested gain. Transport does not affect gain.
//! Portable bypass switches at the event sample without a crossfade. Gain
//! smoothing continues while bypassed. Input/output peak and RMS measure each
//! successful nonempty block; nonfinite samples are flagged and counted as
//! silence in telemetry without sanitizing audio.
//! The [`state`] module provides a versioned product record for control-thread
//! persistence; the processor does not perform stream I/O.
//!
//! ```
//! use ravet::foundation::{AudioBuffer, AudioLayout, Event, EventData,
//!     ProcessBlock, ProcessConfig, ProcessStatus, Processor, Transport};
//! use ravet_gain::{GainProcessor, GAIN_ID};
//!
//! let mut gain = GainProcessor::new(0.0)?; // immediate automation
//! gain.prepare(ProcessConfig::new(48_000.0, 4, 4, AudioLayout::MONO)?)?;
//! let mut samples = [1.0, -1.0, 0.5, -0.5];
//! let events = [Event {
//!     frame_offset: 2,
//!     data: EventData::ParameterValue { id: GAIN_ID, value: 0.5 },
//! }];
//! let mut channels = [&mut samples[..]];
//! let audio = AudioBuffer::in_place(&mut channels, 4)?;
//! let mut block = ProcessBlock::new(audio, &events, Transport::default());
//! assert_eq!(gain.process(&mut block), ProcessStatus::Continue);
//! assert_eq!(samples, [1.0, -1.0, 0.25, -0.25]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use ravet::foundation::{
    EventData, ParameterDescriptor, ParameterId, ParameterRange, ParameterScale, ParameterSchema,
    ParameterState, PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus,
    Processor,
};
use ravet::signal::{
    LinearSmoother, MeterReading, PeakRmsMeter, SmoothingError, smoothing_samples,
};

#[cfg(target_has_atomic = "64")]
mod meter_transfer;
mod programs;
pub mod state;
#[cfg(target_has_atomic = "64")]
pub use meter_transfer::GainMeterMailbox;
pub use programs::{GAIN_PROGRAMS, factory_program_bank};

/// Permanent ID; changing the display name must not change this identity.
pub const GAIN_ID: ParameterId = ParameterId(1);
pub const MAXIMUM_GAIN: f32 = 4.0;
/// Permanent portable bypass ID. Values below 0.5 mean off; other finite values mean on.
pub const BYPASS_ID: ParameterId = ParameterId(2);

static PARAMETERS: [ParameterDescriptor<'static>; 2] = [
    ParameterDescriptor {
        id: GAIN_ID,
        name: "Gain",
        unit: "x",
        range: match ParameterRange::new(0.0, MAXIMUM_GAIN, ParameterScale::Linear, None) {
            Ok(range) => range,
            Err(_) => panic!("invalid gain range"),
        },
        default: 1.0,
        automatable: true,
    },
    ParameterDescriptor {
        id: BYPASS_ID,
        name: "Bypass",
        unit: "",
        range: match ParameterRange::new(0.0, 1.0, ParameterScale::Linear, Some(1)) {
            Ok(range) => range,
            Err(_) => panic!("invalid bypass range"),
        },
        default: 0.0,
        automatable: true,
    },
];

/// Shared gain/bypass product metadata, also exposed by the CLAP prototype.
pub fn parameter_schema() -> ParameterSchema<'static, 2> {
    ParameterSchema::new(&PARAMETERS).expect("static gain schema is valid")
}

/// One block's input/output sample peak and RMS per channel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GainMeters {
    pub input: [MeterReading; 2],
    pub output: [MeterReading; 2],
}

#[derive(Debug)]
pub struct GainProcessor {
    config: Option<ProcessConfig>,
    smoothing_ms: f64,
    smoother: LinearSmoother,
    bypassed: bool,
    meters: GainMeters,
}

impl Default for GainProcessor {
    fn default() -> Self {
        Self::new(5.0).expect("the default smoothing duration is valid")
    }
}

impl GainProcessor {
    /// Construct an inactive unity-gain processor. Preparation converts this
    /// nonnegative, finite duration to samples for the actual sample rate.
    pub fn new(smoothing_ms: f64) -> Result<Self, SmoothingError> {
        if !smoothing_ms.is_finite() || smoothing_ms < 0.0 {
            return Err(SmoothingError::InvalidTime);
        }
        Ok(Self {
            config: None,
            smoothing_ms,
            smoother: LinearSmoother::new(1.0, 0)?,
            bypassed: false,
            meters: GainMeters::default(),
        })
    }
    pub fn requested_gain(&self) -> f32 {
        self.smoother.target()
    }
    pub fn effective_gain(&self) -> f32 {
        self.smoother.current()
    }
    pub fn bypassed(&self) -> bool {
        self.bypassed
    }
    /// Last successful nonempty block; zero-frame and rejected blocks retain it.
    pub fn meters(&self) -> GainMeters {
        self.meters
    }
    /// Capture requested values, excluding smoothing progress and telemetry.
    pub fn parameter_state(&self) -> ParameterState<'static, 2> {
        parameter_schema()
            .state([self.requested_gain(), f32::from(self.bypassed)])
            .expect("processor values are valid")
    }
    /// Restore a complete gain/bypass snapshot, snapping smoothing. Schema and
    /// values are revalidated before mutation, even for a caller-created schema.
    pub fn restore_parameters(
        &mut self,
        state: &ParameterState<'_, 2>,
    ) -> Result<(), ravet::foundation::ParameterError> {
        let gain = state
            .get(GAIN_ID)
            .ok_or(ravet::foundation::ParameterError::WrongParameter(GAIN_ID))?;
        let bypass = state
            .get(BYPASS_ID)
            .ok_or(ravet::foundation::ParameterError::WrongParameter(BYPASS_ID))?;
        let state = parameter_schema().state([gain, bypass])?;
        let _ = self.smoother.set_target(state.values()[0]);
        self.smoother.reset();
        self.bypassed = state.values()[1] == 1.0;
        Ok(())
    }

    fn apply_event(&mut self, data: EventData) {
        if let EventData::ParameterValue { id: GAIN_ID, value } = data {
            // The whole stream was validated before this point; clamp physical
            // gain to the product range. No invalid float reaches the smoother.
            let _ = self.smoother.set_target(value.clamp(0.0, MAXIMUM_GAIN));
        }
        if let EventData::ParameterValue {
            id: BYPASS_ID,
            value,
        } = data
        {
            self.bypassed = value >= 0.5;
        }
    }
}

impl Processor for GainProcessor {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        let layout = config.layout();
        if layout.input_channels() != layout.output_channels()
            || !(1..=2).contains(&layout.output_channels())
            || !config.auxiliary_buses().is_empty()
            || config.maximum_output_events() != 0
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        let duration = smoothing_samples(self.smoothing_ms, config.sample_rate())
            .map_err(|_| PrepareError::InvalidSmoothingTime)?;
        self.smoother = LinearSmoother::new(self.requested_gain(), duration)
            .map_err(|_| PrepareError::InvalidSmoothingTime)?;
        self.config = Some(config);
        self.meters = GainMeters::default();
        Ok(())
    }

    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        let validation = match self.config {
            Some(config) => config.validate_block(block),
            None => Err(ProcessError::NotPrepared),
        };
        if let Err(error) = validation {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }

        let mut next_event = 0;
        let frames = block.audio.frames();
        let mut input_meters = [PeakRmsMeter::default(); 2];
        let mut output_meters = [PeakRmsMeter::default(); 2];
        if frames == 0 {
            for event in block.events {
                self.apply_event(event.data);
            }
        }
        for frame in 0..frames {
            while let Some(event) = block.events.get(next_event) {
                if event.frame_offset != frame {
                    break;
                }
                self.apply_event(event.data);
                next_event += 1;
            }
            let gain = self.smoother.next_value();
            for channel in 0..block.audio.output_channels() {
                // Read the input value before borrowing the matching output,
                // which supports both in-place and separate storage safely.
                let input = block
                    .audio
                    .input(channel)
                    .and_then(|samples| samples.get(frame))
                    .copied()
                    .unwrap_or(0.0);
                if let Some(output) = block.audio.output_mut(channel) {
                    output[frame] = if self.bypassed { input } else { input * gain };
                    input_meters[channel].push(input);
                    output_meters[channel].push(output[frame]);
                }
            }
        }
        if frames != 0 {
            self.meters = GainMeters {
                input: input_meters.map(|m| m.reading()),
                output: output_meters.map(|m| m.reading()),
            };
        }
        ProcessStatus::Continue
    }

    fn reset(&mut self) {
        self.smoother.reset();
        self.meters = GainMeters::default();
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
}

mod precision64;
pub use precision64::{GainMeters64, GainProcessor64};
