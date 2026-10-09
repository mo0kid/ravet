//! Prepared memory playback; file loading and device clocks remain caller-owned.
use std::ops::Range;

/// Invalid preparation, position, loop or render shape; rejection preserves
/// transport state. Render rejection silences the complete supplied buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackError {
    InvalidConfiguration,
    InvalidSamples,
    InvalidPosition,
    InvalidLoop,
}
impl std::fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "playback error: {self:?}")
    }
}
impl std::error::Error for PlaybackError {}
/// Progress counts source frames; paused/end silence does not advance position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaybackProgress {
    pub source_frames: usize,
    /// Number of end-to-start loop transitions in this render, including one
    /// after the final copied frame. A one-frame loop may wrap once per frame.
    pub loop_wraps: usize,
    pub ended: bool,
}
/// Owned interleaved f32 clip with a sample-counted transport.
/// Preparation copies 1..=16,777,216 finite samples (at most 64 MiB), validates
/// 1..64 channels, 1..768,000 Hz metadata and 1..65,536 maximum render frames.
/// Empty clips reject. All subsequent operations are bounded, allocation-free,
/// exclusively borrowed and perform no I/O. Drop frees the owned clip off audio.
///
/// Playback has no resampling, crossfade, smoothing or device-clock mapping.
/// Consumers must run at the clip rate and serialize commands on the owning
/// audio thread; the API supplies no cross-thread command queue. Pause retains
/// position. Seek permits EOF and preserves play/pause. A loop is half-open,
/// nonempty and within the clip; enabling it moves a position outside its range
/// to its start. Boundary transitions can click. End stops playback at EOF.
pub struct MemoryPlayback {
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
    maximum_frames: usize,
    position: usize,
    playing: bool,
    loop_range: Option<Range<usize>>,
}
impl MemoryPlayback {
    pub fn new(
        samples: &[f32],
        channels: usize,
        sample_rate: u32,
        maximum_frames: usize,
    ) -> Result<Self, PlaybackError> {
        if !(1..=64).contains(&channels)
            || !(1..=768_000).contains(&sample_rate)
            || !(1..=65_536).contains(&maximum_frames)
        {
            return Err(PlaybackError::InvalidConfiguration);
        }
        if samples.is_empty()
            || samples.len() > 16_777_216
            || !samples.len().is_multiple_of(channels)
            || samples.iter().any(|s| !s.is_finite())
        {
            return Err(PlaybackError::InvalidSamples);
        }
        Ok(Self {
            samples: samples.to_vec(),
            channels,
            sample_rate,
            maximum_frames,
            position: 0,
            playing: false,
            loop_range: None,
        })
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }
    pub fn position(&self) -> usize {
        self.position
    }
    pub fn is_playing(&self) -> bool {
        self.playing
    }
    pub fn loop_range(&self) -> Option<Range<usize>> {
        self.loop_range.clone()
    }
    /// Start from the current position; EOF without a loop remains stopped.
    pub fn play(&mut self) {
        self.normalize_loop();
        self.playing = self.position < self.frames();
    }
    pub fn pause(&mut self) {
        self.playing = false;
    }
    /// Pause and return to frame zero, or the configured loop's start.
    pub fn stop(&mut self) {
        self.playing = false;
        self.position = self.loop_range.as_ref().map_or(0, |r| r.start);
    }
    pub fn seek(&mut self, frame: usize) -> Result<(), PlaybackError> {
        if frame > self.frames() {
            return Err(PlaybackError::InvalidPosition);
        }
        self.position = frame;
        self.normalize_loop();
        Ok(())
    }
    pub fn set_loop(&mut self, range: Option<Range<usize>>) -> Result<(), PlaybackError> {
        if range
            .as_ref()
            .is_some_and(|r| r.start >= r.end || r.end > self.frames())
        {
            return Err(PlaybackError::InvalidLoop);
        }
        self.loop_range = range;
        self.normalize_loop();
        Ok(())
    }
    fn normalize_loop(&mut self) {
        if let Some(range) = &self.loop_range
            && !range.contains(&self.position)
        {
            self.position = range.start;
        }
    }
    /// Render complete interleaved frames, filling every supplied sample.
    /// Reject malformed/oversized buffers before advancing; zero-frame renders
    /// preserve play/pause and report current EOF. Paused/end suffixes are zero.
    pub fn render(&mut self, output: &mut [f32]) -> Result<PlaybackProgress, PlaybackError> {
        output.fill(0.);
        if !output.len().is_multiple_of(self.channels)
            || output.len() / self.channels > self.maximum_frames
        {
            return Err(PlaybackError::InvalidSamples);
        }
        let mut progress = PlaybackProgress::default();
        let requested = output.len() / self.channels;
        while self.playing && progress.source_frames < requested {
            let end = self.loop_range.as_ref().map_or(self.frames(), |r| r.end);
            let count = (end - self.position).min(requested - progress.source_frames);
            let destination = progress.source_frames * self.channels;
            let source = self.position * self.channels;
            output[destination..destination + count * self.channels]
                .copy_from_slice(&self.samples[source..source + count * self.channels]);
            self.position += count;
            progress.source_frames += count;
            if self.position == end {
                if let Some(range) = &self.loop_range {
                    self.position = range.start;
                    progress.loop_wraps += 1;
                } else {
                    self.playing = false;
                }
            }
        }
        progress.ended = self.loop_range.is_none() && self.position == self.frames();
        Ok(progress)
    }
}
