use crate::{DenormalError, DenormalMode, ScopedDenormalMode};
use ravet_foundation::{
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor, Sample, Tail,
};

/// Constructor refusal returns the exact uncalled processor and original native error.
pub struct DenormalConstructionFailure<P> {
    pub processor: P,
    pub error: DenormalError,
}
/// Explicit per-call scope around an exclusively owned portable processor.
///
/// Constructor probes only the constructing thread. Preparation delegates the
/// child's actual PrepareError; it cannot qualify a future AUDIO environment.
/// Every process call preflights the entire block, then re-admits that thread.
/// Native refusal silences all outputs without calling the child, retaining the
/// first original native error until reset/reprepare. Child faults or panics can
/// have committed histories; this adapter promises no rollback.
///
/// Manual start_audio/stop_audio must be paired on the AUDIO owner before CONTROL
/// deactivate or extraction. No Drop hook implicitly stops or deactivates P.
/// Child lifecycle panic leaves ownership uncertain until explicit successful
/// stop/deactivate; it is never reported as a successful extraction receipt.
pub struct DenormalProcessor<P> {
    processor: P,
    mode: DenormalMode,
    config: Option<ProcessConfig>,
    session: bool,
    uncertain: bool,
    faulted: bool,
    first_error: Option<DenormalError>,
}
impl<P> DenormalProcessor<P> {
    pub fn new(processor: P, mode: DenormalMode) -> Result<Self, DenormalConstructionFailure<P>> {
        Self::new_using(processor, mode, || {
            ScopedDenormalMode::enter(mode).map(drop)
        })
    }
    fn new_using(
        processor: P,
        mode: DenormalMode,
        probe: impl FnOnce() -> Result<(), DenormalError>,
    ) -> Result<Self, DenormalConstructionFailure<P>> {
        match probe() {
            Ok(()) => Ok(Self {
                processor,
                mode,
                config: None,
                session: false,
                uncertain: false,
                faulted: false,
                first_error: None,
            }),
            Err(error) => Err(DenormalConstructionFailure { processor, error }),
        }
    }
    pub fn mode(&self) -> DenormalMode {
        self.mode
    }
    pub fn processor(&self) -> &P {
        &self.processor
    }
    pub fn first_native_error(&self) -> Option<DenormalError> {
        self.first_error
    }
    /// Stopped, deactivated extraction. Refusal retains the entire adapter owner.
    // Returning the exact owner on refusal is intentional; boxing would add an
    // allocator requirement and alter ownership. Size includes fixed ProcessConfig.
    #[allow(clippy::result_large_err)]
    pub fn into_inner(self) -> Result<P, Self> {
        if self.config.is_some() || self.session || self.uncertain {
            Err(self)
        } else {
            Ok(self.processor)
        }
    }
}
impl<P> DenormalProcessor<P> {
    fn process_using<S: Sample>(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, S>,
        enter: impl FnOnce() -> Result<ScopedDenormalMode, DenormalError>,
    ) -> ProcessStatus
    where
        P: Processor<S>,
    {
        let validation = match self.config {
            Some(config) => config.validate_block(block),
            None => Err(ProcessError::NotPrepared),
        };
        if let Err(error) = validation {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        if self.faulted || self.uncertain {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::BackendFailure);
        }
        let _scope = match enter() {
            Ok(scope) => scope,
            Err(error) => {
                self.first_error.get_or_insert(error);
                self.faulted = true;
                block.clear_outputs();
                return ProcessStatus::Fault(ProcessError::BackendFailure);
            }
        };
        // Unwinding leaves this fail-closed until explicit reset/reprepare; the
        // local scope still restores control/status during unwinding.
        self.faulted = true;
        let status = self.processor.process(block);
        self.faulted = matches!(status, ProcessStatus::Fault(_));
        if self.faulted {
            block.clear_outputs();
        }
        status
    }
}
impl<S: Sample, P: Processor<S>> Processor<S> for DenormalProcessor<P> {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        if self.session || self.uncertain {
            return Err(PrepareError::AudioSessionActive);
        }
        self.uncertain = true;
        self.config = None;
        let result = self.processor.prepare(config);
        self.uncertain = false;
        if result.is_ok() {
            self.config = Some(config);
            self.first_error = None;
            self.faulted = false;
        }
        result
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        if self.config.is_none() {
            return Err(ProcessError::NotPrepared);
        }
        if self.session || self.uncertain || self.faulted {
            return Err(ProcessError::BackendFailure);
        }
        self.uncertain = true;
        let result = self.processor.start_audio();
        self.uncertain = false;
        self.session = result.is_ok();
        result
    }
    fn stop_audio(&mut self) {
        if self.session || self.uncertain {
            let previously_uncertain = self.uncertain;
            self.uncertain = true;
            self.processor.stop_audio();
            self.session = false;
            // A stop hook cannot prove that a previously panicked preparation
            // left P inactive. Explicit successful deactivation is still required.
            self.uncertain = previously_uncertain;
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        let mode = self.mode;
        self.process_using(block, || ScopedDenormalMode::enter(mode))
    }

    fn reset(&mut self) {
        let _ = self.try_reset();
    }
    fn try_reset(&mut self) -> Result<(), ProcessError> {
        self.faulted = true;
        self.processor.try_reset()?;
        self.faulted = false;
        self.first_error = None;
        Ok(())
    }
    fn deactivate(&mut self) {
        assert!(
            !self.session,
            "stop the audio-owner session before deactivation"
        );
        if self.config.is_some() || self.uncertain {
            self.uncertain = true;
            self.processor.deactivate();
            self.config = None;
            self.uncertain = false;
        }
    }
    fn latency_samples(&self) -> u32 {
        self.processor.latency_samples()
    }
    fn tail(&self) -> Tail {
        self.processor.tail()
    }
}

#[cfg(test)]
mod tests;
