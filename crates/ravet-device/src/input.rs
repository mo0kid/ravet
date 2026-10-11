use super::{
    Counters, DeviceRenderer, MAXIMUM_FRAMES, OpenError, OutputStatistics, OutputStream,
    output_callback,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ravet_audio_file::{AudioConsumer, AudioProducer, audio_ring};
use ravet_foundation::{
    AudioBuffer, AudioLayout, MAXIMUM_CHANNELS, PrepareError, ProcessBlock, ProcessConfig,
    ProcessStatus, Processor, Transport,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
/// Control-thread input enumeration; IDs belong to the selected default host.
#[derive(Clone, Debug)]
pub struct InputDeviceInfo {
    pub id: String,
    pub name: String,
    pub configurations: Vec<cpal::SupportedStreamConfigRange>,
}
pub fn input_devices() -> Result<Vec<InputDeviceInfo>, cpal::Error> {
    cpal::default_host()
        .input_devices()?
        .map(|device| {
            Ok(InputDeviceInfo {
                id: device.id()?.to_string(),
                name: device.description()?.name().to_owned(),
                configurations: device.supported_input_configs()?.collect(),
            })
        })
        .collect()
}
/// Select an exact input ID or default device, with no implicit fallback.
pub fn input_device(id: Option<&str>) -> Result<Option<cpal::Device>, cpal::Error> {
    let host = cpal::default_host();
    if let Some(id) = id {
        for device in host.input_devices()? {
            if device.id()?.to_string() == id {
                return Ok(Some(device));
            }
        }
        Ok(None)
    } else {
        Ok(host.default_input_device())
    }
}
/// Relaxed counters, not a coherent sample/latency measurement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputStatistics {
    pub callbacks: u64,
    pub captured_frames: u64,
    pub dropped_frames: u64,
    pub invalid_buffers: u64,
    pub backend_errors: u64,
}
#[derive(Default)]
pub(super) struct CaptureCounters {
    callbacks: AtomicU64,
    captured: AtomicU64,
    dropped: AtomicU64,
    invalid: AtomicU64,
    errors: AtomicU64,
}
impl CaptureCounters {
    pub(super) fn report_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn snapshot(&self) -> InputStatistics {
        InputStatistics {
            callbacks: self.callbacks.load(Ordering::Relaxed),
            captured_frames: self.captured.load(Ordering::Relaxed),
            dropped_frames: self.dropped.load(Ordering::Relaxed),
            invalid_buffers: self.invalid.load(Ordering::Relaxed),
            backend_errors: self.errors.load(Ordering::Relaxed),
        }
    }
}
/// Prepared interleaved capture into a whole-frame SPSC ring. Full rings discard
/// the newest input suffix and count it. No callback allocation, locks or I/O.
pub struct PreparedCapture {
    producer: AudioProducer,
    pub(super) counters: Arc<CaptureCounters>,
}
impl PreparedCapture {
    /// Input channels 1..64, power-of-two ring capacity 1..65536 frames.
    pub fn new(channels: usize, ring_frames: usize) -> Result<(Self, AudioConsumer), PrepareError> {
        if !(1..=MAXIMUM_CHANNELS).contains(&channels) {
            return Err(PrepareError::UnsupportedLayout);
        }
        let (producer, consumer) =
            audio_ring(channels, ring_frames).map_err(|_| PrepareError::InvalidMaximumFrames)?;
        Ok((
            Self {
                producer,
                counters: Arc::default(),
            },
            consumer,
        ))
    }
    /// Reject a partial frame before publishing any input; keep existing ring data.
    pub fn capture(&mut self, input: &[f32]) {
        self.counters.callbacks.fetch_add(1, Ordering::Relaxed);
        let channels = self.producer.channels();
        if !input.len().is_multiple_of(channels) {
            self.counters.invalid.fetch_add(1, Ordering::Relaxed);
            self.counters
                .dropped
                .fetch_add((input.len() / channels) as u64, Ordering::Relaxed);
            return;
        }
        let count = self.producer.push(input).expect("complete frames");
        self.counters
            .captured
            .fetch_add(count as u64, Ordering::Relaxed);
        self.counters
            .dropped
            .fetch_add((input.len() / channels - count) as u64, Ordering::Relaxed);
    }
    pub fn statistics(&self) -> InputStatistics {
        self.counters.snapshot()
    }
}
/// Live f32 input stream. Control-thread play/pause/drop owns backend lifecycle.
/// Pause does not discard queued captured frames; seek/flush policy is external.
pub struct InputStream {
    stream: cpal::Stream,
    pub(super) counters: Arc<CaptureCounters>,
}
impl InputStream {
    pub fn play(&self) -> Result<(), cpal::Error> {
        self.stream.play()
    }
    pub fn pause(&self) -> Result<(), cpal::Error> {
        self.stream.pause()
    }
    pub fn statistics(&self) -> InputStatistics {
        self.counters.snapshot()
    }
}
/// Open explicitly configured f32 capture; backend format/rate errors propagate.
/// No sample conversion, resampling or automatic device recovery is performed.
pub fn open_input(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    ring_frames: usize,
) -> Result<(InputStream, AudioConsumer), OpenError> {
    if config.sample_rate == 0 {
        return Err(OpenError::Preparation(PrepareError::InvalidSampleRate));
    }
    if matches!(config.buffer_size, cpal::BufferSize::Fixed(0)) {
        return Err(OpenError::Preparation(PrepareError::InvalidMaximumFrames));
    }
    let (mut capture, consumer) = PreparedCapture::new(usize::from(config.channels), ring_frames)
        .map_err(OpenError::Preparation)?;
    let counters = Arc::clone(&capture.counters);
    let errors = Arc::clone(&counters);
    let stream = device
        .build_input_stream(
            config,
            move |input: &[f32], _| capture.capture(input),
            move |_| {
                errors.errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(OpenError::Backend)?;
    Ok((InputStream { stream, counters }, consumer))
}
/// Original preparation error and exclusive unchanged endpoint ownership.
pub struct PreparedDuplexFailure<P> {
    processor: P,
    consumer: AudioConsumer,
    error: PrepareError,
}
impl<P> PreparedDuplexFailure<P> {
    pub fn error(&self) -> &PrepareError {
        &self.error
    }
    pub fn into_parts(self) -> (P, AudioConsumer, PrepareError) {
        (self.processor, self.consumer, self.error)
    }
}
impl<P> std::fmt::Display for PreparedDuplexFailure<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl<P> std::error::Error for PreparedDuplexFailure<P> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
impl<P> std::fmt::Debug for PreparedDuplexFailure<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedDuplexFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
pub(crate) fn scratch<T: Clone>(n: usize, zero: T) -> Result<Vec<T>, PrepareError> {
    let mut v = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| PrepareError::InvalidMaximumFrames)?;
    v.resize(n, zero);
    Ok(v)
}
pub(crate) fn planar<T: Clone>(
    channels: usize,
    frames: usize,
    zero: T,
) -> Result<Vec<Vec<T>>, PrepareError> {
    let mut v = Vec::new();
    v.try_reserve_exact(channels)
        .map_err(|_| PrepareError::InvalidMaximumFrames)?;
    for _ in 0..channels {
        v.push(scratch(frames, zero.clone())?);
    }
    Ok(v)
}
/// Prepared f32 effect processing over queued interleaved input. Input underflow
/// fills the missing suffix with zero and counts frames. Variable output buffers
/// split at the preparation maximum; no rate adaptation or event delivery.
pub struct PreparedDuplex<P: Processor> {
    processor: Option<P>,
    active: bool,
    config: ProcessConfig,
    consumer: Option<AudioConsumer>,
    captured: Vec<f32>,
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,
    position: Option<i64>,
    counters: Arc<Counters>,
}
impl<P: Processor> PreparedDuplex<P> {
    pub fn new(
        processor: P,
        sample_rate: u32,
        consumer: AudioConsumer,
        output_channels: usize,
        maximum_frames: usize,
    ) -> Result<Self, PrepareError> {
        Self::new_owned(
            processor,
            sample_rate,
            consumer,
            output_channels,
            maximum_frames,
        )
        .map_err(|failure| failure.error)
    }
    /// Prepare on control, retaining the original processor and capture consumer
    /// on normal failure. All fallible scratch reserves precede child preparation.
    /// A failed child prepare must leave it inactive; foreign panics are exceptional.
    pub fn new_owned(
        mut processor: P,
        sample_rate: u32,
        consumer: AudioConsumer,
        output_channels: usize,
        maximum_frames: usize,
    ) -> Result<Self, PreparedDuplexFailure<P>> {
        let prepared = (|| {
            if !(1..=MAXIMUM_FRAMES).contains(&maximum_frames) {
                return Err(PrepareError::InvalidMaximumFrames);
            }
            if !(1..=MAXIMUM_CHANNELS).contains(&output_channels) {
                return Err(PrepareError::UnsupportedLayout);
            }
            let input_channels = consumer.channels();
            let config = ProcessConfig::new(
                f64::from(sample_rate),
                maximum_frames,
                0,
                AudioLayout::new(input_channels, output_channels)?,
            )?;
            let captured = scratch(maximum_frames * input_channels, 0.0f32)?;
            let inputs = planar(input_channels, maximum_frames, 0.0f32)?;
            let outputs = planar(output_channels, maximum_frames, 0.0f32)?;
            let counters = Arc::default();
            processor.prepare(config)?;
            Ok((config, captured, inputs, outputs, counters))
        })();
        let (config, captured, inputs, outputs, counters) = match prepared {
            Ok(parts) => parts,
            Err(error) => {
                return Err(PreparedDuplexFailure {
                    processor,
                    consumer,
                    error,
                });
            }
        };
        Ok(Self {
            processor: Some(processor),
            active: true,
            config,
            consumer: Some(consumer),
            captured,
            inputs,
            outputs,
            position: Some(0),
            counters,
        })
    }
    /// Consume stopped ownership on control, deactivating exactly once. Scratch
    /// disposal is on this caller; no capture drain, reset, seek or wait is hidden.
    pub fn into_parts(mut self) -> (P, AudioConsumer) {
        self.deactivate_owned();
        (
            self.processor.take().expect("owned processor"),
            self.consumer.take().expect("owned consumer"),
        )
    }
    pub fn into_processor(self) -> P {
        self.into_parts().0
    }
    fn deactivate_owned(&mut self) {
        if self.active {
            self.active = false;
            self.processor
                .as_mut()
                .expect("owned processor")
                .deactivate();
        }
    }
    /// Offline callback-equivalent rendering. Panics are contained by the live
    /// wrapper, not this method; malformed partial frames silence without dequeue.
    pub fn render(&mut self, output: &mut [f32]) {
        self.counters.callbacks.fetch_add(1, Ordering::Relaxed);
        let channels = self.outputs.len();
        if !output.len().is_multiple_of(channels) {
            output.fill(0.);
            self.counters
                .processor_faults
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        for chunk in output.chunks_mut(self.config.maximum_frames() * channels) {
            self.render_block(chunk);
        }
    }
    pub(super) fn render_block(&mut self, chunk: &mut [f32]) {
        let channels = self.outputs.len();
        let inputs = self.inputs.len();
        let frames = chunk.len() / channels;
        let captured = &mut self.captured[..frames * inputs];
        captured.fill(0.);
        let read = self
            .consumer
            .as_mut()
            .expect("owned consumer")
            .pop(captured)
            .expect("complete frames");
        self.counters
            .input_underflow_frames
            .fetch_add((frames - read) as u64, Ordering::Relaxed);
        for (frame, samples) in captured.chunks_exact(inputs).enumerate() {
            for (channel, sample) in samples.iter().enumerate() {
                self.inputs[channel][frame] = *sample;
            }
        }
        let input_views: [&[f32]; MAXIMUM_CHANNELS] = std::array::from_fn(|c| {
            self.inputs
                .get(c)
                .map_or(&[] as &[f32], |data| &data[..frames])
        });
        let mut iter = self.outputs.iter_mut();
        let mut output_views: [&mut [f32]; MAXIMUM_CHANNELS] = std::array::from_fn(|_| {
            iter.next().map_or(&mut [] as &mut [f32], |data| {
                data[..frames].fill(0.);
                &mut data[..frames]
            })
        });
        let audio = AudioBuffer::separate(
            &input_views[..inputs],
            &mut output_views[..channels],
            frames,
        )
        .expect("prepared lengths");
        let mut block = ProcessBlock::new(
            audio,
            &[],
            Transport {
                sample_position: self.position,
                ..Transport::default()
            },
        );
        match self
            .processor
            .as_mut()
            .expect("owned processor")
            .process(&mut block)
        {
            ProcessStatus::Continue => {}
            ProcessStatus::Silence => block.clear_outputs(),
            ProcessStatus::Fault(_) => {
                block.clear_outputs();
                self.counters
                    .processor_faults
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        for (frame, samples) in chunk.chunks_exact_mut(channels).enumerate() {
            for (channel, sample) in samples.iter_mut().enumerate() {
                *sample = block.audio.output(channel).expect("prepared channels")[frame];
            }
        }
        self.position = self
            .position
            .and_then(|position| position.checked_add(frames as i64));
        self.counters
            .frames
            .fetch_add(frames as u64, Ordering::Relaxed);
    }
    pub fn statistics(&self) -> OutputStatistics {
        self.counters.snapshot()
    }
}
impl<P: Processor> DeviceRenderer for PreparedDuplex<P> {
    fn render_device(&mut self, output: &mut [f32]) {
        self.render(output);
    }
    fn counters(&self) -> &Arc<Counters> {
        &self.counters
    }
}
impl<P: Processor> Drop for PreparedDuplex<P> {
    fn drop(&mut self) {
        self.deactivate_owned();
    }
}
/// Two explicit streams with a bounded input FIFO. Matching nominal rates are
/// required but separate device clocks can drift; no compensation is supplied.
/// Start/pause are sequential and not sample-synchronized. Pause retains queued
/// input/DSP history. Reopen to clear queue or recover backend failure/panic.
pub struct DuplexStream {
    input: InputStream,
    output: OutputStream,
}
impl DuplexStream {
    pub(super) fn from_streams(input: InputStream, output: OutputStream) -> Self {
        Self { input, output }
    }
    pub fn play(&self) -> Result<(), cpal::Error> {
        self.input.play()?;
        if let Err(error) = self.output.play() {
            let _ = self.input.pause();
            return Err(error);
        }
        Ok(())
    }
    pub fn pause(&self) -> Result<(), cpal::Error> {
        let output = self.output.pause();
        let input = self.input.pause();
        output.and(input)
    }
    pub fn input_statistics(&self) -> InputStatistics {
        self.input.statistics()
    }
    pub fn output_statistics(&self) -> OutputStatistics {
        self.output.statistics()
    }
}
impl Drop for DuplexStream {
    fn drop(&mut self) {
        let _ = self.pause();
    }
}
/// Prepare/open matching-rate f32 effect capture/playback with explicit devices.
/// No hidden default fallback, loopback routing, resampling or automatic recovery.
pub fn open_duplex<P: Processor + Send + 'static>(
    input: &cpal::Device,
    output: &cpal::Device,
    input_config: cpal::StreamConfig,
    output_config: cpal::StreamConfig,
    maximum_frames: usize,
    ring_frames: usize,
    processor: P,
) -> Result<DuplexStream, OpenError> {
    if input_config.sample_rate != output_config.sample_rate {
        return Err(OpenError::Preparation(PrepareError::InvalidSampleRate));
    }
    if matches!(output_config.buffer_size, cpal::BufferSize::Fixed(0)) {
        return Err(OpenError::Preparation(PrepareError::InvalidMaximumFrames));
    }
    let (input_stream, consumer) = open_input(input, input_config, ring_frames)?;
    let renderer = PreparedDuplex::new(
        processor,
        output_config.sample_rate,
        consumer,
        usize::from(output_config.channels),
        maximum_frames,
    )
    .map_err(OpenError::Preparation)?;
    let counters = Arc::clone(&renderer.counters);
    let errors = Arc::clone(&counters);
    let mut callback = output_callback(renderer);
    let stream = output
        .build_output_stream(
            output_config,
            move |output: &mut [f32], _| callback(output),
            move |_| {
                errors.backend_errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(OpenError::Backend)?;
    Ok(DuplexStream {
        input: input_stream,
        output: OutputStream { stream, counters },
    })
}
