//! Fixed-capacity audio adaptation for board-owned callbacks.
//!
//! This `no_std` crate uses no allocator, OS service, atomics, HAL or device driver.
//! [`FixedBlockAdapter`] owns planar scratch in arrays, adapts interleaved f32 or
//! signed PCM, and preflights complete blocks before invoking a processor.
//! Processors remain responsible for their own allocation and timing behavior.
//! Board firmware owns DMA, cache coherency, codecs, interrupts and scheduling.
#![no_std]

use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Transport,
};

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for i16 {}
    impl Sealed for i32 {}
}

/// Interleaved sample encodings, sealed to f32 and full-width signed i16/i32 PCM.
///
/// Integer input divides by 2^15/2^31; output multiplies by that factor, truncates
/// toward zero and saturates. NaN output becomes zero; infinities saturate.
/// Packed or left/right-aligned 24-bit codec words need explicit board conversion.
pub trait PcmSample: sealed::Sealed + Copy + Default {
    fn to_f32(self) -> f32;
    fn from_f32(value: f32) -> Self;
}
impl PcmSample for f32 {
    fn to_f32(self) -> f32 {
        self
    }
    fn from_f32(value: f32) -> Self {
        value
    }
}
impl PcmSample for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / 32768.0
    }
    fn from_f32(value: f32) -> Self {
        (value * 32768.0) as i16
    }
}
impl PcmSample for i32 {
    fn to_f32(self) -> f32 {
        self as f32 / 2147483648.0
    }
    fn from_f32(value: f32) -> Self {
        (value * 2147483648.0) as i32
    }
}

/// Rejection silences the requested output prefix that fits the provided slice.
/// Backing tails outside that prefix remain unchanged. DSP is not invoked for
/// adapter/configuration/event errors; processor faults may advance histories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockError {
    InputTooShort,
    OutputTooShort,
    Process(ProcessError),
}
impl core::fmt::Display for BlockError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "embedded block error: {self:?}")
    }
}
impl core::error::Error for BlockError {}

/// Owned f32 processor and fixed planar scratch: 4*(INPUTS+OUTPUTS)*FRAMES bytes.
///
/// Prepare only while callbacks are stopped. Layout must exactly match the
/// const channel counts; auxiliary buses and generated events are excluded.
/// `maximum_frames` may be smaller than FRAMES. Zero-frame event flushes work.
/// Place large instances in board-owned static storage rather than a small ISR
/// stack. An exclusive mutable borrow must cover each callback; this provides
/// no cross-core/interrupt synchronization. No callback is split or retried.
pub struct FixedBlockAdapter<P, const INPUTS: usize, const OUTPUTS: usize, const FRAMES: usize> {
    processor: P,
    config: Option<ProcessConfig>,
    reset_error: Option<ProcessError>,
    input: [[f32; FRAMES]; INPUTS],
    output: [[f32; FRAMES]; OUTPUTS],
}
impl<P: Processor, const I: usize, const O: usize, const F: usize> FixedBlockAdapter<P, I, O, F> {
    /// Construct inactive scratch without allocating. The processor may own other storage.
    pub const fn new(processor: P) -> Self {
        Self {
            processor,
            config: None,
            reset_error: None,
            input: [[0.0; F]; I],
            output: [[0.0; F]; O],
        }
    }
    /// Validate capacities and prepare DSP. Any failure leaves the adapter inactive.
    /// Processor preparation may allocate; choose an allocation-free implementation
    /// for firmware without an allocator. No audio-owner session is started here.
    pub fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.deactivate();
        if F == 0 || config.maximum_frames() > F {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        if config.layout() != AudioLayout::new(I, O)?
            || O == 0
            || !config.auxiliary_buses().is_empty()
            || config.maximum_output_events() != 0
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.processor.prepare(config)?;
        self.config = Some(config);
        self.reset_error = None;
        Ok(())
    }
    /// Forward an optional explicit audio-owner session hook on the callback owner.
    /// Stop the session on that owner before preparation, transfer or deactivation.
    pub fn start_audio(&mut self) -> Result<(), ProcessError> {
        if self.config.is_none() {
            return Err(ProcessError::NotPrepared);
        }
        self.processor.start_audio()
    }
    pub fn stop_audio(&mut self) {
        self.processor.stop_audio();
    }
    pub fn reset(&mut self) {
        let _ = self.try_reset();
    }
    /// Forward checked reset; refused histories remain unavailable to processing.
    pub fn try_reset(&mut self) -> Result<(), ProcessError> {
        match self.processor.try_reset() {
            Ok(()) => {
                self.reset_error = None;
                Ok(())
            }
            Err(error) => {
                self.reset_error.get_or_insert(error);
                Err(error)
            }
        }
    }
    pub fn reset_error(&self) -> Option<ProcessError> {
        self.reset_error
    }
    pub fn deactivate(&mut self) {
        self.config = None;
        self.processor.deactivate();
    }
    pub fn processor(&self) -> &P {
        &self.processor
    }

    /// Adapt a complete borrowed block. Input and output may use different encodings.
    ///
    /// Required lengths are frames*channel count; excess input/output tails are
    /// untouched. Instruments supply an empty input slice. Events retain offsets
    /// and same-frame order. Transport passes through without clock inference.
    /// Every rejection silences available active output, including a processor
    /// fault even if its implementation failed to clear its scratch output.
    pub fn process_interleaved<In: PcmSample, Out: PcmSample>(
        &mut self,
        input: &[In],
        output: &mut [Out],
        frames: usize,
        events: &[Event],
        transport: Transport,
    ) -> Result<ProcessStatus, BlockError> {
        let result = self.process_inner(input, output, frames, events, transport);
        if result.is_err() || result == Ok(ProcessStatus::Silence) {
            let active = frames.saturating_mul(O).min(output.len());
            output[..active].fill(Out::default());
        }
        result
    }
    fn process_inner<In: PcmSample, Out: PcmSample>(
        &mut self,
        input: &[In],
        output: &mut [Out],
        frames: usize,
        events: &[Event],
        transport: Transport,
    ) -> Result<ProcessStatus, BlockError> {
        let config = self
            .config
            .ok_or(BlockError::Process(ProcessError::NotPrepared))?;
        if frames > config.maximum_frames() {
            return Err(BlockError::Process(ProcessError::TooManyFrames {
                frames,
                maximum: config.maximum_frames(),
            }));
        }
        if frames.checked_mul(I).is_none_or(|n| input.len() < n) {
            return Err(BlockError::InputTooShort);
        }
        if frames.checked_mul(O).is_none_or(|n| output.len() < n) {
            return Err(BlockError::OutputTooShort);
        }
        let inputs = self.input.each_ref().map(|c| &c[..]);
        let mut outputs = self.output.each_mut().map(|c| &mut c[..]);
        let audio = AudioBuffer::separate(&inputs, &mut outputs, frames).expect("checked capacity");
        let block = ProcessBlock::new(audio, events, transport);
        config.validate_block(&block).map_err(BlockError::Process)?;
        if let Some(error) = self.reset_error {
            return Err(BlockError::Process(error));
        }
        // End the preflight borrows before copying input. No DSP/event mutation yet.
        for frame in 0..frames {
            for channel in 0..I {
                self.input[channel][frame] = input[frame * I + channel].to_f32();
            }
        }
        let inputs = self.input.each_ref().map(|c| &c[..]);
        let mut outputs = self.output.each_mut().map(|c| &mut c[..]);
        let audio = AudioBuffer::separate(&inputs, &mut outputs, frames).expect("checked capacity");
        let mut block = ProcessBlock::new(audio, events, transport);
        let status = self.processor.process(&mut block);
        if let ProcessStatus::Fault(error) = status {
            return Err(BlockError::Process(error));
        }
        for frame in 0..frames {
            for channel in 0..O {
                output[frame * O + channel] = Out::from_f32(self.output[channel][frame]);
            }
        }
        Ok(status)
    }
}
