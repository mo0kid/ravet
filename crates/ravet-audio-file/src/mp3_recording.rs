//! Codec stays on one worker; private shared controller owns transferable lifecycle.
use crate::codec_recording::{CodecRecordingCompletion, CodecRecordingExit, CodecRecordingWorker};
use crate::mp3_factory::Mp3Factory;
use crate::{AudioConsumer, AudioFileError as Error, AudioProducer, Mp3Format};
use std::io::{Seek, Write};
/// Diagnostic lifecycle; only a finalized exit proves metadata rewrite and successful flush.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mp3RecordingStatus {
    Prepared,
    Starting,
    Running,
    Paused,
    Failed,
    Finalized,
    Aborted,
}
impl Mp3RecordingStatus {
    fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Prepared,
            1 => Self::Starting,
            2 => Self::Running,
            3 => Self::Paused,
            5 => Self::Finalized,
            6 => Self::Aborted,
            _ => Self::Failed,
        }
    }
}
/// Mutually exclusive completion evidence carried by a worker-constructed exit.
/// Failed retains the first original startup/pump/drain/metadata rewrite/flush error.
#[derive(Debug)]
pub enum Mp3RecordingCompletion {
    Finalized,
    Aborted,
    Failed(Error),
}
/// Explicit complete or incomplete worker outcome, with reclaimed sink/source.
/// An `Ok` controller result alone does not prove a complete recording: inspect
/// `completion()`. Irreversible output prefixes are retained on faults.
#[must_use = "inspect completion evidence before treating a recovered sink as finalized"]
pub struct Mp3RecordingExit<W> {
    /// Recovered complete or partial destination; inspect completion first.
    pub destination: W,
    /// Recovered exact consumer, including retained queued suffix after failure.
    pub consumer: AudioConsumer,
    completion: Mp3RecordingCompletion,
    /// Encoder-submitted frames, including any failing accepted codec batch.
    pub accepted_frames: u64,
    /// Ring-dequeued frames, including any rejected source batch.
    pub dequeued_frames: u64,
    /// Actual sink-accepted bytes, including partial failed writes.
    pub written_bytes: u64,
    /// Final/current file region bytes, excluding repeated metadata writes.
    pub file_bytes: u64,
}
impl<W> Mp3RecordingExit<W> {
    /// Mutually exclusive finalization/abort/original-failure evidence.
    pub fn completion(&self) -> &Mp3RecordingCompletion {
        &self.completion
    }
    /// Consume the private completion evidence with recovered sink and consumer.
    /// Inspect/save public diagnostic counts first if needed.
    pub fn into_parts(self) -> (W, AudioConsumer, Mp3RecordingCompletion) {
        (self.destination, self.consumer, self.completion)
    }
}
/// Prepared-ring automatic MP3 owner. The encoder is constructed, pumped,
/// explicitly finalized and dropped on one named worker. Capture keeps its
/// unique prepared producer; callback push adds no heap, lock, I/O or implicit
/// backpressure. Pause acknowledges quiescence after the active batch and I/O.
/// Control operations, join and Drop may block indefinitely on caller sink I/O.
/// Drop requests abort and joins; it never drains, rewrites metadata or flushes.
/// Sink destructors retain their own contract. Worker panic loses unwound owners.
/// An outer `Ok` proves recovery only; inspect [`Mp3RecordingExit::completion`]
/// before treating the destination as complete. Writer format, source, metadata,
/// physical-write budgets and independent LGPL requirements remain applicable.
/// No physical device, perceptual, resampling or callback deadline claim.
///
/// ```
/// use ravet_audio_file::{audio_ring, Mp3Format, Mp3Bitrate,
///     Mp3RecordingWorker, Mp3RecordingCompletion};
/// # fn main() -> Result<(), ravet_audio_file::AudioFileError> {
/// let (mut producer, consumer) = audio_ring(1, 4).unwrap();
/// let mut worker = Mp3RecordingWorker::new(std::io::Cursor::new(Vec::new()),
///     Mp3Format { channels: 1, sample_rate: 48000,
///         bit_rate: Mp3Bitrate::Kbps256 }, consumer, 2, 4, 65536)?;
/// worker.start()?;
/// producer.push(&[0.25, -0.25]).unwrap();
/// drop(producer); // after capture stop; proves no future publication
/// let exit = worker.finish_after_producer_retired()?;
/// match exit.completion() {
///     Mp3RecordingCompletion::Finalized => assert!(!exit.destination.into_inner().is_empty()),
///     Mp3RecordingCompletion::Failed(error) => panic!("recording failed: {error:?}"),
///     Mp3RecordingCompletion::Aborted => panic!("recording was aborted"),
/// }
/// # Ok(()) }
/// ```
pub struct Mp3RecordingWorker<W: Write + Seek + Send + 'static> {
    inner: CodecRecordingWorker<W, Mp3Factory>,
}
impl<W: Write + Seek + Send + 'static> Mp3RecordingWorker<W> {
    /// Validate configuration/queue and prepare 1..4096-frame source scratch, without codec/thread/header creation.
    pub fn new(
        sink: W,
        format: Mp3Format,
        consumer: AudioConsumer,
        batch_frames: usize,
        maximum_frames: u64,
        maximum_bytes: u64,
    ) -> Result<Self, Error> {
        Ok(Self {
            inner: CodecRecordingWorker::new(
                sink,
                format,
                consumer,
                batch_frames,
                maximum_frames,
                maximum_bytes,
            )?,
        })
    }
    /// Acknowledge OS spawn only; startup faults are carried by status/eventual completion. Spawn failure keeps preparation for retry/abort.
    pub fn start(&mut self) -> Result<(), Error> {
        self.inner.start()
    }
    #[cfg(test)]
    fn start_with(
        &mut self,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<std::thread::JoinHandle<()>>,
    ) -> Result<(), Error> {
        self.inner.start_with(spawn)
    }
    pub fn status(&self) -> Mp3RecordingStatus {
        Mp3RecordingStatus::from_code(self.inner.status().code())
    }
    /// Encoder-submitted frames, including a failing accepted batch.
    pub fn accepted_frames(&self) -> u64 {
        self.inner.accepted_frames()
    }
    /// Ring-dequeued frames, including a rejected source batch.
    pub fn dequeued_frames(&self) -> u64 {
        self.inner.dequeued_frames()
    }
    /// Actual sink-accepted bytes, including partial failed writes.
    pub fn written_bytes(&self) -> u64 {
        self.inner.written_bytes()
    }
    /// Current file region bytes, excluding repeated metadata writes.
    pub fn file_bytes(&self) -> u64 {
        self.inner.file_bytes()
    }
    /// Wait for active I/O and acknowledgement; no later dequeue until resume/finalize. No I/O cancellation/deadline.
    pub fn pause(&mut self) -> Result<(), Error> {
        self.inner.pause()
    }
    /// Resume same local codec and wait for running acknowledgement.
    pub fn resume(&mut self) -> Result<(), Error> {
        self.inner.resume()
    }
    /// Validate matching stopped producer before bounded tail drain/metadata rewrite/flush. Foreign proof preserves state; inspect completion, outer Ok proves recovery only.
    pub fn finish_stopped(
        &mut self,
        producer: &mut AudioProducer,
    ) -> Result<Mp3RecordingExit<W>, Error> {
        self.inner.finish_stopped(producer).map(convert)
    }
    /// Permanent producer Drop proves no later publication, including hidden capture adapters. Live/leaked proof preserves state.
    pub fn finish_after_producer_retired(&mut self) -> Result<Mp3RecordingExit<W>, Error> {
        self.inner.finish_after_producer_retired().map(convert)
    }
    /// Reclaim incomplete sink/source/original fault without tail drain/metadata rewrite/flush. Prepared abort touches no sink; panic loses unwound owners.
    pub fn abort(self) -> Result<Mp3RecordingExit<W>, Error> {
        self.inner.abort().map(convert)
    }
}
fn convert<W>(exit: CodecRecordingExit<W>) -> Mp3RecordingExit<W> {
    Mp3RecordingExit {
        destination: exit.destination,
        consumer: exit.consumer,
        completion: match exit.completion {
            CodecRecordingCompletion::Finalized => Mp3RecordingCompletion::Finalized,
            CodecRecordingCompletion::Aborted => Mp3RecordingCompletion::Aborted,
            CodecRecordingCompletion::Failed(error) => Mp3RecordingCompletion::Failed(error),
        },
        accepted_frames: exit.accepted_frames,
        dequeued_frames: exit.dequeued_frames,
        written_bytes: exit.written_bytes,
        file_bytes: exit.file_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spawn_failure_keeps_preparation_for_retry_and_explicit_finalization() {
        let (mut producer, consumer) = crate::audio_ring(1, 4).unwrap();
        producer.push(&[0.25]).unwrap();
        drop(producer);
        let mut worker = Mp3RecordingWorker::new(
            std::io::Cursor::new(Vec::new()),
            Mp3Format {
                channels: 1,
                sample_rate: 48000,
                bit_rate: crate::Mp3Bitrate::Kbps256,
            },
            consumer,
            1,
            4,
            65536,
        )
        .unwrap();
        let result = worker.start_with(|operation| {
            drop(operation);
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected spawn denial",
            ))
        });
        assert!(
            matches!(result,Err(Error::Io(error)) if error.to_string()=="injected spawn denial")
        );
        assert_eq!(worker.status(), Mp3RecordingStatus::Failed);
        worker.start().unwrap();
        let exit = worker.finish_after_producer_retired().unwrap();
        assert!(matches!(
            exit.completion(),
            Mp3RecordingCompletion::Finalized
        ));
        assert_eq!((exit.accepted_frames, exit.dequeued_frames), (1, 1));
    }
}
