//! Paired callback retirement; stopped ownership needs all four callback guards.
#![allow(unsafe_code)]
use crate::{
    ClockedDuplexError, ClockedDuplexOptions, ClockedDuplexStatistics, Counters, DeviceRenderer,
    InputStatistics, OutputStatistics, PreparedCapture, PreparedClockedDuplex,
};
use crate::{clocked::ClockCounters, input::CaptureCounters};
use cpal::traits::{DeviceTrait, StreamTrait};
use ravet_audio_file::AudioConsumer;
use ravet_foundation::{PrepareError, Processor};
use std::{
    cell::UnsafeCell,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Independently loaded control telemetry, not shared hardware timestamps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReclaimableClockedStatistics {
    pub input: InputStatistics,
    pub output: OutputStatistics,
    pub clock: ClockedDuplexStatistics,
    pub capture_panics: u64,
}
struct Statistics {
    input: Arc<CaptureCounters>,
    output: Arc<Counters>,
    clock: Arc<ClockCounters>,
    capture_panics: AtomicU64,
}
impl Statistics {
    fn snapshot(&self) -> ReclaimableClockedStatistics {
        ReclaimableClockedStatistics {
            input: self.input.snapshot(),
            output: self.output.snapshot(),
            clock: self.clock.snapshot(),
            capture_panics: self.capture_panics.load(Ordering::Relaxed),
        }
    }
}
/// Original opening failure; native build failures may leave pending owners.
#[derive(Debug)]
pub enum ReclaimableClockedOpenError {
    CapturePreparation(PrepareError),
    ClockedPreparation(ClockedDuplexError),
    InputBackend(cpal::Error),
    OutputBackend(cpal::Error),
    WorkerSpawn(std::io::Error),
}
impl std::fmt::Display for ReclaimableClockedOpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "clocked opening: {self:?}")
    }
}
impl std::error::Error for ReclaimableClockedOpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::CapturePreparation(e) => e,
            Self::ClockedPreparation(e) => e,
            Self::InputBackend(e) | Self::OutputBackend(e) => e,
            Self::WorkerSpawn(e) => e,
        })
    }
}
/// Exclusive original processor or proof pending asynchronous callback teardown.
pub enum FailedClockedOwner<P: Processor> {
    Immediate(P),
    Pending(ClockedDuplexRetirement<P>),
}
pub struct ReclaimableClockedFailure<P: Processor> {
    error: ReclaimableClockedOpenError,
    owner: FailedClockedOwner<P>,
}
impl<P: Processor> ReclaimableClockedFailure<P> {
    pub fn error(&self) -> &ReclaimableClockedOpenError {
        &self.error
    }
    pub fn into_parts(self) -> (ReclaimableClockedOpenError, FailedClockedOwner<P>) {
        (self.error, self.owner)
    }
}
impl<P: Processor> std::fmt::Debug for ReclaimableClockedFailure<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReclaimableClockedFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl<P: Processor> std::fmt::Display for ReclaimableClockedFailure<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl<P: Processor> std::error::Error for ReclaimableClockedFailure<P> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockedRetirementError {
    AlreadyReclaimed,
    Disconnected,
    WorkerPanicked,
    WorkerPollPanicked,
}
impl std::fmt::Display for ClockedRetirementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "clocked retirement: {self:?}")
    }
}
impl std::error::Error for ClockedRetirementError {}
/// All native callback owners are gone. No render or mutable-processor access.
/// Extraction belongs on control and disposes transient conversion/prefetched
/// history. Queued raw frames keep their actual cursor; no rollback is promised.
pub struct StoppedClockedDuplex<P: Processor> {
    renderer: PreparedClockedDuplex<P>,
    capture: PreparedCapture,
}
impl<P: Processor> StoppedClockedDuplex<P> {
    pub fn into_parts(self) -> (P, AudioConsumer, PreparedCapture) {
        let (processor, consumer) = self.renderer.into_parts();
        (processor, consumer, self.capture)
    }
    pub fn into_processor(self) -> P {
        self.into_parts().0
    }
}
/// Normal retirement versus caught worker-poll fault; both retain exclusive
/// owners after proof. Processing panic remains explicit statistics, not valid history.
pub enum ReclaimedClockedDuplex<P: Processor> {
    Reclaimed {
        owner: StoppedClockedDuplex<P>,
        statistics: ReclaimableClockedStatistics,
    },
    WorkerFault {
        owner: StoppedClockedDuplex<P>,
        statistics: ReclaimableClockedStatistics,
        error: ClockedRetirementError,
    },
}
/// Poll without waiting; explicit wait is CONTROL only with no deadline guarantee.
/// Early Drop detaches the worker, which retains both capture fields until proof.
///
/// ```
/// use ravet_device::{ClockedDuplexRetirement, ClockedRetirementError,
///     ReclaimedClockedDuplex};
/// use ravet_foundation::Processor;
/// // Control only: this explicitly waits, then consumes stopped ownership.
/// fn recover<P: Processor>(proof: ClockedDuplexRetirement<P>)
///     -> Result<(P, Option<ClockedRetirementError>), ClockedRetirementError>
/// {
///     Ok(match proof.wait()? {
///         ReclaimedClockedDuplex::Reclaimed { owner, .. } =>
///             (owner.into_processor(), None),
///         ReclaimedClockedDuplex::WorkerFault { owner, error, .. } =>
///             (owner.into_processor(), Some(error)),
///     })
/// }
/// ```
pub struct ClockedDuplexRetirement<P: Processor> {
    receiver: mpsc::Receiver<ReclaimedClockedDuplex<P>>,
    worker: Option<JoinHandle<()>>,
    taken: bool,
}
impl<P: Processor> ClockedDuplexRetirement<P> {
    pub fn try_reclaim(
        &mut self,
    ) -> Result<Option<ReclaimedClockedDuplex<P>>, ClockedRetirementError> {
        if self.taken {
            return Err(ClockedRetirementError::AlreadyReclaimed);
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                self.taken = true;
                Ok(Some(result))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                if self.worker.as_ref().is_some_and(|w| !w.is_finished()) {
                    return Ok(None);
                }
                self.taken = true;
                Err(self.join_failure())
            }
        }
    }
    pub fn wait(mut self) -> Result<ReclaimedClockedDuplex<P>, ClockedRetirementError> {
        if self.taken {
            return Err(ClockedRetirementError::AlreadyReclaimed);
        }
        self.taken = true;
        match self.receiver.recv() {
            Ok(result) => {
                if let Some(w) = self.worker.take() {
                    let _ = w.join();
                }
                Ok(result)
            }
            Err(_) => Err(self.join_failure()),
        }
    }
    fn join_failure(&mut self) -> ClockedRetirementError {
        if self.worker.take().is_some_and(|w| w.join().is_err()) {
            ClockedRetirementError::WorkerPanicked
        } else {
            ClockedRetirementError::Disconnected
        }
    }
}
struct Cell<P: Processor> {
    capture: UnsafeCell<Option<PreparedCapture>>,
    renderer: UnsafeCell<Option<PreparedClockedDuplex<P>>>,
    statistics: Arc<Statistics>,
    retired: [AtomicBool; 4],
}
// SAFETY: private, no Weak or public clones. Exactly one non-Clone FnMut input
// guard borrows capture, exactly one output guard borrows the DISJOINT renderer.
// Ring communication is its safe SPSC API. Error guards touch immutable atomics
// only. Worker retains an Arc outside unwind scopes; accesses neither field until
// all FOUR Acquire retirement flags AND stable sole ownership. Flags alone are
// insufficient: Arc field Drop may still be pending. No later clones exist then.
unsafe impl<P: Processor + Send> Sync for Cell<P> {}
struct InputData<P: Processor> {
    cell: Arc<Cell<P>>,
    disabled: bool,
}
impl<P: Processor> InputData<P> {
    fn capture(&mut self, input: &[f32]) {
        self.capture_with(|capture| capture.capture(input));
    }
    fn capture_with(&mut self, operation: impl FnOnce(&mut PreparedCapture)) {
        if self.disabled {
            return;
        }
        // SAFETY: unique FnMut input guard; other callbacks never borrow this field.
        let capture = unsafe { &mut *self.cell.capture.get() }
            .as_mut()
            .expect("live capture");
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(capture))).is_err() {
            self.cell
                .statistics
                .capture_panics
                .fetch_add(1, Ordering::Relaxed);
            self.disabled = true;
        }
    }
}
impl<P: Processor> Drop for InputData<P> {
    fn drop(&mut self) {
        self.cell.retired[0].store(true, Ordering::Release);
    }
}
struct InputError<P: Processor> {
    cell: Arc<Cell<P>>,
}
impl<P: Processor> InputError<P> {
    fn report(&self) {
        self.cell.statistics.input.report_error();
    }
}
impl<P: Processor> Drop for InputError<P> {
    fn drop(&mut self) {
        self.cell.retired[1].store(true, Ordering::Release);
    }
}
struct OutputData<P: Processor> {
    cell: Arc<Cell<P>>,
    disabled: bool,
}
impl<P: Processor> OutputData<P> {
    fn render(&mut self, output: &mut [f32]) {
        if self.disabled {
            self.cell
                .statistics
                .output
                .callbacks
                .fetch_add(1, Ordering::Relaxed);
            output.fill(0.);
            return;
        }
        // SAFETY: unique FnMut output guard; input borrows the disjoint field only.
        let renderer = unsafe { &mut *self.cell.renderer.get() }
            .as_mut()
            .expect("live renderer");
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render(output)))
            .is_err()
        {
            self.cell
                .statistics
                .output
                .panics
                .fetch_add(1, Ordering::Relaxed);
            self.disabled = true;
            output.fill(0.);
        }
    }
}
impl<P: Processor> Drop for OutputData<P> {
    fn drop(&mut self) {
        self.cell.retired[2].store(true, Ordering::Release);
    }
}
struct OutputError<P: Processor> {
    cell: Arc<Cell<P>>,
}
impl<P: Processor> OutputError<P> {
    fn report(&self) {
        self.cell
            .statistics
            .output
            .backend_errors
            .fetch_add(1, Ordering::Relaxed);
    }
}
impl<P: Processor> Drop for OutputError<P> {
    fn drop(&mut self) {
        self.cell.retired[3].store(true, Ordering::Release);
    }
}
type SpawnJob = Box<dyn FnOnce() + Send>;
fn spawn_worker(job: SpawnJob) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("ravet-clocked-retirement".into())
        .spawn(job)
}
fn retire<P: Processor + Send + 'static>(
    cell: Arc<Cell<P>>,
    sender: mpsc::Sender<ReclaimedClockedDuplex<P>>,
    mut hook: Option<Box<dyn FnOnce() + Send>>,
) {
    let mut fault = None;
    loop {
        let ready = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(hook) = hook.take() {
                hook();
            }
            cell.retired.iter().all(|flag| flag.load(Ordering::Acquire))
                && Arc::strong_count(&cell) == 1
        }));
        match ready {
            Ok(true) => break,
            Ok(false) => {}
            Err(_) => fault = Some(ClockedRetirementError::WorkerPollPanicked),
        }
        thread::sleep(Duration::from_millis(1));
    }
    let cell = match Arc::try_unwrap(cell) {
        Ok(cell) => cell,
        Err(_) => unreachable!("sole retired owner"),
    };
    let statistics = cell.statistics.snapshot();
    let owner = StoppedClockedDuplex {
        renderer: cell.renderer.into_inner().expect("retired renderer"),
        capture: cell.capture.into_inner().expect("retired capture"),
    };
    let outcome = match fault {
        None => ReclaimedClockedDuplex::Reclaimed { owner, statistics },
        Some(error) => ReclaimedClockedDuplex::WorkerFault {
            owner,
            statistics,
            error,
        },
    };
    // Disconnected payload destruction is this worker's, never a callback's.
    let _ = sender.send(outcome);
}
fn start_prepared<P: Processor + Send + 'static, S>(
    owner: StoppedClockedDuplex<P>,
    build: impl FnOnce(
        InputData<P>,
        InputError<P>,
        OutputData<P>,
        OutputError<P>,
    ) -> Result<S, ReclaimableClockedOpenError>,
    spawn: impl FnOnce(SpawnJob) -> std::io::Result<JoinHandle<()>>,
    hook: Option<Box<dyn FnOnce() + Send>>,
) -> Result<(S, Arc<Statistics>, ClockedDuplexRetirement<P>), ReclaimableClockedFailure<P>> {
    let statistics = Arc::new(Statistics {
        input: Arc::clone(&owner.capture.counters),
        output: Arc::clone(owner.renderer.counters()),
        clock: Arc::clone(&owner.renderer.clock_counters),
        capture_panics: AtomicU64::new(0),
    });
    let cell = Arc::new(Cell {
        capture: UnsafeCell::new(Some(owner.capture)),
        renderer: UnsafeCell::new(Some(owner.renderer)),
        statistics: Arc::clone(&statistics),
        retired: std::array::from_fn(|_| AtomicBool::new(false)),
    });
    let input = InputData {
        cell: Arc::clone(&cell),
        disabled: false,
    };
    let input_error = InputError {
        cell: Arc::clone(&cell),
    };
    let output = OutputData {
        cell: Arc::clone(&cell),
        disabled: false,
    };
    let output_error = OutputError {
        cell: Arc::clone(&cell),
    };
    let (sender, receiver) = mpsc::channel();
    let worker_cell = Arc::clone(&cell);
    let worker = match spawn(Box::new(move || retire(worker_cell, sender, hook))) {
        Ok(worker) => worker,
        Err(error) => {
            drop(input);
            drop(input_error);
            drop(output);
            drop(output_error);
            let cell = match Arc::try_unwrap(cell) {
                Ok(cell) => cell,
                Err(_) => unreachable!("spawn retains no owner"),
            };
            let owner = StoppedClockedDuplex {
                capture: cell.capture.into_inner().expect("prepared capture"),
                renderer: cell.renderer.into_inner().expect("prepared renderer"),
            };
            return Err(ReclaimableClockedFailure {
                error: ReclaimableClockedOpenError::WorkerSpawn(error),
                owner: FailedClockedOwner::Immediate(owner.into_processor()),
            });
        }
    };
    let retirement = ClockedDuplexRetirement {
        receiver,
        worker: Some(worker),
        taken: false,
    };
    let result = build(input, input_error, output, output_error);
    drop(cell);
    match result {
        Ok(streams) => Ok((streams, statistics, retirement)),
        Err(error) => Err(ReclaimableClockedFailure {
            error,
            owner: FailedClockedOwner::Pending(retirement),
        }),
    }
}
/// Explicit f32 pair. Control owns play/pause/stop; callbacks only release guards.
/// No physical synchronization, hotplug subscription or teardown deadline.
pub struct ReclaimableClockedDuplex<P: Processor> {
    input: Option<cpal::Stream>,
    output: Option<cpal::Stream>,
    statistics: Arc<Statistics>,
    retirement: Option<ClockedDuplexRetirement<P>>,
}
impl<P: Processor> ReclaimableClockedDuplex<P> {
    pub fn play(&self) -> Result<(), cpal::Error> {
        self.input.as_ref().expect("live input").play()?;
        if let Err(error) = self.output.as_ref().expect("live output").play() {
            let _ = self.input.as_ref().expect("live input").pause();
            return Err(error);
        }
        Ok(())
    }
    pub fn pause(&self) -> Result<(), cpal::Error> {
        let input = self.input.as_ref().expect("live input").pause();
        let output = self.output.as_ref().expect("live output").pause();
        input.and(output)
    }
    pub fn statistics(&self) -> ReclaimableClockedStatistics {
        self.statistics.snapshot()
    }
    pub fn stop(mut self) -> ClockedDuplexRetirement<P> {
        drop(self.input.take());
        drop(self.output.take());
        self.retirement.take().expect("retirement owner")
    }
}
impl<P: Processor> Drop for ReclaimableClockedDuplex<P> {
    fn drop(&mut self) {
        drop(self.input.take());
        drop(self.output.take());
    }
}
/// Open a matched clocked capture/render pair with original processor ownership.
/// All four callback owners must retire before recovery. No format/device fallback,
/// inferred timestamp alignment or native deadline/support guarantee follows.
pub fn open_reclaimable_clocked_duplex<P: Processor + Send + 'static>(
    input: &cpal::Device,
    output: &cpal::Device,
    input_config: cpal::StreamConfig,
    output_config: cpal::StreamConfig,
    ring_frames: usize,
    processor: P,
    options: ClockedDuplexOptions,
) -> Result<ReclaimableClockedDuplex<P>, ReclaimableClockedFailure<P>> {
    let validation = crate::clocked::validate_configuration(
        input_config.sample_rate,
        output_config.sample_rate,
        usize::from(input_config.channels),
        usize::from(output_config.channels),
        ring_frames,
        options,
    );
    let invalid_buffer = matches!(input_config.buffer_size, cpal::BufferSize::Fixed(0))
        || matches!(output_config.buffer_size, cpal::BufferSize::Fixed(0));
    if let Err(error) = validation.and({
        if invalid_buffer {
            Err(ClockedDuplexError::InvalidConfiguration)
        } else {
            Ok(())
        }
    }) {
        return Err(ReclaimableClockedFailure {
            error: ReclaimableClockedOpenError::ClockedPreparation(error),
            owner: FailedClockedOwner::Immediate(processor),
        });
    }
    let (capture, consumer) =
        match PreparedCapture::new(usize::from(input_config.channels), ring_frames) {
            Ok(parts) => parts,
            Err(error) => {
                return Err(ReclaimableClockedFailure {
                    error: ReclaimableClockedOpenError::CapturePreparation(error),
                    owner: FailedClockedOwner::Immediate(processor),
                });
            }
        };
    let renderer = match PreparedClockedDuplex::new_owned(
        processor,
        consumer,
        input_config.sample_rate,
        output_config.sample_rate,
        usize::from(output_config.channels),
        options,
    ) {
        Ok(renderer) => renderer,
        Err(failure) => {
            let (processor, _raw, error) = failure.into_parts();
            return Err(ReclaimableClockedFailure {
                error: ReclaimableClockedOpenError::ClockedPreparation(error),
                owner: FailedClockedOwner::Immediate(processor),
            });
        }
    };
    let ((input_stream, output_stream), statistics, retirement) = start_prepared(
        StoppedClockedDuplex { renderer, capture },
        |mut data, errors, mut output_data, output_errors| {
            let input_stream = input
                .build_input_stream(
                    input_config,
                    move |pcm: &[f32], _| data.capture(pcm),
                    move |_| errors.report(),
                    None,
                )
                .map_err(ReclaimableClockedOpenError::InputBackend)?;
            let output_stream = output
                .build_output_stream(
                    output_config,
                    move |pcm: &mut [f32], _| output_data.render(pcm),
                    move |_| output_errors.report(),
                    None,
                )
                .map_err(ReclaimableClockedOpenError::OutputBackend)?;
            Ok((input_stream, output_stream))
        },
        spawn_worker,
        None,
    )?;
    Ok(ReclaimableClockedDuplex {
        input: Some(input_stream),
        output: Some(output_stream),
        statistics,
        retirement: Some(retirement),
    })
}

#[cfg(test)]
#[path = "reclaimable_clocked_tests.rs"]
pub(super) mod tests;
