//! Explicit automatic recorder ownership; capture callbacks retain prepared push.
use crate::background::{BackgroundWorker, PumpProgress};
use crate::{AudioFileError, AudioProducer};
use std::sync::atomic::Ordering;
/// Worker-only incremental recorder contract for an automatic owner.
/// Each pump must drain at most prepared scratch capacity and perform no work
/// when empty. Any failure after dequeue must poison subsequent source calls.
/// Finalization requires the matching producer stopped and performs a bounded
/// remaining-ring drain followed by explicit file/codec finalization.
pub trait RecordingPump: Send + 'static {
    /// Caller-selected sink returned only after successful consuming finish.
    type Destination;
    /// Stable exact owned consumer drained by pump/finish. It must identify the
    /// same ring for the recorder's lifetime, including permanent retirement proof.
    fn consumer(&self) -> &crate::AudioConsumer;
    /// Check exact prepared producer/consumer identity outside callbacks.
    fn owns(&self, producer: &AudioProducer) -> bool;
    /// Drain one bounded worker batch, returning successfully accepted frames.
    fn pump(&mut self) -> Result<usize, AudioFileError>;
    /// Source count semantics remain explicit: WAV/AIFF count successful writes;
    /// FLAC counts accepted encoder frames, including retained pending blocks.
    fn frames(&self) -> u64;
    /// Drain bounded remaining input and finalize the sink with producer stopped.
    fn finish(self) -> Result<Self::Destination, AudioFileError>;
}
/// Worker running/drain ownership is distinct from finalized-file success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingStatus {
    Running,
    Stopped,
    Failed,
    Finalized,
}
/// Control-owned bounded automatic recording pump. `new` validates the matching
/// prepared producer and retains the recorder; `start` creates a named worker.
/// Capture callbacks keep their unique AudioProducer and only call prepared push.
/// Full-ring overload remains explicit accepted-prefix/caller-drop policy; no
/// hidden allocation, blocking backpressure, device/path/clock choice occurs.
/// Pump allocations, encoding and I/O belong to the worker. Stop/join/finish/Drop
/// belong to control after capture shutdown; cooperative stop does not cancel
/// active I/O or guarantee a join deadline. Dropping the owner joins but never
/// drains/finalizes. A panicked worker loses its unwound recorder and reports
/// Failed/Poisoned, never successful finalization.
pub struct RecordingWorker<P: RecordingPump> {
    inner: BackgroundWorker<P>,
    queue: crate::ring::QueueIdentity,
}
impl<P: RecordingPump> RecordingWorker<P> {
    /// Validate queue identity before transferring the recorder. No thread or
    /// final output is created here; recorder preparation may already have
    /// written its caller-selected file header. Capture need not start yet.
    pub fn new(recorder: P, producer: &AudioProducer) -> Result<Self, AudioFileError> {
        if !recorder.owns(producer) {
            return Err(AudioFileError::InvalidSamples);
        }
        Ok(Self::from_recorder(recorder))
    }
    /// Prepare from the recorder's exact owned ring, without borrowing a producer.
    /// This accepts device capture adapters that keep the producer inside their
    /// callback. Finalize only after the producer is permanently retired/dropped.
    /// The RecordingPump consumer contract must identify its actual drained ring.
    pub fn from_recorder(recorder: P) -> Self {
        let queue = recorder.consumer().queue_identity();
        Self {
            inner: BackgroundWorker::new(recorder),
            queue,
        }
    }
    /// Begin automatic bounded draining. An active worker rejects. Empty input
    /// uses a 2 ms park timeout. OS thread creation failure retains recorder
    /// ownership and reports Failed; callers may retry or reclaim with shutdown.
    pub fn start(&mut self) -> Result<(), AudioFileError> {
        self.inner.start("ravet-recording", |recorder| {
            Ok(PumpProgress {
                progressed: recorder.pump()? != 0,
                completed: false,
            })
        })
    }
    /// Concurrent diagnostic snapshot; Running is not finalized-file evidence.
    pub fn status(&self) -> RecordingStatus {
        match self.inner.shared.status.load(Ordering::Acquire) {
            0 => RecordingStatus::Running,
            1 => RecordingStatus::Finalized,
            2 => RecordingStatus::Failed,
            _ => RecordingStatus::Stopped,
        }
    }
    /// Stop/join without draining or finalizing, retaining the recorder for a
    /// later start, explicit finish or shutdown. Pump faults remain Failed.
    pub fn stop(&mut self) -> Result<(), AudioFileError> {
        self.inner.join()
    }
    /// Reclaim the producer only after its capture user stops/joins. A foreign
    /// producer rejects before stopping this worker. Join, then invoke the source's
    /// bounded remaining drain and consuming finish on control. Success yields
    /// a complete sink and Finalized; further start/finish reject. A worker pump
    /// error is returned once and leaves the owned poisoned recorder available to
    /// shutdown. A finalization error consumes its sink and cannot be retried or
    /// treated as a complete file. Late publication violates the stopped-producer
    /// lifecycle and can reject finish or lose frames.
    pub fn finish_stopped(
        &mut self,
        producer: &mut AudioProducer,
    ) -> Result<P::Destination, AudioFileError> {
        if !self.queue.matches(producer) {
            return Err(AudioFileError::InvalidSamples);
        }
        self.inner.finalize(|recorder| recorder.finish())
    }
    /// Finalize after the unique producer has been dropped, including hidden
    /// device callback producers. Live or leaked producers reject before stopping
    /// this worker. Retirement is permanent; the queue's acquire proof synchronizes
    /// its last publication, then joining permits the source's bounded final drain.
    /// Device/callback shutdown belongs to its owner; this method performs no
    /// device operation. Error/sink-consumption rules match finish_stopped.
    pub fn finish_after_producer_retired(&mut self) -> Result<P::Destination, AudioFileError> {
        if !self.queue.producer_retired() {
            return Err(AudioFileError::InvalidRecord);
        }
        self.inner.finalize(|recorder| recorder.finish())
    }
    /// Stop/join and reclaim the recorder plus its original pump error. This does
    /// no final drain/header patch/flush; queued input remains source-owned. The
    /// error is absent if already returned by either finish method. A worker panic
    /// returns Poisoned because its unwound recorder cannot be reclaimed.
    pub fn shutdown(mut self) -> Result<RecordingExit<P>, AudioFileError> {
        let exit = self.inner.shutdown()?;
        Ok(RecordingExit {
            recorder: exit.source,
            error: exit.error,
        })
    }
}
/// Reclaimed recorder and original worker error; both belong outside callbacks.
pub struct RecordingExit<P> {
    /// Owned stopped recorder; its queued input and poison state are unchanged.
    pub recorder: P,
    /// Original pump error, unless already returned by either finish method.
    pub error: Option<AudioFileError>,
}
