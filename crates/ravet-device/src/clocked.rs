//! Explicit bounded input-clock conversion; no implicit device fallback.
use super::{
    Counters, DeviceRenderer, DuplexStream, InputStatistics, MAXIMUM_FRAMES, OpenError,
    OutputStatistics, OutputStream, PreparedDuplex, open_input, output_callback,
};
use crate::sample_rate::{Direction, StreamOpenError, verified_stream};
use cpal::traits::DeviceTrait;
use ravet_audio_file::{AudioConsumer, AudioProducer, audio_ring};
use ravet_foundation::{MAXIMUM_CHANNELS, PrepareError, Processor};
use ravet_signal::{
    FifoClockConfig, FifoClockController, FifoClockError, ResamplingError, SincResampler,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// Prepared callback profile. Clock capacity must match the capture ring;
/// target is strictly inside it. Complete queued+prefetched occupancy is used,
/// capped at raw ring capacity for feedback. The internal converted ring never
/// crosses threads and is drained immediately by the existing duplex renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockedDuplexOptions {
    /// 1..=65536 output scratch frames; native buffers split at this bound.
    pub maximum_frames: usize,
    /// Even 16..=256 sinc taps per input channel.
    pub taps: usize,
    pub clock: FifoClockConfig,
}
impl Default for ClockedDuplexOptions {
    fn default() -> Self {
        Self {
            maximum_frames: 512,
            taps: 32,
            clock: FifoClockConfig::default(),
        }
    }
}
pub(crate) fn validate_configuration(
    input_rate: u32,
    output_rate: u32,
    input_channels: usize,
    output_channels: usize,
    ring_frames: usize,
    options: ClockedDuplexOptions,
) -> Result<(), ClockedDuplexError> {
    if !(1..=MAXIMUM_FRAMES).contains(&options.maximum_frames)
        || !(1..=MAXIMUM_CHANNELS).contains(&input_channels)
        || !(1..=MAXIMUM_CHANNELS).contains(&output_channels)
        || !(8000..=192000).contains(&input_rate)
        || !(8000..=192000).contains(&output_rate)
        || !ring_frames.is_power_of_two()
        || !(2..=65536).contains(&ring_frames)
        || options.clock.capacity_frames != ring_frames
    {
        return Err(ClockedDuplexError::InvalidConfiguration);
    }
    FifoClockController::new(options.clock).map_err(ClockedDuplexError::Clock)?;
    SincResampler::validate_variable(
        f64::from(input_rate),
        f64::from(output_rate),
        options.taps,
        options.clock.maximum_ppm,
    )
    .map_err(ClockedDuplexError::Resampler)
}
#[derive(Debug)]
pub enum ClockedDuplexError {
    InvalidConfiguration,
    Clock(FifoClockError),
    Resampler(ResamplingError),
    Processor(PrepareError),
}
impl std::fmt::Display for ClockedDuplexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "clocked duplex preparation: {self:?}")
    }
}
impl std::error::Error for ClockedDuplexError {}
#[derive(Debug)]
pub enum ClockedDuplexOpenError {
    Input(OpenError),
    Preparation(ClockedDuplexError),
    Output(cpal::Error),
    /// Output failed rate admission; both streams are dropped.
    /// Input failures remain in `Input(OpenError::SampleRateMismatch { .. })`.
    SampleRateMismatch {
        requested: u32,
        actual: u32,
    },
}
impl std::fmt::Display for ClockedDuplexOpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "clocked duplex opening: {self:?}")
    }
}
impl std::error::Error for ClockedDuplexOpenError {}
impl From<StreamOpenError> for ClockedDuplexOpenError {
    fn from(error: StreamOpenError) -> Self {
        match error {
            StreamOpenError::Backend(error) => Self::Output(error),
            StreamOpenError::SampleRateMismatch { requested, actual } => {
                Self::SampleRateMismatch { requested, actual }
            }
        }
    }
}

/// Independently loaded telemetry; these fields are not one atomic callback
/// snapshot. Initial/recovery priming is intentional zero input, not underflow.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClockedDuplexStatistics {
    pub priming_output_frames: u64,
    pub underflow_output_frames: u64,
    pub queued_input_frames: u64,
    pub correction_ppm: f64,
}
#[derive(Default)]
pub(super) struct ClockCounters {
    priming: AtomicU64,
    underflow: AtomicU64,
    queued: AtomicU64,
    ppm: AtomicU64,
}
impl ClockCounters {
    pub(super) fn snapshot(&self) -> ClockedDuplexStatistics {
        ClockedDuplexStatistics {
            priming_output_frames: self.priming.load(Ordering::Relaxed),
            underflow_output_frames: self.underflow.load(Ordering::Relaxed),
            queued_input_frames: self.queued.load(Ordering::Relaxed),
            correction_ppm: f64::from_bits(self.ppm.load(Ordering::Relaxed)),
        }
    }
}
/// Original preparation error and original processor/raw capture consumer.
/// Failed prepare is inactive; foreign lifecycle panics are exceptional.
pub struct PreparedClockedDuplexFailure<P> {
    processor: P,
    consumer: AudioConsumer,
    error: ClockedDuplexError,
}
impl<P> PreparedClockedDuplexFailure<P> {
    pub fn error(&self) -> &ClockedDuplexError {
        &self.error
    }
    pub fn into_parts(self) -> (P, AudioConsumer, ClockedDuplexError) {
        (self.processor, self.consumer, self.error)
    }
}
impl<P> std::fmt::Display for PreparedClockedDuplexFailure<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl<P> std::error::Error for PreparedClockedDuplexFailure<P> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
impl<P> std::fmt::Debug for PreparedClockedDuplexFailure<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedClockedDuplexFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
/// Prepared f32 duplex input-clock conversion with per-channel f64 sinc storage.
/// Input/output nominal rates are 8000..=192000 and the complete corrected ratio
/// must satisfy the sinc bounds. Preparation allocates; rendering allocates
/// nothing and gives the processor input at the output clock/rate.
/// Prime to target occupancy before consuming capture. Underflow fills remaining
/// converted input with zeros, clears converter/controller histories and reprimes;
/// processor history/output clock continues. Nonfinite audio propagates. Caller
/// owns callback serialization/lifecycle; no physical alignment, universal drift
/// stability, device recovery, headroom or native deadline guarantee.
pub struct PreparedClockedDuplex<P: Processor> {
    inner: PreparedDuplex<P>,
    raw: AudioConsumer,
    converted: AudioProducer,
    options: ClockedDuplexOptions,
    output_rate: u32,
    output_channels: usize,
    clock: FifoClockController,
    converters: Vec<SincResampler>,
    captured: Vec<f32>,
    raw_planar: Vec<Vec<f64>>,
    resampled: Vec<Vec<f64>>,
    interleaved: Vec<f32>,
    raw_frames: usize,
    raw_position: usize,
    primed: bool,
    pub(super) clock_counters: Arc<ClockCounters>,
}
impl<P: Processor> PreparedClockedDuplex<P> {
    pub fn new(
        processor: P,
        consumer: AudioConsumer,
        input_rate: u32,
        output_rate: u32,
        output_channels: usize,
        options: ClockedDuplexOptions,
    ) -> Result<Self, ClockedDuplexError> {
        Self::new_owned(
            processor,
            consumer,
            input_rate,
            output_rate,
            output_channels,
            options,
        )
        .map_err(|failure| failure.error)
    }
    /// Prepare on control with failure-owned original P and raw capture consumer.
    /// Fallible transient scratch reserves precede child preparation. Existing
    /// converter/controller bounds and initial priming remain unchanged.
    pub fn new_owned(
        processor: P,
        consumer: AudioConsumer,
        input_rate: u32,
        output_rate: u32,
        output_channels: usize,
        options: ClockedDuplexOptions,
    ) -> Result<Self, PreparedClockedDuplexFailure<P>> {
        let prepared = (|| {
            validate_configuration(
                input_rate,
                output_rate,
                consumer.channels(),
                output_channels,
                consumer.capacity_frames(),
                options,
            )?;
            let clock =
                FifoClockController::new(options.clock).map_err(ClockedDuplexError::Clock)?;
            let channels = consumer.channels();
            let mut converters = Vec::new();
            converters
                .try_reserve_exact(channels)
                .map_err(|_| ClockedDuplexError::InvalidConfiguration)?;
            for _ in 0..channels {
                converters.push(
                    SincResampler::new_variable(
                        f64::from(input_rate),
                        f64::from(output_rate),
                        options.taps,
                        options.clock.maximum_ppm,
                    )
                    .map_err(ClockedDuplexError::Resampler)?,
                );
            }
            let (converted, converted_consumer) =
                audio_ring(channels, options.maximum_frames.next_power_of_two())
                    .map_err(|_| ClockedDuplexError::InvalidConfiguration)?;
            let maximum = options.maximum_frames;
            let captured = super::input::scratch(maximum * channels, 0.0f32)
                .map_err(ClockedDuplexError::Processor)?;
            let interleaved = super::input::scratch(maximum * channels, 0.0f32)
                .map_err(ClockedDuplexError::Processor)?;
            let raw_planar = super::input::planar(channels, maximum, 0.0f64)
                .map_err(ClockedDuplexError::Processor)?;
            let resampled = super::input::planar(channels, maximum, 0.0f64)
                .map_err(ClockedDuplexError::Processor)?;
            let clock_counters = Arc::default();
            Ok((
                clock,
                converters,
                converted,
                converted_consumer,
                captured,
                interleaved,
                raw_planar,
                resampled,
                clock_counters,
            ))
        })();
        let (
            clock,
            converters,
            converted,
            converted_consumer,
            captured,
            interleaved,
            raw_planar,
            resampled,
            clock_counters,
        ) = match prepared {
            Ok(parts) => parts,
            Err(error) => {
                return Err(PreparedClockedDuplexFailure {
                    processor,
                    consumer,
                    error,
                });
            }
        };
        let inner = match PreparedDuplex::new_owned(
            processor,
            output_rate,
            converted_consumer,
            output_channels,
            options.maximum_frames,
        ) {
            Ok(inner) => inner,
            Err(failure) => {
                let (processor, _converted_consumer, error) = failure.into_parts();
                return Err(PreparedClockedDuplexFailure {
                    processor,
                    consumer,
                    error: ClockedDuplexError::Processor(error),
                });
            }
        };
        Ok(Self {
            inner,
            raw: consumer,
            converted,
            options,
            output_rate,
            output_channels,
            clock,
            converters,
            captured,
            raw_planar,
            resampled,
            interleaved,
            raw_frames: 0,
            raw_position: 0,
            primed: false,
            clock_counters,
        })
    }
    /// Consume exclusive stopped ownership on control, deactivate once and return
    /// original processor/raw consumer. Prefetched capture and converted transient
    /// histories are discarded; queued raw frames retain their existing position.
    /// This performs no seek, source-worker join or endpoint rollback.
    pub fn into_parts(self) -> (P, AudioConsumer) {
        let Self { inner, raw, .. } = self;
        (inner.into_processor(), raw)
    }
    pub fn into_processor(self) -> P {
        self.into_parts().0
    }
    pub fn statistics(&self) -> OutputStatistics {
        self.inner.statistics()
    }
    pub fn clock_statistics(&self) -> ClockedDuplexStatistics {
        self.clock_counters.snapshot()
    }
    fn convert(&mut self, frames: usize) {
        let channels = self.converters.len();
        for samples in &mut self.resampled {
            samples[..frames].fill(0.);
        }
        let queued = self.raw.queued_frames() + self.raw_frames - self.raw_position;
        self.clock_counters
            .queued
            .store(queued as u64, Ordering::Relaxed);
        if !self.primed {
            if queued < self.options.clock.target_frames {
                self.clock_counters
                    .priming
                    .fetch_add(frames as u64, Ordering::Relaxed);
                return;
            }
            self.primed = true;
        }
        let ppm = self
            .clock
            .observe(
                queued.min(self.options.clock.capacity_frames),
                frames as f64 / f64::from(self.output_rate),
            )
            .expect("prepared clock observation");
        self.clock_counters
            .ppm
            .store(ppm.to_bits(), Ordering::Relaxed);
        for converter in &mut self.converters {
            converter
                .set_rate_adjustment_ppm(ppm)
                .expect("matched prepared correction");
        }
        let mut produced = 0;
        while produced < frames {
            // Drain any internally pending upsample output before dequeuing more
            // input. If raw scratch is exhausted, empty input is valid.
            let mut progress = None;
            for channel in 0..channels {
                let current = self.converters[channel].process(
                    &self.raw_planar[channel][self.raw_position..self.raw_frames],
                    &mut self.resampled[channel][produced..frames],
                );
                if let Some(expected) = progress {
                    assert_eq!(current, expected, "channel clock alignment");
                } else {
                    progress = Some(current);
                }
            }
            let progress = progress.expect("positive input channels");
            self.raw_position += progress.consumed;
            produced += progress.produced;
            if produced == frames {
                break;
            }
            if self.raw_position == self.raw_frames {
                self.raw_frames = self
                    .raw
                    .pop(&mut self.captured)
                    .expect("prepared complete frames");
                self.raw_position = 0;
                if self.raw_frames == 0 {
                    break;
                }
                for (frame, samples) in self.captured[..self.raw_frames * channels]
                    .chunks_exact(channels)
                    .enumerate()
                {
                    for (channel, sample) in samples.iter().enumerate() {
                        self.raw_planar[channel][frame] = f64::from(*sample);
                    }
                }
            } else {
                assert!(
                    progress.consumed > 0 || progress.produced > 0,
                    "bounded resampler progress"
                );
            }
        }
        if produced < frames {
            let missing = (frames - produced) as u64;
            self.clock_counters
                .underflow
                .fetch_add(missing, Ordering::Relaxed);
            self.inner
                .counters()
                .input_underflow_frames
                .fetch_add(missing, Ordering::Relaxed);
            self.primed = false;
            self.clock.reset();
            self.clock_counters.ppm.store(0, Ordering::Relaxed);
            for converter in &mut self.converters {
                converter.reset();
                converter.set_rate_adjustment_ppm(0.).expect("nominal rate");
            }
        }
    }
    /// Same malformed-frame policy as PreparedDuplex, without consuming capture.
    pub fn render(&mut self, output: &mut [f32]) {
        let outputs = self.output_channels;
        if !output.len().is_multiple_of(outputs) {
            self.inner.render(output);
            return;
        }
        self.inner
            .counters()
            .callbacks
            .fetch_add(1, Ordering::Relaxed);
        for chunk in output.chunks_mut(self.options.maximum_frames * outputs) {
            let frames = chunk.len() / outputs;
            self.convert(frames);
            let channels = self.converters.len();
            for (frame, samples) in self.interleaved[..frames * channels]
                .chunks_exact_mut(channels)
                .enumerate()
            {
                for (channel, sample) in samples.iter_mut().enumerate() {
                    *sample = self.resampled[channel][frame] as f32;
                }
            }
            assert_eq!(
                self.converted
                    .push(&self.interleaved[..frames * channels])
                    .expect("prepared converted frames"),
                frames
            );
            self.inner.render_block(chunk);
        }
    }
}
impl<P: Processor> DeviceRenderer for PreparedClockedDuplex<P> {
    fn render_device(&mut self, output: &mut [f32]) {
        self.render(output);
    }
    fn counters(&self) -> &Arc<Counters> {
        self.inner.counters()
    }
}
/// Explicit paired native streams; pause preserves queue/converter/DSP history.
/// Start/pause are sequential, not physically sample-synchronized. Reopen for
/// fresh state or backend failure/panic recovery; no fallback/hotplug is implicit.
pub struct ClockedDuplexStream {
    streams: DuplexStream,
    pub(super) clock_counters: Arc<ClockCounters>,
}
impl ClockedDuplexStream {
    pub fn play(&self) -> Result<(), cpal::Error> {
        self.streams.play()
    }
    pub fn pause(&self) -> Result<(), cpal::Error> {
        self.streams.pause()
    }
    pub fn input_statistics(&self) -> InputStatistics {
        self.streams.input_statistics()
    }
    pub fn output_statistics(&self) -> OutputStatistics {
        self.streams.output_statistics()
    }
    pub fn clock_statistics(&self) -> ClockedDuplexStatistics {
        self.clock_counters.snapshot()
    }
}
/// Open explicit f32 input/output devices using prepared clocked conversion.
/// Ring frames must match options.clock.capacity_frames. No implicit format,
/// device or output routing changes; both device rates are verified after building.
/// Mismatch drops both streams; streams start only on play().
pub fn open_clocked_duplex<P: Processor + Send + 'static>(
    input: &cpal::Device,
    output: &cpal::Device,
    input_config: cpal::StreamConfig,
    output_config: cpal::StreamConfig,
    ring_frames: usize,
    processor: P,
    options: ClockedDuplexOptions,
) -> Result<ClockedDuplexStream, ClockedDuplexOpenError> {
    validate_configuration(
        input_config.sample_rate,
        output_config.sample_rate,
        usize::from(input_config.channels),
        usize::from(output_config.channels),
        ring_frames,
        options,
    )
    .map_err(ClockedDuplexOpenError::Preparation)?;
    if matches!(input_config.buffer_size, cpal::BufferSize::Fixed(0))
        || matches!(output_config.buffer_size, cpal::BufferSize::Fixed(0))
    {
        return Err(ClockedDuplexOpenError::Preparation(
            ClockedDuplexError::InvalidConfiguration,
        ));
    }
    let input_rate = input_config.sample_rate;
    let (input_stream, consumer) =
        open_input(input, input_config, ring_frames).map_err(ClockedDuplexOpenError::Input)?;
    let renderer = PreparedClockedDuplex::new(
        processor,
        consumer,
        input_rate,
        output_config.sample_rate,
        usize::from(output_config.channels),
        options,
    )
    .map_err(ClockedDuplexOpenError::Preparation)?;
    let counters = Arc::clone(renderer.counters());
    let errors = Arc::clone(&counters);
    let clock_counters = Arc::clone(&renderer.clock_counters);
    let mut callback = output_callback(renderer);
    let stream = verified_stream(output, Direction::Output, output_config.sample_rate, || {
        output.build_output_stream(
            output_config,
            move |data: &mut [f32], _| callback(data),
            move |_| {
                errors.backend_errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
    })
    .map_err(ClockedDuplexOpenError::from)?;
    let input_stream = verified_stream(input, Direction::Input, input_rate, || Ok(input_stream))
        .map_err(|error| ClockedDuplexOpenError::Input(OpenError::from(error)))?;
    Ok(ClockedDuplexStream {
        streams: DuplexStream::from_streams(input_stream, OutputStream { stream, counters }),
        clock_counters,
    })
}
