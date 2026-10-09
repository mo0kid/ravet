//! Owned background work; all construction and teardown occur off the audio thread.
use std::{
    fmt,
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub type Task = Box<dyn FnOnce() + Send + 'static>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    Full,
    Stopped,
}
impl fmt::Display for SubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Full => "background queue is full",
            Self::Stopped => "background queue stopped",
        })
    }
}
impl std::error::Error for SubmitError {}

/// Bounded serialized worker. Tasks own their captures; rejected tasks are
/// dropped without execution. Panics are contained, and accepted tasks drain on
/// destruction. Joining may block. Self-teardown detaches the joining handle;
/// the worker retains its receiver and completes safely.
pub struct SerialExecutor {
    sender: Option<SyncSender<Task>>,
    worker: Option<JoinHandle<()>>,
}
impl SerialExecutor {
    /// Zero capacity is rejected. Capacity excludes the currently running task.
    pub fn new(pending_capacity: usize) -> std::io::Result<Self> {
        if pending_capacity == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "zero task capacity",
            ));
        }
        let (sender, receiver) = mpsc::sync_channel::<Task>(pending_capacity);
        let worker = thread::Builder::new()
            .name("ravet-worker".into())
            .spawn(move || {
                for task in receiver {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
        })
    }
    /// Nonblocking queue submission. Task destruction can run consumer code.
    pub fn submit(&self, task: impl FnOnce() + Send + 'static) -> Result<(), SubmitError> {
        self.sender
            .as_ref()
            .ok_or(SubmitError::Stopped)?
            .try_send(Box::new(task))
            .map_err(|e| match e {
                TrySendError::Full(_) => SubmitError::Full,
                TrySendError::Disconnected(_) => SubmitError::Stopped,
            })
    }
}
impl Drop for SerialExecutor {
    fn drop(&mut self) {
        drop(self.sender.take());
        if let Some(worker) = self.worker.take()
            && worker.thread().id() != thread::current().id()
        {
            let _ = worker.join();
        }
    }
}
/// Serialized worker timer. First callback follows one interval. No catch-up
/// bursts. Owns its callback; stop/drop waits for an in-flight callback.
/// Self-teardown is safe and prevents further callbacks. Panics are contained.
pub struct PeriodicTask {
    state: Arc<(Mutex<bool>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}
impl PeriodicTask {
    pub fn start(
        interval: Duration,
        mut callback: impl FnMut() + Send + 'static,
    ) -> std::io::Result<Self> {
        if interval.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "zero timer interval",
            ));
        }
        let state = Arc::new((Mutex::new(false), Condvar::new()));
        let shared = state.clone();
        let worker = thread::Builder::new()
            .name("ravet-timer".into())
            .spawn(move || {
                let (mutex, wake) = &*shared;
                let mut stopped = mutex.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    let (guard, _) = wake
                        .wait_timeout_while(stopped, interval, |s| !*s)
                        .unwrap_or_else(|e| e.into_inner());
                    stopped = guard;
                    if *stopped {
                        break;
                    }
                    drop(stopped);
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut callback));
                    stopped = mutex.lock().unwrap_or_else(|e| e.into_inner());
                }
            })?;
        Ok(Self {
            state,
            worker: Some(worker),
        })
    }
    /// Idempotently stop and join; never call from an audio callback.
    pub fn stop(&mut self) {
        let (mutex, wake) = &*self.state;
        *mutex.lock().unwrap_or_else(|e| e.into_inner()) = true;
        wake.notify_all();
        if let Some(worker) = self.worker.take()
            && worker.thread().id() != thread::current().id()
        {
            let _ = worker.join();
        }
    }
}
impl Drop for PeriodicTask {
    fn drop(&mut self) {
        self.stop();
    }
}
