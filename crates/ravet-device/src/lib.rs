//! Explicit CPAL input, output and FIFO duplex adapters, outside the portable umbrella.
//!
//! Control threads enumerate/select devices and prepare processors. The callback
//! uses preallocated planar f32 scratch, divides variable device buffers into
//! bounded blocks and publishes only atomic counters. This prototype accepts
//! f32 main-bus processors, with no auxiliary buses or event delivery. Duplex
//! connects independently scheduled streams through a prepared bounded ring;
//! matching-rate mode has no drift correction; an explicit clocked mode combines
//! bounded FIFO feedback and prepared sinc conversion without sample alignment. Device
//! availability, callback deadlines and audible output require target validation.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ravet_foundation::{
    AudioBuffer, AudioLayout, MAXIMUM_CHANNELS, PrepareError, ProcessBlock, ProcessConfig,
    ProcessStatus, Processor, Transport,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub use cpal;
mod graph_events;
mod graph_output;
pub use graph_events::{
    GraphEventBatch, GraphEventConsumer, GraphEventProducer, GraphEventQueueConfig,
    GraphEventQueueError, graph_event_queue,
};
mod input;
pub use graph_output::{
    GraphOutputError, GraphOutputProgress, GraphOutputSession, PreparedDynamicGraphOutput,
    PreparedGraphOutput, RingOutput, RingOutputMonitor, RingOutputStatistics,
};
pub use input::{
    DuplexStream, InputDeviceInfo, InputStatistics, InputStream, PreparedCapture, PreparedDuplex,
    input_device, input_devices, open_duplex, open_input,
};

/// Upper bound on scratch frames per channel, allocated before stream creation.
pub const MAXIMUM_FRAMES: usize = 65_536;

/// A control-thread snapshot; IDs belong to the current CPAL host/backend.
#[derive(Clone, Debug)]
pub struct OutputDeviceInfo {
    pub id: String,
    pub name: String,
    pub configurations: Vec<cpal::SupportedStreamConfigRange>,
}

/// Enumerate the default host's output devices and supported configurations.
/// Refresh explicitly after device changes; this does not subscribe to hotplug.
pub fn output_devices() -> Result<Vec<OutputDeviceInfo>, cpal::Error> {
    cpal::default_host()
        .output_devices()?
        .map(|device| {
            Ok(OutputDeviceInfo {
                id: device.id()?.to_string(),
                name: device.description()?.name().to_owned(),
                configurations: device.supported_output_configs()?.collect(),
            })
        })
        .collect()
}

/// Select an enumerated host-local ID, or the current default output device.
/// Returns None when the requested device is unavailable; no silent fallback.
pub fn output_device(id: Option<&str>) -> Result<Option<cpal::Device>, cpal::Error> {
    let host = cpal::default_host();
    if let Some(id) = id {
        for device in host.output_devices()? {
            if device.id()?.to_string() == id {
                return Ok(Some(device));
            }
        }
        Ok(None)
    } else {
        Ok(host.default_output_device())
    }
}

/// Relaxed atomic diagnostics, sampled on a control thread. Fields may come
/// from adjacent callbacks; this is not a coherent meter or latency measurement.
/// Frames count completed DSP sub-blocks; callbacks include permanently disabled
/// callbacks after a panic. Counters wrap on overflow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutputStatistics {
    pub callbacks: u64,
    pub frames: u64,
    pub processor_faults: u64,
    pub backend_errors: u64,
    pub panics: u64,
    /// Frames zero-filled because a duplex input ring was short.
    pub input_underflow_frames: u64,
}

#[derive(Default)]
struct Counters {
    callbacks: AtomicU64,
    frames: AtomicU64,
    processor_faults: AtomicU64,
    backend_errors: AtomicU64,
    panics: AtomicU64,
    input_underflow_frames: AtomicU64,
}
impl Counters {
    fn snapshot(&self) -> OutputStatistics {
        OutputStatistics {
            callbacks: self.callbacks.load(Ordering::Relaxed),
            frames: self.frames.load(Ordering::Relaxed),
            processor_faults: self.processor_faults.load(Ordering::Relaxed),
            backend_errors: self.backend_errors.load(Ordering::Relaxed),
            panics: self.panics.load(Ordering::Relaxed),
            input_underflow_frames: self.input_underflow_frames.load(Ordering::Relaxed),
        }
    }
}

/// Prepared output-only f32 rendering, independently testable without hardware.
/// Construction may allocate; render uses bounded scratch without allocation.
/// Processor implementations must obey the portable callback contract.
pub struct PreparedOutput<P: Processor> {
    processor: P,
    config: ProcessConfig,
    scratch: Vec<Vec<f32>>,
    position: Option<i64>,
    counters: Arc<Counters>,
}
impl<P: Processor> PreparedOutput<P> {
    /// Prepare 1..=64 output channels and 1..=65,536 scratch frames per channel.
    /// Input/events/auxiliary capacities are zero. A failed prepare must leave
    /// the supplied processor inactive, as required by Processor.
    pub fn new(
        mut processor: P,
        sample_rate: u32,
        channels: usize,
        maximum_frames: usize,
    ) -> Result<Self, PrepareError> {
        if maximum_frames > MAXIMUM_FRAMES {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        let config = ProcessConfig::new(
            f64::from(sample_rate),
            maximum_frames,
            0,
            AudioLayout::new(0, channels)?,
        )?;
        processor.prepare(config)?;
        Ok(Self {
            processor,
            config,
            scratch: vec![vec![0.0; maximum_frames]; channels],
            position: Some(0),
            counters: Arc::default(),
        })
    }

    /// Fill an interleaved device buffer, partitioning oversized callbacks.
    /// A malformed partial frame silences the entire buffer without invoking
    /// DSP or advancing its position. Processor faults silence that sub-block.
    /// Silence status is enforced even if a processor left nonzero samples.
    /// Panics are contained by the live adapter, not this offline method.
    pub fn render(&mut self, output: &mut [f32]) {
        self.counters.callbacks.fetch_add(1, Ordering::Relaxed);
        let channels = self.scratch.len();
        if !output.len().is_multiple_of(channels) {
            output.fill(0.0);
            self.counters
                .processor_faults
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        for chunk in output.chunks_mut(self.config.maximum_frames() * channels) {
            let frames = chunk.len() / channels;
            let mut iter = self.scratch.iter_mut();
            let mut views: [&mut [f32]; MAXIMUM_CHANNELS] = std::array::from_fn(|_| {
                iter.next().map_or(&mut [] as &mut [f32], |channel| {
                    channel[..frames].fill(0.0);
                    &mut channel[..frames]
                })
            });
            let audio = AudioBuffer::separate(&[], &mut views[..channels], frames)
                .expect("prepared channel lengths");
            let mut block = ProcessBlock::new(
                audio,
                &[],
                Transport {
                    sample_position: self.position,
                    ..Transport::default()
                },
            );
            match self.processor.process(&mut block) {
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
    }
    pub fn statistics(&self) -> OutputStatistics {
        self.counters.snapshot()
    }
}
impl<P: Processor> Drop for PreparedOutput<P> {
    fn drop(&mut self) {
        self.processor.deactivate();
    }
}

/// A live stream. Start/pause/drop on the control thread; drop stops callbacks.
/// Backend errors are counted and require caller-directed recovery/reopening.
/// Pause/resume retains DSP history and logical sample position.
pub struct OutputStream {
    stream: cpal::Stream,
    counters: Arc<Counters>,
}
impl OutputStream {
    pub fn play(&self) -> Result<(), cpal::Error> {
        self.stream.play()
    }
    pub fn pause(&self) -> Result<(), cpal::Error> {
        self.stream.pause()
    }
    pub fn statistics(&self) -> OutputStatistics {
        self.counters.snapshot()
    }
}

/// Output stream construction error, reported outside the callback.
#[derive(Debug)]
pub enum OpenError {
    Preparation(PrepareError),
    Backend(cpal::Error),
}
impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preparation(error) => error.fmt(f),
            Self::Backend(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Preparation(error) => error,
            Self::Backend(error) => error,
        })
    }
}

/// Open an explicitly configured f32 output stream; unsupported device settings
/// are returned by CPAL. No implicit sample format conversion or rate fallback.
/// Fixed buffer size is a backend request, not a guaranteed callback size.
/// Processors are moved into the callback after preparation. A panic silences
/// output and permanently disables DSP for this stream; reopen to recover.
/// Panic unwinding/handlers are exceptional and have no real-time guarantee.
pub fn open_output<P: Processor + Send + 'static>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    maximum_frames: usize,
    processor: P,
) -> Result<OutputStream, OpenError> {
    if matches!(config.buffer_size, cpal::BufferSize::Fixed(0)) {
        return Err(OpenError::Preparation(PrepareError::InvalidMaximumFrames));
    }
    let renderer = PreparedOutput::new(
        processor,
        config.sample_rate,
        usize::from(config.channels),
        maximum_frames,
    )
    .map_err(OpenError::Preparation)?;
    let counters = Arc::clone(&renderer.counters);
    let errors = Arc::clone(&counters);
    let mut callback = output_callback(renderer);
    let stream = device
        .build_output_stream(
            config,
            move |output: &mut [f32], _| callback(output),
            move |_| {
                errors.backend_errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(OpenError::Backend)?;
    Ok(OutputStream { stream, counters })
}

trait DeviceRenderer {
    fn render_device(&mut self, output: &mut [f32]);
    fn counters(&self) -> &Arc<Counters>;
}
impl<P: Processor> DeviceRenderer for PreparedOutput<P> {
    fn render_device(&mut self, output: &mut [f32]) {
        self.render(output);
    }
    fn counters(&self) -> &Arc<Counters> {
        &self.counters
    }
}
fn output_callback<R: DeviceRenderer>(mut renderer: R) -> impl FnMut(&mut [f32]) {
    let counters = Arc::clone(renderer.counters());
    let mut disabled = false;
    move |output| {
        if disabled {
            output.fill(0.0);
            counters.callbacks.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            renderer.render_device(output)
        }))
        .is_err()
        {
            output.fill(0.0);
            disabled = true;
            counters.panics.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod callback_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    struct PanicProcessor(Arc<AtomicUsize>);
    impl Processor for PanicProcessor {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
            Ok(())
        }
        fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
            self.0.fetch_add(1, Ordering::Relaxed);
            panic!("deliberate callback containment probe");
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {}
    }
    #[test]
    fn unwinding_panic_silences_and_permanently_disables_callback_dsp() {
        let calls = Arc::new(AtomicUsize::new(0));
        let renderer =
            PreparedOutput::new(PanicProcessor(Arc::clone(&calls)), 48000, 2, 64).unwrap();
        let counters = Arc::clone(&renderer.counters);
        let mut callback = output_callback(renderer);
        let mut output = [1.; 128];
        callback(&mut output);
        assert_eq!(output, [0.; 128]);
        output.fill(2.);
        callback(&mut output);
        assert_eq!(output, [0.; 128]);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(counters.snapshot().panics, 1);
        assert_eq!(counters.snapshot().callbacks, 2);
    }
}

mod recovery;
pub use recovery::{
    ClockedDuplexRecoveryError, DuplexRecoveryError, DuplexRecoveryStatus, OutputRecoveryError,
    OutputRecoveryStatus, RecoveringClockedDuplex, RecoveringDuplex, RecoveringOutput,
};

mod clocked_input;
pub use clocked_input::{ClockedInputProgress, PreparedClockedInput};
mod clocked;
pub use clocked::{
    ClockedDuplexError, ClockedDuplexOpenError, ClockedDuplexOptions, ClockedDuplexStatistics,
    ClockedDuplexStream, PreparedClockedDuplex, open_clocked_duplex,
};
