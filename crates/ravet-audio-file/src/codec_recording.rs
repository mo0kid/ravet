//! Codec ownership remains local to the worker that creates it.
use crate::{AudioConsumer, AudioFileError as Error, AudioProducer};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
const RUN: u8 = 0;
const PAUSE: u8 = 1;
const FINISH: u8 = 2;
const ABORT: u8 = 3;
/// Diagnostic lifecycle; only a finalized exit proves explicit codec finalization and successful flush.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodecRecordingStatus {
    Prepared,
    Starting,
    Running,
    Paused,
    Failed,
    Finalized,
    Aborted,
}
impl CodecRecordingStatus {
    pub(crate) fn code(self) -> u8 {
        self as u8
    }
    pub(crate) fn from_code(code: u8) -> Self {
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
struct Preparation<W, F> {
    sink: W,
    format: F,
    consumer: AudioConsumer,
    scratch: Vec<f32>,
    maximum_frames: u64,
    maximum_bytes: u64,
}
enum Transfer<W, C: CodecFactory<W>> {
    Prepared(Preparation<W, C::Format>),
    Exited(CodecRecordingExit<W>),
}
struct Shared {
    command: AtomicU8,
    status: AtomicU8,
    accepted: AtomicU64,
    dequeued: AtomicU64,
    written: AtomicU64,
    file: AtomicU64,
}
/// Mutually exclusive completion evidence carried by a worker-constructed exit.
/// Failed retains the first original startup/pump/drain/explicit codec finalization/flush error.
#[derive(Debug)]
pub(crate) enum CodecRecordingCompletion {
    Finalized,
    Aborted,
    Failed(Error),
}
/// Explicit complete or incomplete worker outcome, with reclaimed sink/source.
/// An `Ok` controller result alone does not prove a complete recording: inspect
/// `completion()`. Irreversible output prefixes are retained on faults.
#[must_use = "inspect completion evidence before treating a recovered sink as finalized"]
pub(crate) struct CodecRecordingExit<W> {
    /// Recovered complete or partial destination; inspect completion first.
    pub destination: W,
    /// Recovered exact consumer, including retained queued suffix after failure.
    pub consumer: AudioConsumer,
    pub(crate) completion: CodecRecordingCompletion,
    /// Encoder-submitted frames, including any failing accepted codec batch.
    pub accepted_frames: u64,
    /// Ring-dequeued frames, including any rejected source batch.
    pub dequeued_frames: u64,
    /// Actual sink-accepted bytes, including partial failed writes.
    pub written_bytes: u64,
    pub file_bytes: u64,
}
/// Private shared transferable local-codec recording controller.
pub(crate) struct CodecRecordingWorker<W: Send + 'static, C: CodecFactory<W>> {
    transfer: Arc<Mutex<Option<Transfer<W, C>>>>,
    shared: Arc<Shared>,
    queue: crate::ring::QueueIdentity,
    thread: Option<JoinHandle<()>>,
}
impl<W: Send + 'static, C: CodecFactory<W>> CodecRecordingWorker<W, C> {
    /// Validate format/budgets and prepare 1..4096-frame scratch with the exact
    /// consumer before thread/codec/header output. The ring channel count must
    /// equal the selected mono/stereo format. Existing codec writer bounds apply.
    pub fn new(
        sink: W,
        format: C::Format,
        consumer: AudioConsumer,
        batch_frames: usize,
        maximum_frames: u64,
        maximum_bytes: u64,
    ) -> Result<Self, Error> {
        C::validate_configuration(format, maximum_frames, maximum_bytes)?;
        if !(1..=4096).contains(&batch_frames) || consumer.channels() != C::channels(format) {
            return Err(Error::InvalidSamples);
        }
        let queue = consumer.queue_identity();
        let scratch = vec![0.; batch_frames * C::channels(format)];
        Ok(Self {
            transfer: Arc::new(Mutex::new(Some(Transfer::Prepared(Preparation {
                sink,
                format,
                consumer,
                scratch,
                maximum_frames,
                maximum_bytes,
            })))),
            shared: Arc::new(Shared {
                command: AtomicU8::new(RUN),
                status: AtomicU8::new(CodecRecordingStatus::Prepared.code()),
                accepted: AtomicU64::new(0),
                dequeued: AtomicU64::new(0),
                written: AtomicU64::new(0),
                file: AtomicU64::new(0),
            }),
            queue,
            thread: None,
        })
    }
    /// Spawn once, acknowledging thread creation rather than codec/header success.
    /// Check status/outcome for startup faults. OS spawn failure keeps preparation
    /// recoverable by retry or abort; the codec is never constructed on control.
    pub fn start(&mut self) -> Result<(), Error> {
        self.start_with(|operation| {
            thread::Builder::new()
                .name(C::THREAD_NAME.into())
                .spawn(operation)
        })
    }
    pub(crate) fn start_with(
        &mut self,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<JoinHandle<()>>,
    ) -> Result<(), Error> {
        if self.thread.is_some()
            || !matches!(
                self.transfer.lock().map_err(|_| Error::Poisoned)?.as_ref(),
                Some(Transfer::Prepared(_))
            )
        {
            return Err(Error::InvalidRecord);
        }
        self.shared.command.store(RUN, Ordering::Release);
        self.shared
            .status
            .store(CodecRecordingStatus::Starting.code(), Ordering::Release);
        let transfer = Arc::clone(&self.transfer);
        let shared = Arc::clone(&self.shared);
        match spawn(Box::new(move || run::<W, C>(transfer, shared))) {
            Ok(worker) => {
                self.thread = Some(worker);
                Ok(())
            }
            Err(error) => {
                self.shared
                    .status
                    .store(CodecRecordingStatus::Failed.code(), Ordering::Release);
                Err(error.into())
            }
        }
    }
    pub fn status(&self) -> CodecRecordingStatus {
        CodecRecordingStatus::from_code(self.shared.status.load(Ordering::Acquire))
    }
    /// Encoder submitted frames, including any failing accepted batch.
    pub fn accepted_frames(&self) -> u64 {
        self.shared.accepted.load(Ordering::Acquire)
    }
    /// Ring frames dequeued, including a batch rejected before encoder acceptance.
    pub fn dequeued_frames(&self) -> u64 {
        self.shared.dequeued.load(Ordering::Acquire)
    }
    #[cfg(feature = "mp3-encode")]
    pub fn file_bytes(&self) -> u64 {
        self.shared.file.load(Ordering::Acquire)
    }
    /// Sink-accepted bytes, including irreversible partial writes.
    pub fn written_bytes(&self) -> u64 {
        self.shared.written.load(Ordering::Acquire)
    }
    fn wake(&self) {
        if let Some(worker) = &self.thread {
            worker.thread().unpark();
        }
    }
    /// Wait for quiescent acknowledgement after active I/O. No queue frames are
    /// consumed after success until resume/finalization. A failed worker rejects
    /// without discarding its original operational error.
    pub fn pause(&mut self) -> Result<(), Error> {
        if self.thread.is_none()
            || matches!(
                self.status(),
                CodecRecordingStatus::Failed
                    | CodecRecordingStatus::Finalized
                    | CodecRecordingStatus::Aborted
            )
        {
            return Err(Error::InvalidRecord);
        }
        self.shared
            .command
            .compare_exchange(RUN, PAUSE, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::InvalidRecord)?;
        self.wake();
        loop {
            match self.status() {
                CodecRecordingStatus::Paused => return Ok(()),
                CodecRecordingStatus::Failed => return Err(Error::Poisoned),
                _ => {}
            }
            if self
                .thread
                .as_ref()
                .is_some_and(|worker| worker.is_finished())
            {
                return Err(Error::Poisoned);
            }
            thread::park_timeout(Duration::from_millis(2));
        }
    }
    /// Resume the same worker-local codec and wait for its running acknowledgement
    /// without reconstructing headers/state. Active I/O can block this control call.
    pub fn resume(&mut self) -> Result<(), Error> {
        if self.thread.is_none() || self.status() != CodecRecordingStatus::Paused {
            return Err(Error::InvalidRecord);
        }
        self.shared
            .command
            .compare_exchange(PAUSE, RUN, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::InvalidRecord)?;
        self.wake();
        loop {
            match self.status() {
                CodecRecordingStatus::Running => return Ok(()),
                CodecRecordingStatus::Failed => return Err(Error::Poisoned),
                _ => {}
            }
            if self
                .thread
                .as_ref()
                .is_some_and(|worker| worker.is_finished())
            {
                return Err(Error::Poisoned);
            }
            thread::park_timeout(Duration::from_millis(2));
        }
    }
    /// Validate a reclaimed stopped producer before requesting final drain/explicit codec finalization.
    /// The bounded final ring tail is encoded on the owning worker. The returned
    /// exit preserves sink/source and original startup/pump/explicit codec finalization/flush error; only
    /// CodecRecordingCompletion::Finalized means complete. A foreign producer preserves the
    /// current worker. Late capture violates this stopped-producer lifecycle.
    pub fn finish_stopped(
        &mut self,
        producer: &mut AudioProducer,
    ) -> Result<CodecRecordingExit<W>, Error> {
        if !self.queue.matches(producer) {
            return Err(Error::InvalidSamples);
        }
        self.finish()
    }
    /// Finish after permanent producer Drop, including capture adapters that hide
    /// it in their callback. Live/leaked producers reject before worker changes.
    pub fn finish_after_producer_retired(&mut self) -> Result<CodecRecordingExit<W>, Error> {
        if !self.queue.producer_retired() {
            return Err(Error::InvalidRecord);
        }
        self.finish()
    }
    fn finish(&mut self) -> Result<CodecRecordingExit<W>, Error> {
        if self.thread.is_none()
            && matches!(
                self.transfer.lock().map_err(|_| Error::Poisoned)?.as_ref(),
                Some(Transfer::Prepared(_))
            )
        {
            self.start()?;
        }
        self.take_exit(FINISH)
    }
    fn take_exit(&mut self, command: u8) -> Result<CodecRecordingExit<W>, Error> {
        self.shared.command.store(command, Ordering::Release);
        self.wake();
        if let Some(worker) = self.thread.take()
            && worker.join().is_err()
        {
            self.shared
                .status
                .store(CodecRecordingStatus::Failed.code(), Ordering::Release);
            return Err(Error::Poisoned);
        }
        let mut slot = self.transfer.lock().map_err(|_| Error::Poisoned)?;
        match slot.take() {
            Some(Transfer::Exited(exit)) => Ok(exit),
            Some(Transfer::Prepared(preparation)) if command == ABORT => {
                self.shared
                    .status
                    .store(CodecRecordingStatus::Aborted.code(), Ordering::Release);
                Ok(CodecRecordingExit {
                    destination: preparation.sink,
                    consumer: preparation.consumer,
                    completion: CodecRecordingCompletion::Aborted,
                    accepted_frames: 0,
                    dequeued_frames: 0,
                    written_bytes: 0,
                    file_bytes: 0,
                })
            }
            _ => Err(Error::Poisoned),
        }
    }
    /// Join and return an incomplete sink/source without final drain/explicit codec finalization/flush.
    /// Prepared abort leaves the sink untouched. Original operational faults are
    /// delivered once in the exit; panic cannot reclaim the unwound local owner.
    pub fn abort(mut self) -> Result<CodecRecordingExit<W>, Error> {
        self.take_exit(ABORT)
    }
}
impl<W: Send + 'static, C: CodecFactory<W>> Drop for CodecRecordingWorker<W, C> {
    fn drop(&mut self) {
        self.shared.command.store(ABORT, Ordering::Release);
        self.wake();
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}
fn run<W: Send + 'static, C: CodecFactory<W>>(
    transfer: Arc<Mutex<Option<Transfer<W, C>>>>,
    shared: Arc<Shared>,
) {
    struct Guard(Arc<Shared>, bool);
    impl Drop for Guard {
        fn drop(&mut self) {
            if self.1 {
                self.0
                    .status
                    .store(CodecRecordingStatus::Failed.code(), Ordering::Release);
            }
        }
    }
    let mut guard = Guard(Arc::clone(&shared), true);
    let preparation = match transfer
        .lock()
        .expect("transfer ownership")
        .take()
        .expect("prepared ownership")
    {
        Transfer::Prepared(preparation) => preparation,
        _ => unreachable!(),
    };
    let Preparation {
        sink,
        format,
        mut consumer,
        mut scratch,
        maximum_frames,
        maximum_bytes,
    } = preparation;
    if shared.command.load(Ordering::Acquire) == ABORT {
        shared
            .status
            .store(CodecRecordingStatus::Aborted.code(), Ordering::Release);
        *transfer.lock().expect("result ownership") = Some(Transfer::Exited(CodecRecordingExit {
            destination: sink,
            consumer,
            completion: CodecRecordingCompletion::Aborted,
            accepted_frames: 0,
            dequeued_frames: 0,
            written_bytes: 0,
            file_bytes: 0,
        }));
        guard.1 = false;
        return;
    }
    let mut writer = match C::initialize(sink, format, maximum_frames, maximum_bytes) {
        Ok(writer) => writer,
        Err(CodecRecovery {
            sink,
            completion,
            accepted_frames,
            written_bytes,
            file_bytes,
            ..
        }) => {
            assert!(
                matches!(completion, CodecRecordingCompletion::Failed(_)),
                "factory initialization errors retain failed completion"
            );
            shared.written.store(written_bytes, Ordering::Release);
            shared.file.store(file_bytes, Ordering::Release);
            shared
                .status
                .store(CodecRecordingStatus::Failed.code(), Ordering::Release);
            *transfer.lock().expect("result ownership") =
                Some(Transfer::Exited(CodecRecordingExit {
                    destination: sink,
                    consumer,
                    completion,
                    accepted_frames,
                    dequeued_frames: 0,
                    written_bytes,
                    file_bytes,
                }));
            guard.1 = false;
            return;
        }
    };
    shared
        .written
        .store(writer.written_bytes(), Ordering::Release);
    shared.file.store(writer.file_bytes(), Ordering::Release);
    let mut error = None;
    let mut dequeued = 0;
    let recovery = loop {
        match shared.command.load(Ordering::Acquire) {
            ABORT => break writer.abort_reclaim(error),
            FINISH => {
                if error.is_none() {
                    let mut remaining = consumer.queued_frames();
                    while remaining != 0 {
                        let samples = (remaining * C::channels(format)).min(scratch.len());
                        match pump::<W, C>(
                            &mut writer,
                            &mut consumer,
                            &mut scratch[..samples],
                            &shared,
                            &mut dequeued,
                        ) {
                            Ok(0) => {
                                error = Some(Error::InvalidRecord);
                                break;
                            }
                            Ok(frames) => remaining -= frames,
                            Err(failure) => {
                                error = Some(failure);
                                break;
                            }
                        }
                    }
                }
                break if error.is_some() {
                    writer.abort_reclaim(error)
                } else {
                    writer.finish_reclaim()
                };
            }
            PAUSE if error.is_none() => {
                shared
                    .status
                    .store(CodecRecordingStatus::Paused.code(), Ordering::Release);
                thread::park_timeout(Duration::from_millis(2));
            }
            _ if error.is_some() => {
                thread::park_timeout(Duration::from_millis(2));
            }
            _ => {
                shared
                    .status
                    .store(CodecRecordingStatus::Running.code(), Ordering::Release);
                match pump::<W, C>(
                    &mut writer,
                    &mut consumer,
                    &mut scratch,
                    &shared,
                    &mut dequeued,
                ) {
                    Ok(0) => thread::park_timeout(Duration::from_millis(2)),
                    Ok(_) => {}
                    Err(failure) => {
                        error = Some(failure);
                        shared
                            .status
                            .store(CodecRecordingStatus::Failed.code(), Ordering::Release);
                    }
                }
            }
        }
    };
    let CodecRecovery {
        sink,
        completion,
        accepted_frames,
        written_bytes,
        file_bytes,
    } = recovery;
    shared.accepted.store(accepted_frames, Ordering::Release);
    shared.written.store(written_bytes, Ordering::Release);
    shared.file.store(file_bytes, Ordering::Release);
    shared.status.store(
        match &completion {
            CodecRecordingCompletion::Finalized => CodecRecordingStatus::Finalized,
            CodecRecordingCompletion::Failed(_) => CodecRecordingStatus::Failed,
            CodecRecordingCompletion::Aborted => CodecRecordingStatus::Aborted,
        }
        .code(),
        Ordering::Release,
    );
    *transfer.lock().expect("result ownership") = Some(Transfer::Exited(CodecRecordingExit {
        destination: sink,
        consumer,
        completion,
        accepted_frames,
        dequeued_frames: dequeued,
        written_bytes,
        file_bytes,
    }));
    guard.1 = false;
}
fn pump<W: Send + 'static, C: CodecFactory<W>>(
    writer: &mut C::Session,
    consumer: &mut AudioConsumer,
    scratch: &mut [f32],
    shared: &Shared,
    dequeued: &mut u64,
) -> Result<usize, Error> {
    let frames = consumer.pop(scratch).map_err(|_| Error::InvalidSamples)?;
    *dequeued += frames as u64;
    shared.dequeued.store(*dequeued, Ordering::Release);
    if frames == 0 {
        return Ok(0);
    }
    let result = writer.write_samples(&scratch[..frames * consumer.channels()]);
    shared
        .accepted
        .store(writer.accepted_frames(), Ordering::Release);
    shared
        .written
        .store(writer.written_bytes(), Ordering::Release);
    shared.file.store(writer.file_bytes(), Ordering::Release);
    result.map(|()| frames)
}

pub(crate) struct CodecRecovery<W> {
    pub sink: W,
    pub completion: CodecRecordingCompletion,
    pub accepted_frames: u64,
    pub file_bytes: u64,
    pub written_bytes: u64,
}
pub(crate) trait CodecSession<W> {
    fn write_samples(&mut self, samples: &[f32]) -> Result<(), Error>;
    fn accepted_frames(&self) -> u64;
    fn written_bytes(&self) -> u64;
    fn file_bytes(&self) -> u64;
    fn abort_reclaim(self, error: Option<Error>) -> CodecRecovery<W>;
    fn finish_reclaim(self) -> CodecRecovery<W>;
}
pub(crate) trait CodecFactory<W>: 'static {
    type Format: Copy + Send + 'static;
    // Deliberately no Send bound: constructed, used and dropped on worker.
    type Session: CodecSession<W>;
    const THREAD_NAME: &'static str;
    fn channels(format: Self::Format) -> usize;
    fn validate_configuration(
        format: Self::Format,
        maximum_frames: u64,
        maximum_bytes: u64,
    ) -> Result<(), Error>;
    fn initialize(
        sink: W,
        format: Self::Format,
        maximum_frames: u64,
        maximum_bytes: u64,
    ) -> Result<Self::Session, CodecRecovery<W>>;
}
