use crate::{AudioBuffer, Event, EventError, OutputEvents, Sample, Transport};

/// Maximum channels per portable bus; processors may support a smaller subset.
pub const MAXIMUM_CHANNELS: usize = 64;
/// Maximum additional buses, excluding the main bus.
pub const MAXIMUM_AUXILIARY_BUSES: usize = 8;

/// A bus channel layout; zero inputs/outputs permit instrument/sidechain buses.
/// At least one direction must be nonempty, with at most 64 channels per direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioLayout {
    inputs: usize,
    outputs: usize,
}

impl AudioLayout {
    pub const MONO: Self = Self {
        inputs: 1,
        outputs: 1,
    };
    pub const STEREO: Self = Self {
        inputs: 2,
        outputs: 2,
    };
    pub const MONO_INSTRUMENT: Self = Self {
        inputs: 0,
        outputs: 1,
    };
    pub const STEREO_INSTRUMENT: Self = Self {
        inputs: 0,
        outputs: 2,
    };

    pub fn new(inputs: usize, outputs: usize) -> Result<Self, PrepareError> {
        if inputs > MAXIMUM_CHANNELS || outputs > MAXIMUM_CHANNELS || (inputs == 0 && outputs == 0)
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        Ok(Self { inputs, outputs })
    }
    pub fn input_channels(self) -> usize {
        self.inputs
    }
    pub fn output_channels(self) -> usize {
        self.outputs
    }
}

/// Small, nonallocating preparation errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrepareError {
    /// Stop an explicit audio-owner session before control-thread preparation.
    AudioSessionActive,
    InvalidSampleRate,
    InvalidMaximumFrames,
    UnsupportedLayout,
    InvalidSmoothingTime,
    TooManyAuxiliaryBuses,
}

/// Validated capacities; preparation may allocate up to these limits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcessConfig {
    sample_rate: f64,
    maximum_frames: usize,
    maximum_events: usize,
    layout: AudioLayout,
    auxiliary: [AudioLayout; MAXIMUM_AUXILIARY_BUSES],
    auxiliary_count: usize,
    maximum_output_events: usize,
}

impl ProcessConfig {
    pub fn new(
        sample_rate: f64,
        maximum_frames: usize,
        maximum_events: usize,
        layout: AudioLayout,
    ) -> Result<Self, PrepareError> {
        if !sample_rate.is_finite() || sample_rate <= 0.0 {
            return Err(PrepareError::InvalidSampleRate);
        }
        if maximum_frames == 0 {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        Ok(Self {
            sample_rate,
            maximum_frames,
            maximum_events,
            layout,
            auxiliary: [AudioLayout::MONO; MAXIMUM_AUXILIARY_BUSES],
            auxiliary_count: 0,
            maximum_output_events: 0,
        })
    }
    pub fn sample_rate(self) -> f64 {
        self.sample_rate
    }
    pub fn maximum_frames(self) -> usize {
        self.maximum_frames
    }
    pub fn maximum_events(self) -> usize {
        self.maximum_events
    }
    pub fn layout(self) -> AudioLayout {
        self.layout
    }

    /// Configure ordered auxiliary buses during preparation, without allocation.
    /// Each block must supply matching buses in this order. Processor implementations
    /// explicitly accept or reject these layouts; this does not enable host ports.
    pub fn with_auxiliary_buses(mut self, buses: &[AudioLayout]) -> Result<Self, PrepareError> {
        if buses.len() > MAXIMUM_AUXILIARY_BUSES {
            return Err(PrepareError::TooManyAuxiliaryBuses);
        }
        self.auxiliary[..buses.len()].copy_from_slice(buses);
        self.auxiliary_count = buses.len();
        Ok(self)
    }
    pub fn auxiliary_buses(&self) -> &[AudioLayout] {
        &self.auxiliary[..self.auxiliary_count]
    }
    /// Bound emitted events independently of input capacity. The caller supplies
    /// exactly this much storage before processing; a full writer rejects additional events.
    pub fn with_output_events(mut self, capacity: usize) -> Self {
        self.maximum_output_events = capacity;
        self
    }
    pub fn maximum_output_events(self) -> usize {
        self.maximum_output_events
    }

    /// Validate the entire block before applying events or changing samples.
    /// Event capacity is checked before traversing the event slice.
    pub fn validate_block<S: Sample>(
        &self,
        block: &ProcessBlock<'_, '_, '_, S>,
    ) -> Result<(), ProcessError> {
        let frames = block.audio.frames();
        if frames > self.maximum_frames {
            return Err(ProcessError::TooManyFrames {
                frames,
                maximum: self.maximum_frames,
            });
        }
        if block.audio.input_channels() != self.layout.inputs
            || block.audio.output_channels() != self.layout.outputs
        {
            return Err(ProcessError::LayoutMismatch);
        }
        let auxiliary = block.auxiliary.as_deref().unwrap_or(&[]);
        if auxiliary.len() != self.auxiliary_count {
            return Err(ProcessError::LayoutMismatch);
        }
        for (audio, layout) in auxiliary.iter().zip(self.auxiliary_buses()) {
            if audio.frames() != frames
                || audio.input_channels() != layout.inputs
                || audio.output_channels() != layout.outputs
            {
                return Err(ProcessError::LayoutMismatch);
            }
        }
        match &block.output_events {
            Some(output)
                if output.frames() == frames
                    && output.capacity() == self.maximum_output_events
                    && output.is_empty() => {}
            None if self.maximum_output_events == 0 => {}
            _ => return Err(ProcessError::OutputEventCapacity),
        }
        if block.events.len() > self.maximum_events {
            return Err(ProcessError::Events(EventError::TooMany {
                count: block.events.len(),
                maximum: self.maximum_events,
            }));
        }
        let mut previous = 0;
        for (index, event) in block.events.iter().enumerate() {
            if (frames == 0 && event.frame_offset != 0)
                || (frames > 0 && event.frame_offset >= frames)
            {
                return Err(ProcessError::Events(EventError::OffsetOutOfRange {
                    index,
                    offset: event.frame_offset,
                    frames,
                }));
            }
            if event.frame_offset < previous {
                return Err(ProcessError::Events(EventError::Unordered { index }));
            }
            if !event.data.valid() {
                return Err(ProcessError::Events(EventError::InvalidValue { index }));
            }
            previous = event.frame_offset;
        }
        Ok(())
    }
}

/// A callback's borrowed storage and event slice; no allocation is required.
#[derive(Debug)]
pub struct ProcessBlock<'channels, 'samples, 'events, S: Sample = f32> {
    pub audio: AudioBuffer<'channels, 'samples, S>,
    /// Additional buses in preparation order; absent means no auxiliary buses.
    pub auxiliary: Option<&'channels mut [AudioBuffer<'channels, 'samples, S>]>,
    /// Initially empty, bounded event output for this callback.
    pub output_events: Option<OutputEvents<'events>>,
    pub events: &'events [Event],
    pub transport: Transport,
}

impl<'channels, 'samples, 'events, S: Sample> ProcessBlock<'channels, 'samples, 'events, S> {
    pub fn new(
        audio: AudioBuffer<'channels, 'samples, S>,
        events: &'events [Event],
        transport: Transport,
    ) -> Self {
        Self {
            audio,
            auxiliary: None,
            output_events: None,
            events,
            transport,
        }
    }
    /// Clear all active outputs and discard emitted events on a rejected block.
    /// Input-only auxiliary buses and backing tails remain untouched.
    pub fn clear_outputs(&mut self) {
        self.audio.clear_outputs();
        if let Some(buses) = self.auxiliary.as_deref_mut() {
            for bus in buses {
                bus.clear_outputs();
            }
        }
        if let Some(events) = &mut self.output_events {
            events.clear();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessError {
    /// Native/backend execution failed; histories may have advanced. Silence
    /// output and stop/reset/rebuild under the owning session policy.
    BackendFailure,
    /// A format mapping cannot represent the supplied known transport context.
    /// Validation rejection before native/backend state changes.
    InvalidTransport,
    NotPrepared,
    LayoutMismatch,
    OutputEventCapacity,
    TooManyFrames {
        frames: usize,
        maximum: usize,
    },
    Events(EventError),
}

/// Faults require initialized silence; Continue makes no silence/tail claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessStatus {
    Continue,
    Silence,
    Fault(ProcessError),
}

/// A portable processor, exclusively owned by the processing thread.
///
/// Create -> prepare -> process/reset -> deactivate. Call prepare/deactivate
/// only after callbacks stop. Explicit session consumers optionally bracket
/// rendering/reset with start_audio -> stop_audio on the audio owner, ending
/// before transfer/deactivation. A failed prepare must leave the processor
/// inactive. Reset preserves configuration and requested parameter values,
/// clears history, and runs exclusively without allocation. Deactivate stops
/// prepared processing but may retain parameter values for reactivation.
///
/// Processing must validate layout, size, and the complete event stream before
/// mutating state. Validation faults reject before state changes, silence all
/// active outputs and discard emitted events using [`ProcessBlock::clear_outputs`].
/// BackendFailure also silences/discards outputs but cannot roll back native
/// histories; consumers stop/reset/rebuild under their session policy.
/// Valid processing initializes every output frame, consumes events even in
/// zero-frame blocks, and performs no allocation, deallocation, blocking, or
/// I/O. Unknown supported event kinds/parameter IDs may be ignored, as stated
/// by the processor. These are implementor obligations, not type guarantees.
/// No Sync bound is imposed. ABI panic containment belongs to format adapters.
pub trait Processor<S: Sample = f32> {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError>;
    /// Optional explicit audio-owner session start, after prepare and before
    /// rendering. Default is a no-op. Consumers using these hooks call once on
    /// the audio owner and stop there before transfer/deactivation. Failure must
    /// leave this processor stopped; earlier successful nodes may be stopped.
    /// Hooks follow prepared callback allocation/locking/I/O obligations. Existing
    /// adapters do not implicitly opt in; explicit graph sessions supply them.
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        Ok(())
    }
    /// End an explicitly started session on its audio owner, without deactivation
    /// or storage destruction. Default no-op; preparation/history may be retained.
    fn stop_audio(&mut self) {}
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus;
    fn reset(&mut self);
    fn deactivate(&mut self);
    /// Prepared algorithmic latency in samples. Changes are advertised by adapters
    /// using their format's restart/latency protocol outside the audio callback.
    fn latency_samples(&self) -> u32 {
        0
    }
    /// Decay policy after input stops. Adapters must map this to the host protocol.
    fn tail(&self) -> Tail {
        Tail::None
    }
}

/// A processor's declared tail; this does not cause the framework to drain audio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tail {
    #[default]
    None,
    Samples(u64),
    Infinite,
}

impl core::fmt::Display for PrepareError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::AudioSessionActive => "stop the audio-owner session before preparation",
            Self::InvalidSampleRate => "sample rate must be finite and positive",
            Self::InvalidMaximumFrames => "maximum frame count must be nonzero",
            Self::UnsupportedLayout => "processor does not support this channel layout",
            Self::TooManyAuxiliaryBuses => "too many auxiliary buses",
            Self::InvalidSmoothingTime => {
                "smoothing duration cannot be represented at this sample rate"
            }
        })
    }
}
impl core::error::Error for PrepareError {}

impl core::fmt::Display for ProcessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidTransport => f.write_str("transport context cannot be represented by the backend"),
            Self::BackendFailure => f.write_str("backend execution failed; histories may have advanced"),
            Self::NotPrepared => f.write_str("processor is not prepared"),
            Self::LayoutMismatch => {
                f.write_str("buffer channel counts do not match the prepared layout")
            }
            Self::TooManyFrames { frames, maximum } => {
                write!(f, "block has {frames} frames; capacity is {maximum}")
            }
            Self::OutputEventCapacity => f.write_str("output event storage is missing, nonempty, undersized or has the wrong frame count"),
            Self::Events(error) => core::fmt::Display::fmt(error, f),
        }
    }
}
impl core::error::Error for ProcessError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Events(error) => Some(error),
            _ => None,
        }
    }
}
