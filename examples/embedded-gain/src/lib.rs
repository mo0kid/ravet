//! Allocation-free stereo gain reference; gain changes are immediate (no smoothing).
//!
//! [`Gain`] shares the portable processor contract with desktop RAVET but needs
//! no signal/surface crate, allocator, atomics, HAL or OS. Gain ID zero accepts
//! finite amplitudes 0..=4; unknown events/IDs are ignored. C uses block-start gain.
//! The opt-in `firmware-panic` handler is for standalone bare-metal static libraries;
//! Rust firmware should supply its own handler and leave that feature disabled.
#![cfg_attr(not(feature = "std"), no_std)]

use ravet_foundation::{
    AudioLayout, EventData, ParameterId, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor,
};

pub const GAIN_ID: ParameterId = ParameterId(0);
pub const MAXIMUM_FRAMES: usize = 64;

/// C-layout reference state, exclusively owned by the callback/firmware.
/// Treat fields as opaque in C; initialize through the reference ABI.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Gain {
    sample_rate: f64,
    maximum_frames: u32,
    gain: f32,
}
impl Default for Gain {
    fn default() -> Self {
        Self::new()
    }
}
impl Gain {
    pub const fn new() -> Self {
        Self {
            sample_rate: 0.0,
            maximum_frames: 0,
            gain: 1.0,
        }
    }
    pub fn gain(&self) -> f32 {
        self.gain
    }
    /// Reject nonfinite/out-of-range amplitudes without mutation.
    pub fn set_gain(&mut self, gain: f32) -> bool {
        if !gain.is_finite() || !(0.0..=4.0).contains(&gain) {
            return false;
        }
        self.gain = gain;
        true
    }
    fn config(&self) -> Option<ProcessConfig> {
        ProcessConfig::new(
            self.sample_rate,
            self.maximum_frames as usize,
            8,
            AudioLayout::STEREO,
        )
        .ok()
    }
}
impl Processor for Gain {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.deactivate();
        if config.maximum_frames() > MAXIMUM_FRAMES {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        if config.layout() != AudioLayout::STEREO
            || !config.auxiliary_buses().is_empty()
            || config.maximum_output_events() != 0
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        // This reference has one fixed input-event capacity.
        if config.maximum_events() != 8 {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.sample_rate = config.sample_rate();
        self.maximum_frames = config.maximum_frames() as u32;
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        let error = self
            .config()
            .ok_or(ProcessError::NotPrepared)
            .and_then(|config| config.validate_block(block));
        if let Err(error) = error {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        // Validate the reference's narrower value range before any event mutation.
        if let Some(index) = block.events.iter().position(|e| {
            matches!(e.data,
            EventData::ParameterValue { id: GAIN_ID, value } if !(0.0..=4.0).contains(&value))
        }) {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::Events(
                ravet_foundation::EventError::InvalidValue { index },
            ));
        }
        let mut next = 0;
        let events = block.events;
        block.audio.map_stereo_frames(|frame, input| {
            while next < events.len() && events[next].frame_offset == frame {
                if let EventData::ParameterValue { id: GAIN_ID, value } = events[next].data {
                    self.gain = value;
                }
                next += 1;
            }
            [input[0] * self.gain, input[1] * self.gain]
        });
        // Zero-frame events change control state without advancing audio.
        for event in &events[next..] {
            if let EventData::ParameterValue { id: GAIN_ID, value } = event.data {
                self.gain = value;
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.sample_rate = 0.0;
        self.maximum_frames = 0;
    }
}

#[allow(unsafe_code)]
mod ffi;
pub use ffi::*;

#[cfg(all(feature = "firmware-panic", target_os = "none", not(test)))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
