//! Prepared file playback through the existing standalone output processor path.
use ravet_audio_file::{FilePlaybackInfo, MemoryPlayback, PreparedFilePlayback};
use ravet_foundation::{
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor,
};

/// Output-only processor owning a complete, already converted file clip.
/// Configuration must match the prepared target rate/channel order and fit the
/// clip's render bound. Prepare allocates scratch; process only copies samples.
/// File reads, conversion, device opening and clip destruction belong off audio.
///
/// Configure play/pause/seek/loop through `playback_mut` on the exclusive owner
/// before transferring into `PreparedOutput` or `open_output`. This adapter does
/// not add a cross-thread command queue. Cursors and loops are prepared output
/// frames, including the explicit filter delay/tail. Device clocks and physical
/// output remain separately validated. Reset stops at zero/configured loop start.
pub struct FilePlaybackOutput {
    prepared: PreparedFilePlayback,
    config: Option<ProcessConfig>,
    scratch: Vec<f32>,
}
impl FilePlaybackOutput {
    pub fn new(prepared: PreparedFilePlayback) -> Self {
        Self {
            prepared,
            config: None,
            scratch: Vec::new(),
        }
    }
    pub fn info(&self) -> FilePlaybackInfo {
        self.prepared.info()
    }
    pub fn playback(&self) -> &MemoryPlayback {
        self.prepared.playback()
    }
    pub fn playback_mut(&mut self) -> &mut MemoryPlayback {
        self.prepared.playback_mut()
    }
    /// Recover the clip after stopping callbacks; no conversion or seek occurs.
    pub fn into_prepared(self) -> PreparedFilePlayback {
        self.prepared
    }
}
impl Processor for FilePlaybackOutput {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        let playback = self.prepared.playback();
        if config.sample_rate() != f64::from(playback.sample_rate())
            || config.layout().input_channels() != 0
            || config.layout().output_channels() != playback.channels()
            || config.maximum_events() != 0
            || config.maximum_output_events() != 0
            || !config.auxiliary_buses().is_empty()
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        if config.maximum_frames() > playback.maximum_render_frames() {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        let count = config.maximum_frames() * playback.channels();
        let mut scratch = Vec::new();
        scratch
            .try_reserve_exact(count)
            .map_err(|_| PrepareError::InvalidMaximumFrames)?;
        scratch.resize(count, 0.);
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
        let channels = self.prepared.playback().channels();
        let samples = &mut self.scratch[..block.audio.frames() * channels];
        let Ok(progress) = self.prepared.playback_mut().render(samples) else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::BackendFailure);
        };
        for channel in 0..channels {
            for (frame, sample) in block
                .audio
                .output_mut(channel)
                .expect("validated channel")
                .iter_mut()
                .enumerate()
            {
                *sample = samples[frame * channels + channel];
            }
        }
        if progress.source_frames == 0 {
            ProcessStatus::Silence
        } else {
            ProcessStatus::Continue
        }
    }
    fn reset(&mut self) {
        self.prepared.playback_mut().stop();
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
}
