//! Control-owned callback retirement. The only unsafe boundary is the unique
//! data callback's temporary mutable renderer borrow; extraction is safe ownership.
#![allow(unsafe_code)]

use crate::sample_rate::{Direction, StreamOpenError, verified_stream};
use crate::{Counters, OutputStatistics, PreparedOutput};
use cpal::traits::{DeviceTrait, StreamTrait};
use ravet_foundation::{PrepareError, Processor};
use std::{
    cell::UnsafeCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Original control-side failure; no original backend/spawn error is cloned.
#[derive(Debug)]
pub enum ReclaimableOutputOpenError {
    Preparation(PrepareError),
    Backend(cpal::Error),
    /// Stream rejected before play; processor retains normal retirement ownership.
    SampleRateMismatch {
        requested: u32,
        actual: u32,
    },
    WorkerSpawn(std::io::Error),
}
impl std::fmt::Display for ReclaimableOutputOpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preparation(e) => e.fmt(f),
            Self::Backend(e) => e.fmt(f),
            Self::SampleRateMismatch { requested, actual } => write!(
                f,
                "Device sample rate mismatch: requested {requested} Hz, actual {actual} Hz"
            ),
            Self::WorkerSpawn(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ReclaimableOutputOpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Preparation(e) => e,
            Self::Backend(e) => e,
            Self::SampleRateMismatch { .. } => return None,
            Self::WorkerSpawn(e) => e,
        })
    }
}
impl From<StreamOpenError> for ReclaimableOutputOpenError {
    fn from(error: StreamOpenError) -> Self {
        match error {
            StreamOpenError::Backend(error) => Self::Backend(error),
            StreamOpenError::SampleRateMismatch { requested, actual } => {
                Self::SampleRateMismatch { requested, actual }
            }
        }
    }
}
/// Opening failure keeps the original processor immediately or an explicitly
/// pending retirement proof when a backend may still own its callback captures.
pub enum FailedOutputOwner<P: Processor> {
    Immediate(P),
    Pending(OutputRetirement<P>),
}
/// Original opening error and exclusive processor recovery ownership.
pub struct ReclaimableOutputFailure<P: Processor> {
    error: ReclaimableOutputOpenError,
    owner: FailedOutputOwner<P>,
}
impl<P: Processor> ReclaimableOutputFailure<P> {
    pub fn error(&self) -> &ReclaimableOutputOpenError {
        &self.error
    }
    pub fn into_parts(self) -> (ReclaimableOutputOpenError, FailedOutputOwner<P>) {
        (self.error, self.owner)
    }
}
impl<P: Processor> std::fmt::Debug for ReclaimableOutputFailure<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReclaimableOutputFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl<P: Processor> std::fmt::Display for ReclaimableOutputFailure<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl<P: Processor> std::error::Error for ReclaimableOutputFailure<P> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
/// Lifecycle or exceptional worker failure. Callback/backend shutdown has no
/// timeout guarantee; a failed worker does not imply a successful owner receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputRetirementError {
    AlreadyReclaimed,
    Disconnected,
    WorkerPanicked,
    WorkerPollPanicked,
}
impl std::fmt::Display for OutputRetirementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "output retirement: {self:?}")
    }
}
impl std::error::Error for OutputRetirementError {}
/// Exclusive stopped processor ownership. A recovered worker fault is distinct
/// from normal retirement; neither outcome promises correct DSP history after a
/// processing panic (inspect statistics). Reopening explicitly prepares the owner.
pub enum ReclaimedOutput<P> {
    Reclaimed {
        processor: P,
        statistics: OutputStatistics,
    },
    WorkerFault {
        processor: P,
        statistics: OutputStatistics,
        error: OutputRetirementError,
    },
}
/// Control-side pending owner. Polling never waits. `wait` explicitly blocks on
/// control, with no deadline/cancellation guarantee. Early Drop detaches the
/// retirement worker; it retains captures until both callback owners are gone.
pub struct OutputRetirement<P> {
    receiver: mpsc::Receiver<ReclaimedOutput<P>>,
    worker: Option<JoinHandle<()>>,
    taken: bool,
}
impl<P> OutputRetirement<P> {
    pub fn try_reclaim(&mut self) -> Result<Option<ReclaimedOutput<P>>, OutputRetirementError> {
        if self.taken {
            return Err(OutputRetirementError::AlreadyReclaimed);
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                self.taken = true;
                Ok(Some(result))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                // A panicking worker may have dropped its sender before all its
                // unwind destructors finished. A poll must not join it yet.
                if self
                    .worker
                    .as_ref()
                    .is_some_and(|worker| !worker.is_finished())
                {
                    return Ok(None);
                }
                self.taken = true;
                Err(self.join_failure())
            }
        }
    }
    /// Block only on control. A foreign processor deactivation panic can lose
    /// ownership after safe callback retirement and is reported as WorkerPanicked.
    pub fn wait(mut self) -> Result<ReclaimedOutput<P>, OutputRetirementError> {
        if self.taken {
            return Err(OutputRetirementError::AlreadyReclaimed);
        }
        let result = self.receiver.recv();
        self.taken = true;
        match result {
            Ok(result) => {
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
                Ok(result)
            }
            Err(_) => Err(self.join_failure()),
        }
    }
    fn join_failure(&mut self) -> OutputRetirementError {
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            OutputRetirementError::WorkerPanicked
        } else {
            OutputRetirementError::Disconnected
        }
    }
}

struct RetirementCell<P: Processor> {
    renderer: UnsafeCell<Option<PreparedOutput<P>>>,
    // The error callback accesses only these immutable atomic counters, never
    // the renderer, so it cannot alias the data callback's exclusive borrow.
    counters: Arc<Counters>,
    data_retired: AtomicBool,
    error_retired: AtomicBool,
}
// SAFETY: P is Send, never shared through a public reference. Exactly one private
// DataGuard (not Clone) exclusively accesses renderer during its FnMut callback.
// ErrorGuard accesses separate atomics only. Worker never touches renderer until
// BOTH Release/Acquire retirement flags and sole strong ownership prove callback
// borrows and owners gone. There are no Weak owners or later clones. Extraction
// then consumes Arc and UnsafeCell safely. Worker retains its Arc outside catch-
// unwind polling scopes, including early receiver Drop and caught polling panic.
unsafe impl<P: Processor + Send> Sync for RetirementCell<P> {}
struct DataGuard<P: Processor> {
    cell: Arc<RetirementCell<P>>,
    disabled: bool,
}
impl<P: Processor> DataGuard<P> {
    fn render(&mut self, output: &mut [f32]) {
        // SAFETY: the unique non-Clone data guard is called through FnMut, so
        // this is its only live renderer borrow. Worker waits for its Drop and
        // actual Arc disappearance; error guard never accesses this field.
        let renderer = unsafe { &mut *self.cell.renderer.get() }
            .as_mut()
            .expect("live renderer");
        if self.disabled {
            renderer.counters.callbacks.fetch_add(1, Ordering::Relaxed);
            output.fill(0.0);
            return;
        }
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render(output)))
            .is_err()
        {
            renderer.counters.panics.fetch_add(1, Ordering::Relaxed);
            self.disabled = true;
            output.fill(0.0);
        }
    }
}
impl<P: Processor> Drop for DataGuard<P> {
    fn drop(&mut self) {
        self.cell.data_retired.store(true, Ordering::Release);
    }
}
struct ErrorGuard<P: Processor> {
    cell: Arc<RetirementCell<P>>,
}
impl<P: Processor> ErrorGuard<P> {
    fn report(&self) {
        self.cell
            .counters
            .backend_errors
            .fetch_add(1, Ordering::Relaxed);
    }
}
impl<P: Processor> Drop for ErrorGuard<P> {
    fn drop(&mut self) {
        self.cell.error_retired.store(true, Ordering::Release);
    }
}

type SpawnJob = Box<dyn FnOnce() + Send>;
fn spawn_worker(job: SpawnJob) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("ravet-output-retirement".into())
        .spawn(job)
}
fn retire<P: Processor + Send + 'static>(
    cell: Arc<RetirementCell<P>>,
    sender: mpsc::Sender<ReclaimedOutput<P>>,
    mut poll_hook: Option<Box<dyn FnOnce() + Send>>,
) {
    let mut fault = None;
    // The permanent cell owner is outside this unwind scope. A poll panic must
    // never destroy callback storage before actual callback retirement.
    loop {
        let ready = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(hook) = poll_hook.take() {
                hook();
            }
            cell.data_retired.load(Ordering::Acquire)
                && cell.error_retired.load(Ordering::Acquire)
                && Arc::strong_count(&cell) == 1
        }));
        match ready {
            Ok(true) => break,
            Ok(false) => {}
            Err(_) => fault = Some(OutputRetirementError::WorkerPollPanicked),
        }
        thread::sleep(Duration::from_millis(1));
    }
    // No Weak or future cloning exists; count==1 remains stable. Both guards
    // finished, including Arc field Drop. Safe consuming extraction needs no
    // unsafe move and performs all renderer/scratch/processor disposal here.
    let cell = match Arc::try_unwrap(cell) {
        Ok(cell) => cell,
        Err(_) => unreachable!("sole retired owner"),
    };
    let renderer = cell.renderer.into_inner().expect("retired renderer");
    let statistics = renderer.statistics();
    let processor = renderer.into_processor();
    let outcome = match fault {
        Some(error) => ReclaimedOutput::WorkerFault {
            processor,
            statistics,
            error,
        },
        None => ReclaimedOutput::Reclaimed {
            processor,
            statistics,
        },
    };
    // Disconnected payload drops on this control worker, never in a callback.
    let _ = sender.send(outcome);
}
fn start_prepared<P: Processor + Send + 'static, S>(
    renderer: PreparedOutput<P>,
    build: impl FnOnce(DataGuard<P>, ErrorGuard<P>) -> Result<S, ReclaimableOutputOpenError>,
    spawn: impl FnOnce(SpawnJob) -> std::io::Result<JoinHandle<()>>,
    poll_hook: Option<Box<dyn FnOnce() + Send>>,
) -> Result<(S, Arc<Counters>, OutputRetirement<P>), ReclaimableOutputFailure<P>> {
    let counters = Arc::clone(&renderer.counters);
    let cell = Arc::new(RetirementCell {
        renderer: UnsafeCell::new(Some(renderer)),
        counters: Arc::clone(&counters),
        data_retired: AtomicBool::new(false),
        error_retired: AtomicBool::new(false),
    });
    let data = DataGuard {
        cell: Arc::clone(&cell),
        disabled: false,
    };
    let errors = ErrorGuard {
        cell: Arc::clone(&cell),
    };
    let (sender, receiver) = mpsc::channel();
    let worker_cell = Arc::clone(&cell);
    let worker = match spawn(Box::new(move || retire(worker_cell, sender, poll_hook))) {
        Ok(worker) => worker,
        Err(error) => {
            drop(data);
            drop(errors);
            let cell = match Arc::try_unwrap(cell) {
                Ok(cell) => cell,
                Err(_) => unreachable!("spawn failure retains no worker"),
            };
            let processor = cell
                .renderer
                .into_inner()
                .expect("prepared owner")
                .into_processor();
            return Err(ReclaimableOutputFailure {
                error: ReclaimableOutputOpenError::WorkerSpawn(error),
                owner: FailedOutputOwner::Immediate(processor),
            });
        }
    };
    let retirement = OutputRetirement {
        receiver,
        worker: Some(worker),
        taken: false,
    };
    let stream = build(data, errors);
    drop(cell); // worker now retains the permanent control owner
    match stream {
        Ok(stream) => Ok((stream, counters, retirement)),
        Err(error) => Err(ReclaimableOutputFailure {
            error,
            owner: FailedOutputOwner::Pending(retirement),
        }),
    }
}

/// Native output plus explicitly pending stopped-processor recovery. Stop/Drop
/// happen on control. Framework callback capture Drop only publishes atomics and
/// releases a non-last Arc; native CPAL/OS destruction is a separate backend limit.
pub struct ReclaimableOutput<P: Processor> {
    stream: Option<cpal::Stream>,
    counters: Arc<Counters>,
    retirement: Option<OutputRetirement<P>>,
}
impl<P: Processor> ReclaimableOutput<P> {
    pub fn play(&self) -> Result<(), cpal::Error> {
        self.stream.as_ref().expect("live output").play()
    }
    pub fn pause(&self) -> Result<(), cpal::Error> {
        self.stream.as_ref().expect("live output").pause()
    }
    pub fn statistics(&self) -> OutputStatistics {
        self.counters.snapshot()
    }
    /// Drop native stream on control, then return a pending proof. CPAL may
    /// destroy callback captures asynchronously; this does not wait or seek.
    pub fn stop(mut self) -> OutputRetirement<P> {
        drop(self.stream.take());
        self.retirement.take().expect("retirement owner")
    }
}
impl<P: Processor> Drop for ReclaimableOutput<P> {
    fn drop(&mut self) {
        drop(self.stream.take());
    }
}
/// Open explicit f32 output with recoverable processor ownership and a dedicated
/// control retirement worker. Preflight/spawn failures return the original owner;
/// backend/rate-query/mismatch failures can return pending ownership. Post-build
/// rate verification follows [`crate::open_output`]. No implicit rate fallback,
/// scheduling/drift/physical support or teardown deadline is promised. Processing
/// panic permanently silences the data callback and has exceptional runtime cost.
pub fn open_reclaimable_output<P: Processor + Send + 'static>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    maximum_frames: usize,
    processor: P,
) -> Result<ReclaimableOutput<P>, ReclaimableOutputFailure<P>> {
    if matches!(config.buffer_size, cpal::BufferSize::Fixed(0)) {
        return Err(ReclaimableOutputFailure {
            error: ReclaimableOutputOpenError::Preparation(PrepareError::InvalidMaximumFrames),
            owner: FailedOutputOwner::Immediate(processor),
        });
    }
    let renderer = PreparedOutput::new_owned(
        processor,
        config.sample_rate,
        usize::from(config.channels),
        maximum_frames,
    )
    .map_err(|failure| {
        let (processor, error) = failure.into_parts();
        ReclaimableOutputFailure {
            error: ReclaimableOutputOpenError::Preparation(error),
            owner: FailedOutputOwner::Immediate(processor),
        }
    })?;
    let (stream, counters, retirement) = start_prepared(
        renderer,
        |mut data, errors| {
            verified_stream(device, Direction::Output, config.sample_rate, || {
                device.build_output_stream(
                    config,
                    move |output: &mut [f32], _| data.render(output),
                    move |_| errors.report(),
                    None,
                )
            })
            .map_err(ReclaimableOutputOpenError::from)
        },
        spawn_worker,
        None,
    )?;
    Ok(ReclaimableOutput {
        stream: Some(stream),
        counters,
        retirement: Some(retirement),
    })
}

#[cfg(test)]
#[path = "reclaimable_tests.rs"]
mod tests;
