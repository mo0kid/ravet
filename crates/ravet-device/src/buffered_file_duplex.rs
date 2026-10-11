//! Output-master buffered file and drift-corrected capture composition.
use ravet_audio_file::{BufferedPlayback, ReadAheadStatus};
use ravet_foundation::{
    AudioBuffer, AudioLayout, MAXIMUM_CHANNELS, PrepareError, ProcessBlock, ProcessConfig,
    ProcessError, ProcessStatus, Processor, Tail, Transport,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// Typed file and logical device coordinates; neither is a hardware timestamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferedFileDuplexStatistics {
    pub file_position: u64,
    pub consumed_file_frames: u64,
    pub file_underflow_frames: u64,
    /// Verified file EOF and drained queue; not downstream/audible completion.
    pub ended: bool,
    /// Source worker state; its original error is recovered from worker shutdown.
    pub worker_status: ReadAheadStatus,
    /// Accepted outer frames, including fault-silenced/poisoned callbacks.
    pub device_render_frames: u64,
    pub last_device_sample_position: Option<i64>,
    pub child_calls: u64,
    pub child_faults: u64,
}
/// Relaxed control telemetry; fields may span adjacent callbacks and counters wrap.
/// No coherent latency, hardware timestamp or audible-delivery claim.
#[derive(Clone)]
pub struct BufferedFileDuplexMonitor(Arc<[AtomicU64; 10]>);
impl BufferedFileDuplexMonitor {
    pub fn statistics(&self) -> BufferedFileDuplexStatistics {
        let words = self.0.each_ref().map(|v| v.load(Ordering::Relaxed));
        BufferedFileDuplexStatistics {
            file_position: words[0],
            consumed_file_frames: words[1],
            file_underflow_frames: words[2],
            ended: words[3] != 0,
            worker_status: match words[4] {
                0 => ReadAheadStatus::Running,
                1 => ReadAheadStatus::EndOfFile,
                2 => ReadAheadStatus::Failed,
                _ => ReadAheadStatus::Stopped,
            },
            device_render_frames: words[5],
            last_device_sample_position: (words[7] != 0).then_some(words[6] as i64),
            child_calls: words[8],
            child_faults: words[9],
        }
    }
    fn publish(&self, s: BufferedFileDuplexStatistics) {
        let status = match s.worker_status {
            ReadAheadStatus::Running => 0,
            ReadAheadStatus::EndOfFile => 1,
            ReadAheadStatus::Failed => 2,
            ReadAheadStatus::Stopped => 3,
        };
        let words = [
            s.file_position,
            s.consumed_file_frames,
            s.file_underflow_frames,
            u64::from(s.ended),
            status,
            s.device_render_frames,
            s.last_device_sample_position.unwrap_or(0) as u64,
            u64::from(s.last_device_sample_position.is_some()),
            s.child_calls,
            s.child_faults,
        ];
        for (atomic, word) in self.0.iter().zip(words) {
            atomic.store(word, Ordering::Relaxed);
        }
    }
}
/// One unique prepared file consumer plus downstream f32 processing. Outer inputs
/// are captured PCM already converted to the output clock by PreparedClockedDuplex;
/// child inputs are file channels first, captured channels second. No file FIFO
/// drift estimator exists. All rates equal the file queue rate. Prepared callbacks
/// allocate nothing and do no lock/I/O/wait; source/capture clocks remain distinct.
///
/// At most two child calls separate consumed file PCM from stalled zero-file PCM.
/// Capture/DSP continue while file playback pauses. Child sample_position describes
/// file OUTPUT-frame coordinates (None above i64::MAX); it is not an original-file
/// coordinate or hardware timestamp. No beat/tempo metadata is inferred. A fault
/// silences the whole outer block and retains first error until reset/reprepare;
/// source and earlier child histories already consumed cannot be rolled back.
/// The outer status is BackendFailure after consumption; first_error retains the
/// original child error. Source Failed alone drains queued prefix and leaves
/// capture/DSP running, with ended=false and its original error on worker shutdown.
/// Child latency/tail declarations exclude worker/converter/FIFO buffering.
///
/// This owner can be extracted only when directly owned and stopped. Native clocked
/// stream closures cannot be reclaimed through this API. Native callback teardown,
/// physical synchronization, recovery factories and session hooks retain their
/// existing contracts. Destruction/preparation belong off audio.
///
/// ```
/// use ravet_audio_file::{AudioConsumer, BufferedPlayback};
/// use ravet_device::{BufferedFileDuplexProcessor, ClockedDuplexOptions,
///     ClockedDuplexError, PreparedClockedDuplex};
/// use ravet_foundation::Processor;
/// // Call on control with a queue prepared at output_rate. Child input channels
/// // are the file channels followed by the raw capture channels.
/// pub fn compose<P: Processor>(file: BufferedPlayback, dsp: P, raw: AudioConsumer,
///     input_rate: u32, output_rate: u32, output_channels: usize,
///     options: ClockedDuplexOptions)
///     -> Result<PreparedClockedDuplex<BufferedFileDuplexProcessor<P>>, ClockedDuplexError>
/// {
///     PreparedClockedDuplex::new(BufferedFileDuplexProcessor::new(file, dsp), raw,
///         input_rate, output_rate, output_channels, options)
/// }
/// ```
pub struct BufferedFileDuplexProcessor<P: Processor> {
    playback: Option<BufferedPlayback>,
    processor: Option<P>,
    config: Option<ProcessConfig>,
    file: Vec<f32>,
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,
    expected_file_position: u64,
    discontinuity: bool,
    first_error: Option<ProcessError>,
    statistics: BufferedFileDuplexStatistics,
    monitor: BufferedFileDuplexMonitor,
}
fn zeros(n: usize) -> Result<Vec<f32>, PrepareError> {
    let mut v = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| PrepareError::InvalidMaximumFrames)?;
    v.resize(n, 0.);
    Ok(v)
}
fn planes(channels: usize, frames: usize) -> Result<Vec<Vec<f32>>, PrepareError> {
    let mut v = Vec::new();
    v.try_reserve_exact(channels)
        .map_err(|_| PrepareError::InvalidMaximumFrames)?;
    for _ in 0..channels {
        v.push(zeros(frames)?);
    }
    Ok(v)
}
impl<P: Processor> BufferedFileDuplexProcessor<P> {
    /// Take exclusive owners without preparation, factory calls or source I/O.
    pub fn new(playback: BufferedPlayback, processor: P) -> Self {
        let position = playback.position_frames();
        let status = playback.worker_status();
        let monitor =
            BufferedFileDuplexMonitor(Arc::new(std::array::from_fn(|_| AtomicU64::new(0))));
        let owner = Self {
            playback: Some(playback),
            processor: Some(processor),
            config: None,
            file: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            expected_file_position: position,
            discontinuity: true,
            first_error: None,
            statistics: BufferedFileDuplexStatistics {
                file_position: position,
                consumed_file_frames: 0,
                file_underflow_frames: 0,
                ended: false,
                worker_status: status,
                device_render_frames: 0,
                last_device_sample_position: None,
                child_calls: 0,
                child_faults: 0,
            },
            monitor,
        };
        owner.monitor.publish(owner.statistics);
        owner
    }
    pub fn monitor(&self) -> BufferedFileDuplexMonitor {
        self.monitor.clone()
    }
    pub fn playback(&self) -> &BufferedPlayback {
        self.playback.as_ref().expect("owned playback")
    }
    /// Exclusive stopped/control configuration, never concurrent with callbacks.
    /// This does not reclaim a playback owner hidden inside a native closure.
    pub fn playback_mut(&mut self) -> &mut BufferedPlayback {
        self.playback.as_mut().expect("owned playback")
    }
    /// Directly owned snapshots; counters wrap, no timestamp/latency claim.
    pub fn statistics(&self) -> BufferedFileDuplexStatistics {
        self.statistics
    }
    /// First original child fault (or source BackendFailure), preserved until reset
    /// or successful preparation. Outer post-consumption faults use BackendFailure
    /// because an earlier source/child history cannot be rolled back.
    pub fn first_error(&self) -> Option<ProcessError> {
        self.first_error
    }
    /// Consume a directly owned stopped wrapper; deactivate exactly once when
    /// prepared. No child reset, file read/drain/seek or worker join is hidden.
    pub fn into_parts(mut self) -> (BufferedPlayback, P) {
        self.deactivate();
        (
            self.playback.take().expect("owned playback"),
            self.processor.take().expect("owned processor"),
        )
    }
    fn segment(&mut self, start: usize, end: usize, position: u64, playing: bool) -> ProcessStatus {
        let input_views: [&[f32]; MAXIMUM_CHANNELS] =
            std::array::from_fn(|c| self.inputs.get(c).map_or(&[] as &[f32], |v| &v[start..end]));
        let mut iter = self.outputs.iter_mut();
        let mut output_views: [&mut [f32]; MAXIMUM_CHANNELS] = std::array::from_fn(|_| {
            iter.next().map_or(&mut [] as &mut [f32], |v| {
                v[start..end].fill(0.);
                &mut v[start..end]
            })
        });
        let outputs = self.config.expect("prepared").layout().output_channels();
        let audio = AudioBuffer::separate(
            &input_views[..self.inputs.len()],
            &mut output_views[..outputs],
            end - start,
        )
        .expect("prepared complete segment");
        let mut block = ProcessBlock::new(
            audio,
            &[],
            Transport {
                playing: Some(playing),
                sample_position: i64::try_from(position).ok(),
                discontinuity: self.discontinuity,
                ..Transport::default()
            },
        );
        self.statistics.child_calls = self.statistics.child_calls.wrapping_add(1);
        self.discontinuity = false;
        let status = self
            .processor
            .as_mut()
            .expect("owned processor")
            .process(&mut block);
        if status == ProcessStatus::Silence {
            block.clear_outputs();
        }
        status
    }
}
impl<P: Processor> Processor for BufferedFileDuplexProcessor<P> {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.deactivate();
        let file_channels = self.playback().channels();
        let capture_channels = config.layout().input_channels();
        if capture_channels == 0
            || config.layout().output_channels() == 0
            || capture_channels + file_channels > MAXIMUM_CHANNELS
            || config.sample_rate() != f64::from(self.playback().sample_rate())
            || config.maximum_events() != 0
            || config.maximum_output_events() != 0
            || !config.auxiliary_buses().is_empty()
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        let frames = config.maximum_frames();
        if frames > self.playback().maximum_render_frames()
            || frames > crate::MAXIMUM_FRAMES
            || frames
                .checked_mul(
                    2 * file_channels + capture_channels + config.layout().output_channels(),
                )
                .and_then(|samples| samples.checked_mul(4))
                .is_none_or(|bytes| bytes > 32 * 1024 * 1024)
        {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        let child_config = ProcessConfig::new(
            config.sample_rate(),
            frames,
            0,
            AudioLayout::new(
                file_channels + capture_channels,
                config.layout().output_channels(),
            )?,
        )?;
        let file = zeros(frames * file_channels)?;
        let inputs = planes(file_channels + capture_channels, frames)?;
        let outputs = planes(config.layout().output_channels(), frames)?;
        self.processor
            .as_mut()
            .expect("owned processor")
            .prepare(child_config)?;
        self.file = file;
        self.inputs = inputs;
        self.outputs = outputs;
        self.config = Some(config);
        self.first_error = None;
        self.expected_file_position = self.playback().position_frames();
        self.discontinuity = true;
        Ok(())
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        if self.config.is_none() {
            return Err(ProcessError::NotPrepared);
        }
        self.processor
            .as_mut()
            .expect("owned processor")
            .start_audio()
    }
    fn stop_audio(&mut self) {
        self.processor
            .as_mut()
            .expect("owned processor")
            .stop_audio();
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        let Some(config) = self.config else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::NotPrepared);
        };
        if let Err(error) = config.validate_block(block) {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let frames = block.audio.frames();
        if frames == 0 {
            return self.first_error.map_or(ProcessStatus::Continue, |_| {
                ProcessStatus::Fault(ProcessError::BackendFailure)
            });
        }
        self.statistics.device_render_frames = self
            .statistics
            .device_render_frames
            .wrapping_add(frames as u64);
        self.statistics.last_device_sample_position = block.transport.sample_position;
        if self.first_error.is_some() {
            self.monitor.publish(self.statistics);
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::BackendFailure);
        }
        let file_channels = self.playback().channels();
        let before = self.playback().position_frames();
        self.discontinuity |= before != self.expected_file_position;
        let samples = &mut self.file[..frames * file_channels];
        let progress = match self
            .playback
            .as_mut()
            .expect("owned playback")
            .render(samples)
        {
            Ok(p) => p,
            Err(_) => {
                self.first_error = Some(ProcessError::BackendFailure);
                self.monitor.publish(self.statistics);
                block.clear_outputs();
                return ProcessStatus::Fault(ProcessError::BackendFailure);
            }
        };
        self.expected_file_position = self.playback().position_frames();
        self.statistics.file_position = self.expected_file_position;
        self.statistics.consumed_file_frames = self
            .statistics
            .consumed_file_frames
            .wrapping_add(progress.source_frames as u64);
        self.statistics.file_underflow_frames = self
            .statistics
            .file_underflow_frames
            .wrapping_add(progress.underflow_frames as u64);
        self.statistics.worker_status = progress.worker_status;
        self.statistics.ended = progress.ended;
        for channel in 0..file_channels {
            for frame in 0..frames {
                self.inputs[channel][frame] = self.file[frame * file_channels + channel];
            }
        }
        for channel in 0..config.layout().input_channels() {
            self.inputs[file_channels + channel][..frames]
                .copy_from_slice(block.audio.input(channel).expect("validated input"));
        }
        let mut silence = true;
        for (start, end, position, playing) in [
            (0, progress.source_frames, before, true),
            (
                progress.source_frames,
                frames,
                self.expected_file_position,
                false,
            ),
        ] {
            if start == end {
                continue;
            }
            match self.segment(start, end, position, playing) {
                ProcessStatus::Continue => silence = false,
                ProcessStatus::Silence => {}
                ProcessStatus::Fault(error) => {
                    self.statistics.child_faults = self.statistics.child_faults.wrapping_add(1);
                    self.first_error = Some(error);
                    self.monitor.publish(self.statistics);
                    block.clear_outputs();
                    return ProcessStatus::Fault(ProcessError::BackendFailure);
                }
            }
        }
        for channel in 0..config.layout().output_channels() {
            block
                .audio
                .output_mut(channel)
                .expect("validated output")
                .copy_from_slice(&self.outputs[channel][..frames]);
        }
        self.monitor.publish(self.statistics);
        if silence {
            ProcessStatus::Silence
        } else {
            ProcessStatus::Continue
        }
    }
    fn reset(&mut self) {
        let _ = self.try_reset();
    }
    fn try_reset(&mut self) -> Result<(), ProcessError> {
        self.playback_mut().pause();
        self.discontinuity = true;
        match self
            .processor
            .as_mut()
            .expect("owned processor")
            .try_reset()
        {
            Ok(()) => {
                self.first_error = None;
                Ok(())
            }
            Err(error) => {
                self.first_error.get_or_insert(error);
                Err(error)
            }
        }
    }
    fn deactivate(&mut self) {
        if self.config.take().is_some() {
            self.processor
                .as_mut()
                .expect("owned processor")
                .deactivate();
        }
    }
    fn latency_samples(&self) -> u32 {
        self.processor
            .as_ref()
            .expect("owned processor")
            .latency_samples()
    }
    fn tail(&self) -> Tail {
        self.processor.as_ref().expect("owned processor").tail()
    }
}
impl<P: Processor> Drop for BufferedFileDuplexProcessor<P> {
    fn drop(&mut self) {
        self.deactivate();
    }
}
