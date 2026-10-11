//! Explicit queue-rate file transport bridge; no unique consumer duplication.
use ravet_audio_file::{BufferedPlayback, ReadAheadStatus};
use ravet_foundation::{
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
/// Relaxed control snapshots can span adjacent callbacks; counters wrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferedPlaybackOutputStatistics {
    pub consumed_output_frames: u64,
    pub underflow_output_frames: u64,
    pub output_source_position: u64,
    /// Verified EOF plus drained queue; failed/stopped source is not EOF.
    pub ended: bool,
    pub worker_status: ReadAheadStatus,
}
#[derive(Clone)]
pub struct BufferedPlaybackOutputMonitor(Arc<[AtomicU64; 5]>);
fn status_code(status: ReadAheadStatus) -> u64 {
    match status {
        ReadAheadStatus::Running => 0,
        ReadAheadStatus::EndOfFile => 1,
        ReadAheadStatus::Failed => 2,
        ReadAheadStatus::Stopped => 3,
    }
}
impl Default for BufferedPlaybackOutputStatistics {
    fn default() -> Self {
        Self {
            consumed_output_frames: 0,
            underflow_output_frames: 0,
            output_source_position: 0,
            ended: false,
            worker_status: ReadAheadStatus::Stopped,
        }
    }
}
impl BufferedPlaybackOutputMonitor {
    pub fn statistics(&self) -> BufferedPlaybackOutputStatistics {
        BufferedPlaybackOutputStatistics {
            consumed_output_frames: self.0[0].load(Ordering::Relaxed),
            underflow_output_frames: self.0[1].load(Ordering::Relaxed),
            output_source_position: self.0[2].load(Ordering::Relaxed),
            ended: self.0[3].load(Ordering::Relaxed) != 0,
            worker_status: match self.0[4].load(Ordering::Relaxed) {
                0 => ReadAheadStatus::Running,
                1 => ReadAheadStatus::EndOfFile,
                2 => ReadAheadStatus::Failed,
                _ => ReadAheadStatus::Stopped,
            },
        }
    }
}
/// Own the automatic worker's unique prepared callback transport through the
/// actual output Processor path. Prepare validates target rate/channel/render
/// bounds and allocates scratch; process renders/publishes atomics/copies only.
/// Configure transport exclusively before transferring into PreparedOutput or
/// open_output. No duplicate consumer, clock mapping or callback mailbox exists.
/// Reset pauses retaining cursor/queue; reader seek needs reclaimed stopped
/// playback plus its matching control worker. into_playback reclaims only this
/// directly owned stopped processor; it does not reclaim an inaccessible native
/// open_output closure. Native live seek/recovery remains separately integrated.
pub struct BufferedPlaybackOutput {
    playback: BufferedPlayback,
    config: Option<ProcessConfig>,
    scratch: Vec<f32>,
    monitor: BufferedPlaybackOutputMonitor,
}
impl BufferedPlaybackOutput {
    pub fn new(playback: BufferedPlayback) -> Self {
        let position = playback.position_frames();
        let status = status_code(playback.worker_status());
        Self {
            playback,
            config: None,
            scratch: Vec::new(),
            monitor: BufferedPlaybackOutputMonitor(Arc::new([
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(position),
                AtomicU64::new(0),
                AtomicU64::new(status),
            ])),
        }
    }
    pub fn playback(&self) -> &BufferedPlayback {
        &self.playback
    }
    pub fn playback_mut(&mut self) -> &mut BufferedPlayback {
        &mut self.playback
    }
    pub fn monitor(&self) -> BufferedPlaybackOutputMonitor {
        self.monitor.clone()
    }
    pub fn statistics(&self) -> BufferedPlaybackOutputStatistics {
        self.monitor.statistics()
    }
    pub fn into_playback(self) -> BufferedPlayback {
        self.playback
    }
}
impl Processor for BufferedPlaybackOutput {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.sample_rate() != f64::from(self.playback.sample_rate())
            || config.layout().input_channels() != 0
            || config.layout().output_channels() != self.playback.channels()
            || config.maximum_events() != 0
            || config.maximum_output_events() != 0
            || !config.auxiliary_buses().is_empty()
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        if config.maximum_frames() > self.playback.maximum_render_frames() {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        let n = config.maximum_frames() * self.playback.channels();
        let mut scratch = Vec::new();
        scratch
            .try_reserve_exact(n)
            .map_err(|_| PrepareError::InvalidMaximumFrames)?;
        scratch.resize(n, 0.);
        self.scratch = scratch;
        self.config = Some(config);
        Ok(())
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
        let channels = self.playback.channels();
        let samples = &mut self.scratch[..block.audio.frames() * channels];
        let Ok(p) = self.playback.render(samples) else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::BackendFailure);
        };
        self.monitor.0[0].fetch_add(p.source_frames as u64, Ordering::Relaxed);
        self.monitor.0[1].fetch_add(p.underflow_frames as u64, Ordering::Relaxed);
        self.monitor.0[2].store(self.playback.position_frames(), Ordering::Relaxed);
        self.monitor.0[3].store(u64::from(p.ended), Ordering::Relaxed);
        self.monitor.0[4].store(status_code(p.worker_status), Ordering::Relaxed);
        for channel in 0..channels {
            for (frame, sample) in block
                .audio
                .output_mut(channel)
                .expect("validated output")
                .iter_mut()
                .enumerate()
            {
                *sample = samples[frame * channels + channel];
            }
        }
        if p.source_frames == 0 {
            ProcessStatus::Silence
        } else {
            ProcessStatus::Continue
        }
    }
    fn reset(&mut self) {
        self.playback.pause();
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
}
