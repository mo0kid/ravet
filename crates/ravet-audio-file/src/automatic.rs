//! Automatic bounded read-ahead workers and prepared callback-side transport.
use crate::background::{BackgroundWorker, PumpProgress, Shared};
use crate::{AudioConsumer, AudioFileError, PlaybackError, ReadAheadProgress};
use std::sync::{Arc, atomic::Ordering};

/// Worker-only source for an automatic read-ahead owner.
/// Implementations must preserve queued/pending/cursor state on failed stopped
/// seeks, and must publish only complete frames to their unique producer. Pump
/// calls must be bounded; worker backend allocations and I/O remain explicit.
pub trait ReadAheadPump: Send + 'static {
    /// Perform one bounded read/publication attempt outside the audio callback.
    fn pump(&mut self) -> Result<ReadAheadProgress, AudioFileError>;
    /// Check that this source owns the consumer's matching producer.
    fn owns(&self, consumer: &AudioConsumer) -> bool;
    /// Remaining worker scratch frames not yet published to the ring.
    fn pending_frames(&self) -> usize;
    /// Decoded/read position, including pending and queued frames.
    fn reader_position(&self) -> u64;
    /// Fixed source sample rate; no implicit rate conversion is performed.
    fn sample_rate(&self) -> u32;
    /// Position exactly, clearing the old generation only after success.
    fn seek_stopped(
        &mut self,
        consumer: &mut AudioConsumer,
        frame: u64,
    ) -> Result<(), AudioFileError>;
}
/// Snapshot of worker progress. EOF denotes all pending frames published;
/// audible EOF additionally requires the callback ring to drain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadAheadStatus {
    Running,
    EndOfFile,
    Failed,
    Stopped,
}
impl ReadAheadStatus {
    fn from_byte(value: u8) -> Self {
        match value {
            0 => Self::Running,
            1 => Self::EndOfFile,
            2 => Self::Failed,
            _ => Self::Stopped,
        }
    }
}
/// Bounded source consumption; zero output suffixes do not advance the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferedPlaybackProgress {
    pub source_frames: usize,
    /// Silence caused by unavailable source frames while the worker is running.
    /// Paused and known EOF/fault/stopped silence is not an underrun.
    pub underflow_frames: usize,
    pub ended: bool,
    pub worker_status: ReadAheadStatus,
}
/// Prepared consumer transport. Render/play/pause perform no heap, lock or I/O.
/// Source position advances only for consumed frames; underruns stall it. Paused
/// renders do not consume the ring. Output always fills with audio plus silence.
/// No device clock, rate conversion, crossfade, looping or live seek is provided.
/// Serialize commands with this exclusive endpoint on its owning audio thread;
/// destruction follows callback shutdown. Worker status snapshots may change
/// concurrently. A stopped seek belongs to the control owner with this endpoint
/// reclaimed after callback stop, never inside render.
pub struct BufferedPlayback {
    consumer: AudioConsumer,
    shared: Arc<Shared>,
    sample_rate: u32,
    maximum_frames: usize,
    position: u64,
    playing: bool,
}
impl BufferedPlayback {
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn channels(&self) -> usize {
        self.consumer.channels()
    }
    pub fn position_frames(&self) -> u64 {
        self.position
    }
    pub fn is_playing(&self) -> bool {
        self.playing
    }
    pub fn queued_frames(&self) -> usize {
        self.consumer.queued_frames()
    }
    pub fn worker_status(&self) -> ReadAheadStatus {
        ReadAheadStatus::from_byte(self.shared.status.load(Ordering::Acquire))
    }
    pub fn play(&mut self) {
        self.playing =
            self.worker_status() == ReadAheadStatus::Running || self.consumer.queued_frames() != 0;
    }
    pub fn pause(&mut self) {
        self.playing = false;
    }
    /// Invalid/oversized shapes or conservative u64 cursor-addition overflow
    /// silence the complete supplied buffer and preserve cursor/play state/queue. Zero frames preserve play/pause. After complete
    /// worker publication, EOF is reported only once the queue drains. Faults may
    /// leave earlier queued audio: consume it once, then stop, reporting Failed.
    pub fn render(
        &mut self,
        output: &mut [f32],
    ) -> Result<BufferedPlaybackProgress, PlaybackError> {
        output.fill(0.);
        let channels = self.channels();
        if !output.len().is_multiple_of(channels) || output.len() / channels > self.maximum_frames {
            return Err(PlaybackError::InvalidSamples);
        }
        let requested = output.len() / channels;
        if self.position.checked_add(requested as u64).is_none() {
            return Err(PlaybackError::InvalidPosition);
        }
        let mut count = 0;
        if self.playing && requested != 0 {
            count = self
                .consumer
                .pop(output)
                .expect("whole-frame prepared output");
            self.position += count as u64;
        }
        let status = self.worker_status();
        let drained = self.consumer.queued_frames() == 0;
        let ended = status == ReadAheadStatus::EndOfFile && drained;
        let underflow = if self.playing && status == ReadAheadStatus::Running {
            requested - count
        } else {
            0
        };
        if self.playing && requested != 0 && status != ReadAheadStatus::Running && drained {
            self.playing = false;
        }
        Ok(BufferedPlaybackProgress {
            source_frames: count,
            underflow_frames: underflow,
            ended,
            worker_status: status,
        })
    }
}
/// Control-owned automatic worker. Explicit `start` starts a named thread; a full ring
/// parks the worker with a 2 ms timeout before retrying, without callback-side wake
/// calls. Pump working allocations/I/O remain source-specific. Control stop/seek/
/// shutdown/Drop can block while joining and must never run in audio callbacks.
/// Stop is cooperative between bounded pump calls; it does not cancel active file
/// I/O and cannot guarantee a join deadline. Worker panic reports Failed, loses
/// the unwound source and causes control reclamation to return Poisoned.
/// The transfer mutex is used only during startup/exit/control operations; pumping
/// and callback rendering never take it. Thread creation failure retains the pump.
/// Callback positions require the source's sample rate; no clock is created.
pub struct ReadAheadWorker<P: ReadAheadPump> {
    inner: BackgroundWorker<P>,
}
impl<P: ReadAheadPump> ReadAheadWorker<P> {
    /// Prepare matching endpoints, no pending/queued frames, source rate 1..768000
    /// and 1..65536 render frames. An already-positioned empty pump is accepted.
    /// Playback is paused and the worker stopped until `start`. No device,
    /// filesystem path or callback thread is selected implicitly.
    pub fn new(
        pump: P,
        consumer: AudioConsumer,
        maximum_frames: usize,
    ) -> Result<(Self, BufferedPlayback), AudioFileError> {
        if !pump.owns(&consumer) || pump.pending_frames() != 0 || consumer.queued_frames() != 0 {
            return Err(AudioFileError::InvalidSamples);
        }
        if !(1..=65_536).contains(&maximum_frames) {
            return Err(AudioFileError::Capacity);
        }
        let sample_rate = pump.sample_rate();
        if !(1..=768_000).contains(&sample_rate) {
            return Err(AudioFileError::Unsupported);
        }
        let position = pump.reader_position();
        let inner = BackgroundWorker::new(pump);
        let shared = Arc::clone(&inner.shared);
        let owner = Self { inner };
        Ok((
            owner,
            BufferedPlayback {
                consumer,
                shared,
                sample_rate,
                maximum_frames,
                position,
                playing: false,
            },
        ))
    }
    /// Start the automatic worker after preparation. An active worker rejects;
    /// a completed worker is joined before restarting. OS thread creation failure
    /// returns an I/O error, retains the owned pump and reports Failed; callers
    /// may retry or reclaim it through shutdown. No source seek/reset is implicit.
    pub fn start(&mut self) -> Result<(), AudioFileError> {
        self.inner.start("ravet-read-ahead", |pump| {
            let progress = pump.pump()?;
            Ok(PumpProgress {
                progressed: progress.queued_frames != 0,
                completed: progress.end_of_file && progress.pending_frames == 0,
            })
        })
    }
    /// Reclaim the callback endpoint after stopping/joining its user. Foreign
    /// endpoints reject before stopping this worker. Join before source seek;
    /// seek failure preserves queue/pending/source/audio state and resumes the
    /// old generation. Success clears old audio, updates source position and
    /// preserves play/pause. OS restart failure retains the pump but sets Failed;
    /// generation changes already completed are not rolled back.
    pub fn seek_stopped(
        &mut self,
        playback: &mut BufferedPlayback,
        frame: u64,
    ) -> Result<(), AudioFileError> {
        if !Arc::ptr_eq(&self.inner.shared, &playback.shared) {
            return Err(AudioFileError::InvalidSamples);
        }
        self.inner.join()?;
        let result = {
            let mut storage = self
                .inner
                .storage
                .lock()
                .map_err(|_| AudioFileError::Poisoned)?;
            let pump = storage.source.as_mut().ok_or(AudioFileError::Poisoned)?;
            let result = pump.seek_stopped(&mut playback.consumer, frame);
            if result.is_ok() {
                storage.error = None;
            }
            result
        };
        if result.is_ok() {
            playback.position = frame;
        }
        self.start()?;
        result
    }
    /// Reclaim the stopped callback endpoint, seek to frame zero and pause.
    /// The restarted worker may prefetch the new generation while playback is
    /// paused. The source-seek/restart failure limits match `seek_stopped`.
    pub fn stop_stopped(&mut self, playback: &mut BufferedPlayback) -> Result<(), AudioFileError> {
        self.seek_stopped(playback, 0)?;
        playback.pause();
        Ok(())
    }
    /// Stop/join and reclaim the source together with its original pump error.
    /// Earlier delivered audio remains consumed. A worker panic returns Poisoned;
    /// its unwound source cannot be reclaimed. Destruction never finalizes a
    /// recording or performs a hidden seek.
    pub fn shutdown(mut self) -> Result<ReadAheadExit<P>, AudioFileError> {
        let exit = self.inner.shutdown()?;
        Ok(ReadAheadExit {
            pump: exit.source,
            error: exit.error,
        })
    }
}
/// Reclaimed stopped source and any original worker pump error.
pub struct ReadAheadExit<P> {
    pub pump: P,
    pub error: Option<AudioFileError>,
}
