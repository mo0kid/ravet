//! Private common ownership for cooperative source workers.
use crate::AudioFileError;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
pub(crate) struct Shared {
    pub(crate) running: AtomicBool,
    pub(crate) status: AtomicU8,
}
pub(crate) struct Storage<P> {
    pub(crate) source: Option<P>,
    pub(crate) error: Option<AudioFileError>,
}
pub(crate) struct WorkerExit<P> {
    pub(crate) source: P,
    pub(crate) error: Option<AudioFileError>,
}
pub(crate) struct PumpProgress {
    pub(crate) progressed: bool,
    pub(crate) completed: bool,
}
pub(crate) struct BackgroundWorker<P: Send + 'static> {
    pub(crate) storage: Arc<Mutex<Storage<P>>>,
    pub(crate) shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}
impl<P: Send + 'static> BackgroundWorker<P> {
    pub(crate) fn new(source: P) -> Self {
        Self {
            storage: Arc::new(Mutex::new(Storage {
                source: Some(source),
                error: None,
            })),
            shared: Arc::new(Shared {
                running: AtomicBool::new(false),
                status: AtomicU8::new(3),
            }),
            thread: None,
        }
    }
    pub(crate) fn start(
        &mut self,
        name: &str,
        poll: impl Fn(&mut P) -> Result<PumpProgress, AudioFileError> + Send + 'static,
    ) -> Result<(), AudioFileError> {
        if self
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
        {
            return Err(AudioFileError::InvalidRecord);
        }
        self.join()?;
        if self
            .storage
            .lock()
            .map_err(|_| AudioFileError::Poisoned)?
            .source
            .is_none()
        {
            return Err(AudioFileError::Poisoned);
        }
        self.shared.running.store(true, Ordering::Release);
        self.shared.status.store(0, Ordering::Release);
        let storage = Arc::clone(&self.storage);
        let shared = Arc::clone(&self.shared);
        let result = thread::Builder::new().name(name.into()).spawn(move || {
            struct FailureGuard(Arc<Shared>, bool);
            impl Drop for FailureGuard {
                fn drop(&mut self) {
                    if self.1 {
                        self.0.status.store(2, Ordering::Release);
                    }
                }
            }
            let mut guard = FailureGuard(Arc::clone(&shared), true);
            let mut source = storage
                .lock()
                .expect("worker storage")
                .source
                .take()
                .expect("owned source");
            let mut error = None;
            while shared.running.load(Ordering::Acquire) {
                match poll(&mut source) {
                    Ok(progress) if progress.completed => {
                        shared.status.store(1, Ordering::Release);
                        break;
                    }
                    Ok(progress) => {
                        if !progress.progressed {
                            thread::park_timeout(Duration::from_millis(2));
                        }
                    }
                    Err(failure) => {
                        error = Some(failure);
                        shared.status.store(2, Ordering::Release);
                        break;
                    }
                }
            }
            if shared.status.load(Ordering::Acquire) == 0 {
                shared.status.store(3, Ordering::Release);
            }
            let mut slot = storage.lock().expect("worker storage");
            slot.source = Some(source);
            slot.error = slot.error.take().or(error);
            guard.1 = false;
        });
        match result {
            Ok(thread) => {
                self.thread = Some(thread);
                Ok(())
            }
            Err(error) => {
                self.shared.running.store(false, Ordering::Release);
                self.shared.status.store(2, Ordering::Release);
                Err(error.into())
            }
        }
    }
    pub(crate) fn join(&mut self) -> Result<(), AudioFileError> {
        self.shared.running.store(false, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            if thread.join().is_err() {
                return Err(AudioFileError::Poisoned);
            }
        }
        Ok(())
    }
    pub(crate) fn shutdown(&mut self) -> Result<WorkerExit<P>, AudioFileError> {
        self.join()?;
        let mut slot = self.storage.lock().map_err(|_| AudioFileError::Poisoned)?;
        Ok(WorkerExit {
            source: slot.source.take().ok_or(AudioFileError::Poisoned)?,
            error: slot.error.take(),
        })
    }
    pub(crate) fn finalize<T>(
        &mut self,
        finish: impl FnOnce(P) -> Result<T, AudioFileError>,
    ) -> Result<T, AudioFileError> {
        self.join()?;
        let source = {
            let mut slot = self.storage.lock().map_err(|_| AudioFileError::Poisoned)?;
            if let Some(error) = slot.error.take() {
                return Err(error);
            }
            slot.source.take().ok_or(AudioFileError::Poisoned)?
        };
        self.shared.status.store(2, Ordering::Release);
        let result = finish(source);
        if result.is_ok() {
            self.shared.status.store(1, Ordering::Release);
        }
        result
    }
}
impl<P: Send + 'static> Drop for BackgroundWorker<P> {
    fn drop(&mut self) {
        let _ = self.join();
    }
}
