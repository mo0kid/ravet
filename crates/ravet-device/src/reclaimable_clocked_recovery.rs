//! Caller-scheduled recovery waits old paired callback proof before restoration.
use crate::{
    ClockedDuplexOptions, ClockedDuplexRetirement, ClockedRetirementError, FailedClockedOwner,
    ReclaimableClockedDuplex, ReclaimableClockedFailure, ReclaimableClockedOpenError,
    ReclaimableClockedStatistics, ReclaimedClockedDuplex, input_device,
    open_reclaimable_clocked_duplex, output_device,
};
use ravet_foundation::Processor;

/// Explicit control state; disabled retirement still polls to retain original owners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReclaimableClockedRecoveryStatus {
    Owned,
    Running,
    Retiring,
    Disabled {
        retiring: bool,
    },
    Opened {
        generation: u64,
    },
    Reclaimed {
        worker_fault: Option<ClockedRetirementError>,
    },
    Lost,
}
/// Exact stopped observations passed once to caller-selected restoration policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockedRecoveryReceipt {
    pub statistics: ReclaimableClockedStatistics,
    pub worker_fault: Option<ClockedRetirementError>,
}
#[derive(Debug)]
pub enum ReclaimableClockedRecoveryError {
    InvalidSelection,
    Open(ReclaimableClockedOpenError),
    Start(cpal::Error),
    Retirement(ClockedRetirementError),
    Lost,
}
impl std::fmt::Display for ReclaimableClockedRecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "clocked recovery: {self:?}")
    }
}
impl std::error::Error for ReclaimableClockedRecoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open(e) => Some(e),
            Self::Start(e) => Some(e),
            Self::Retirement(e) => Some(e),
            _ => None,
        }
    }
}
/// Restoration failures preserve the original processor and receipt for retry.
#[derive(Debug)]
pub enum ClockedRecoveryFailure<E> {
    Control(ReclaimableClockedRecoveryError),
    Restore(E),
}
impl<E: std::fmt::Display + std::fmt::Debug> std::fmt::Display for ClockedRecoveryFailure<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Control(e) => e.fmt(f),
            Self::Restore(e) => std::fmt::Display::fmt(e, f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for ClockedRecoveryFailure<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Control(e) => e,
            Self::Restore(e) => e,
        })
    }
}
trait Pair<P: Processor> {
    fn statistics(&self) -> ReclaimableClockedStatistics;
    fn play(&self) -> Result<(), cpal::Error>;
    fn stop(self) -> ClockedDuplexRetirement<P>;
}
impl<P: Processor> Pair<P> for ReclaimableClockedDuplex<P> {
    fn statistics(&self) -> ReclaimableClockedStatistics {
        self.statistics()
    }
    fn play(&self) -> Result<(), cpal::Error> {
        self.play()
    }
    fn stop(self) -> ClockedDuplexRetirement<P> {
        self.stop()
    }
}
enum State<P: Processor, S: Pair<P>> {
    Owned {
        processor: P,
        previous: Option<ClockedRecoveryReceipt>,
    },
    Live(S),
    Retiring(ClockedDuplexRetirement<P>),
    Disabled {
        processor: Option<P>,
        retirement: Option<ClockedDuplexRetirement<P>>,
        previous: Option<ClockedRecoveryReceipt>,
    },
}
struct Coordinator<P: Processor, S: Pair<P>> {
    state: Option<State<P, S>>,
    generation: u64,
}
impl<P: Processor, S: Pair<P>> Coordinator<P, S> {
    fn new(processor: P) -> Self {
        Self {
            state: Some(State::Owned {
                processor,
                previous: None,
            }),
            generation: 0,
        }
    }
    fn status(&self) -> ReclaimableClockedRecoveryStatus {
        match &self.state {
            Some(State::Owned { .. }) => ReclaimableClockedRecoveryStatus::Owned,
            Some(State::Live(_)) => ReclaimableClockedRecoveryStatus::Running,
            Some(State::Retiring(_)) => ReclaimableClockedRecoveryStatus::Retiring,
            Some(State::Disabled { retirement, .. }) => {
                ReclaimableClockedRecoveryStatus::Disabled {
                    retiring: retirement.is_some(),
                }
            }
            None => ReclaimableClockedRecoveryStatus::Lost,
        }
    }
    fn set_enabled(&mut self, enabled: bool) {
        let Some(state) = self.state.take() else {
            return;
        };
        self.state = Some(match (enabled, state) {
            (
                false,
                State::Owned {
                    processor,
                    previous,
                },
            ) => State::Disabled {
                processor: Some(processor),
                retirement: None,
                previous,
            },
            (false, State::Live(stream)) => State::Disabled {
                processor: None,
                retirement: Some(stream.stop()),
                previous: None,
            },
            (false, State::Retiring(retirement)) => State::Disabled {
                processor: None,
                retirement: Some(retirement),
                previous: None,
            },
            (
                true,
                State::Disabled {
                    processor: Some(processor),
                    retirement: None,
                    previous,
                },
            ) => State::Owned {
                processor,
                previous,
            },
            (
                true,
                State::Disabled {
                    processor: None,
                    retirement: Some(retirement),
                    ..
                },
            ) => State::Retiring(retirement),
            (_, state) => state,
        });
    }
    fn request_reopen(&mut self) {
        if matches!(&self.state, Some(State::Live(_))) {
            let Some(State::Live(stream)) = self.state.take() else {
                unreachable!()
            };
            self.state = Some(State::Retiring(stream.stop()));
        }
    }
    fn statistics(&self) -> Option<ReclaimableClockedStatistics> {
        match &self.state {
            Some(State::Live(stream)) => Some(stream.statistics()),
            Some(State::Owned {
                previous: Some(receipt),
                ..
            })
            | Some(State::Disabled {
                previous: Some(receipt),
                ..
            }) => Some(receipt.statistics),
            _ => None,
        }
    }
    fn service_selected<E, T>(
        &mut self,
        select: impl FnOnce() -> Result<T, ReclaimableClockedRecoveryError>,
        available: impl FnOnce(&T) -> bool,
        open: impl FnOnce(T, P) -> Result<S, ReclaimableClockedFailure<P>>,
        restore: impl FnOnce(&mut P, ClockedRecoveryReceipt) -> Result<(), E>,
    ) -> Result<ReclaimableClockedRecoveryStatus, ClockedRecoveryFailure<E>> {
        if let Some(State::Live(stream)) = &self.state {
            let statistics = stream.statistics();
            if unhealthy(statistics) {
                self.request_reopen();
                return Ok(self.status());
            }
        }
        if !matches!(
            self.status(),
            ReclaimableClockedRecoveryStatus::Owned | ReclaimableClockedRecoveryStatus::Running
        ) {
            return self.service(
                false,
                |_| unreachable!("no open while pending or disabled"),
                restore,
            );
        }
        let selection = select().map_err(ClockedRecoveryFailure::Control)?;
        let available = available(&selection);
        self.service(available, |processor| open(selection, processor), restore)
    }
    fn service<E>(
        &mut self,
        available: bool,
        open: impl FnOnce(P) -> Result<S, ReclaimableClockedFailure<P>>,
        restore: impl FnOnce(&mut P, ClockedRecoveryReceipt) -> Result<(), E>,
    ) -> Result<ReclaimableClockedRecoveryStatus, ClockedRecoveryFailure<E>> {
        let Some(state) = self.state.take() else {
            return Err(ClockedRecoveryFailure::Control(
                ReclaimableClockedRecoveryError::Lost,
            ));
        };
        match state {
            State::Owned {
                mut processor,
                previous,
            } => {
                if !available {
                    self.state = Some(State::Owned {
                        processor,
                        previous,
                    });
                    return Ok(self.status());
                }
                if let Some(receipt) = previous
                    && let Err(error) = restore(&mut processor, receipt)
                {
                    self.state = Some(State::Owned {
                        processor,
                        previous,
                    });
                    return Err(ClockedRecoveryFailure::Restore(error));
                }
                match open(processor) {
                    Ok(stream) => {
                        if let Err(error) = stream.play() {
                            self.state = Some(State::Retiring(stream.stop()));
                            return Err(ClockedRecoveryFailure::Control(
                                ReclaimableClockedRecoveryError::Start(error),
                            ));
                        }
                        self.generation = self.generation.wrapping_add(1);
                        self.state = Some(State::Live(stream));
                        Ok(ReclaimableClockedRecoveryStatus::Opened {
                            generation: self.generation,
                        })
                    }
                    Err(failure) => {
                        let (error, owner) = failure.into_parts();
                        self.state = Some(match owner {
                            FailedClockedOwner::Immediate(processor) => State::Owned {
                                processor,
                                previous: None,
                            },
                            FailedClockedOwner::Pending(retirement) => State::Retiring(retirement),
                        });
                        Err(ClockedRecoveryFailure::Control(
                            ReclaimableClockedRecoveryError::Open(error),
                        ))
                    }
                }
            }
            State::Live(stream) => {
                let statistics = stream.statistics();
                if !available || unhealthy(statistics) {
                    self.state = Some(State::Retiring(stream.stop()));
                } else {
                    self.state = Some(State::Live(stream));
                }
                Ok(self.status())
            }
            State::Retiring(mut retirement) => match retirement.try_reclaim() {
                Ok(None) => {
                    self.state = Some(State::Retiring(retirement));
                    Ok(self.status())
                }
                Ok(Some(outcome)) => {
                    let (processor, receipt) = recovered(outcome);
                    self.state = Some(State::Owned {
                        processor,
                        previous: Some(receipt),
                    });
                    Ok(ReclaimableClockedRecoveryStatus::Reclaimed {
                        worker_fault: receipt.worker_fault,
                    })
                }
                Err(error) => Err(ClockedRecoveryFailure::Control(
                    ReclaimableClockedRecoveryError::Retirement(error),
                )),
            },
            State::Disabled {
                processor,
                mut retirement,
                previous,
            } => {
                if let Some(proof) = retirement.as_mut() {
                    match proof.try_reclaim() {
                        Ok(None) => {}
                        Ok(Some(outcome)) => {
                            let (processor, receipt) = recovered(outcome);
                            self.state = Some(State::Disabled {
                                processor: Some(processor),
                                retirement: None,
                                previous: Some(receipt),
                            });
                            return Ok(self.status());
                        }
                        Err(error) => {
                            return Err(ClockedRecoveryFailure::Control(
                                ReclaimableClockedRecoveryError::Retirement(error),
                            ));
                        }
                    }
                }
                self.state = Some(State::Disabled {
                    processor,
                    retirement,
                    previous,
                });
                Ok(self.status())
            }
        }
    }
    fn into_owner(mut self) -> Result<FailedClockedOwner<P>, ReclaimableClockedRecoveryError> {
        match self.state.take() {
            Some(State::Owned { processor, .. })
            | Some(State::Disabled {
                processor: Some(processor),
                retirement: None,
                ..
            }) => Ok(FailedClockedOwner::Immediate(processor)),
            Some(State::Live(stream)) => Ok(FailedClockedOwner::Pending(stream.stop())),
            Some(State::Retiring(retirement))
            | Some(State::Disabled {
                processor: None,
                retirement: Some(retirement),
                ..
            }) => Ok(FailedClockedOwner::Pending(retirement)),
            _ => Err(ReclaimableClockedRecoveryError::Lost),
        }
    }
}
// CPAL xruns and automatic route changes leave the pair live.
fn unhealthy(statistics: ReclaimableClockedStatistics) -> bool {
    statistics.input.backend_errors != 0
        || statistics.output.backend_errors != 0
        || statistics.output.panics != 0
        || statistics.capture_panics != 0
}
fn recovered<P: Processor>(outcome: ReclaimedClockedDuplex<P>) -> (P, ClockedRecoveryReceipt) {
    match outcome {
        ReclaimedClockedDuplex::Reclaimed { owner, statistics } => (
            owner.into_processor(),
            ClockedRecoveryReceipt {
                statistics,
                worker_fault: None,
            },
        ),
        ReclaimedClockedDuplex::WorkerFault {
            owner,
            statistics,
            error,
        } => (
            owner.into_processor(),
            ClockedRecoveryReceipt {
                statistics,
                worker_fault: Some(error),
            },
        ),
    }
}
/// Explicit selected-ID recovery with original processor ownership. Control service
/// never invokes restoration or opens while old callbacks are pending. No timer,
/// fallback, automatic state migration or physical hotplug guarantee. Restoration
/// runs only after a receipt and before fresh preparation; its error preserves P.
/// CPAL xruns and automatic route changes are counted without retiring the pair;
/// backend failures and capture/output panics remain recovery faults.
/// Foreign restoration/deactivation panic loses recovery, never a false receipt.
///
/// ```
/// use ravet_device::{RecoveringReclaimableClockedDuplex,
///     ReclaimableClockedRecoveryStatus, ClockedRecoveryFailure};
/// use ravet_foundation::Processor;
/// fn service<P: Processor + Send + 'static>(session:
///     &mut RecoveringReclaimableClockedDuplex<P>)
///     -> Result<ReclaimableClockedRecoveryStatus,
///         ClockedRecoveryFailure<std::convert::Infallible>>
/// {
///     session.service(|processor, _receipt| {
///         // Caller-selected policy, invoked only after old callbacks retire.
///         processor.reset();
///         Ok(())
///     })
/// }
/// ```
pub struct RecoveringReclaimableClockedDuplex<P: Processor + Send + 'static> {
    input_id: String,
    output_id: String,
    input_config: cpal::StreamConfig,
    output_config: cpal::StreamConfig,
    ring_frames: usize,
    options: ClockedDuplexOptions,
    coordinator: Coordinator<P, ReclaimableClockedDuplex<P>>,
}
/// Invalid construction retains the original processor, before native calls.
pub struct ReclaimableClockedRecoveryConstructionFailure<P> {
    processor: P,
    error: ReclaimableClockedRecoveryError,
}
impl<P> ReclaimableClockedRecoveryConstructionFailure<P> {
    pub fn into_parts(self) -> (P, ReclaimableClockedRecoveryError) {
        (self.processor, self.error)
    }
    pub fn error(&self) -> &ReclaimableClockedRecoveryError {
        &self.error
    }
}
impl<P> std::fmt::Debug for ReclaimableClockedRecoveryConstructionFailure<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReclaimableClockedRecoveryConstructionFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl<P: Processor + Send + 'static> RecoveringReclaimableClockedDuplex<P> {
    pub fn new(
        input_id: impl Into<String>,
        output_id: impl Into<String>,
        input_config: cpal::StreamConfig,
        output_config: cpal::StreamConfig,
        ring_frames: usize,
        processor: P,
        options: ClockedDuplexOptions,
    ) -> Result<Self, ReclaimableClockedRecoveryConstructionFailure<P>> {
        let input_id = input_id.into();
        let output_id = output_id.into();
        let validation = if [&input_id, &output_id]
            .into_iter()
            .any(|id| id.is_empty() || id.len() > 4096 || id.contains('\0'))
        {
            Err(ReclaimableClockedRecoveryError::InvalidSelection)
        } else {
            crate::clocked::validate_configuration(
                input_config.sample_rate,
                output_config.sample_rate,
                usize::from(input_config.channels),
                usize::from(output_config.channels),
                ring_frames,
                options,
            )
            .and({
                if matches!(input_config.buffer_size, cpal::BufferSize::Fixed(0))
                    || matches!(output_config.buffer_size, cpal::BufferSize::Fixed(0))
                {
                    Err(crate::ClockedDuplexError::InvalidConfiguration)
                } else {
                    Ok(())
                }
            })
            .map_err(|e| {
                ReclaimableClockedRecoveryError::Open(
                    ReclaimableClockedOpenError::ClockedPreparation(e),
                )
            })
        };
        if let Err(error) = validation {
            return Err(ReclaimableClockedRecoveryConstructionFailure { processor, error });
        }
        Ok(Self {
            input_id,
            output_id,
            input_config,
            output_config,
            ring_frames,
            options,
            coordinator: Coordinator::new(processor),
        })
    }
    pub fn status(&self) -> ReclaimableClockedRecoveryStatus {
        self.coordinator.status()
    }
    /// Stop a healthy live pair, entering asynchronous proof before explicit
    /// restoration/reprepare. Disabled/owned/pending states remain unchanged.
    pub fn request_reopen(&mut self) {
        self.coordinator.request_reopen();
    }
    pub fn statistics(&self) -> Option<ReclaimableClockedStatistics> {
        self.coordinator.statistics()
    }
    pub fn generation(&self) -> u64 {
        self.coordinator.generation
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.coordinator.set_enabled(enabled);
    }
    /// Caller-scheduled control work. Known live callback faults retire before
    /// device enumeration. Healthy enumeration errors retain the pair and return
    /// the original error; they do not infer device absence. Pending/disabled
    /// states only poll proof, with no enumeration, opening or restoration.
    pub fn service<E>(
        &mut self,
        restore: impl FnOnce(&mut P, ClockedRecoveryReceipt) -> Result<(), E>,
    ) -> Result<ReclaimableClockedRecoveryStatus, ClockedRecoveryFailure<E>> {
        self.coordinator.service_selected(
            || {
                let input = input_device(Some(&self.input_id)).map_err(|e| {
                    ReclaimableClockedRecoveryError::Open(
                        ReclaimableClockedOpenError::InputBackend(e),
                    )
                })?;
                let output = output_device(Some(&self.output_id)).map_err(|e| {
                    ReclaimableClockedRecoveryError::Open(
                        ReclaimableClockedOpenError::OutputBackend(e),
                    )
                })?;
                Ok((input, output))
            },
            |(input, output)| input.is_some() && output.is_some(),
            |(input, output), processor| {
                open_reclaimable_clocked_duplex(
                    input.as_ref().expect("available input"),
                    output.as_ref().expect("available output"),
                    self.input_config,
                    self.output_config,
                    self.ring_frames,
                    processor,
                    self.options,
                )
            },
            restore,
        )
    }
    /// Return original owner or pending proof; never extract/seek while live/pending.
    pub fn into_owner(self) -> Result<FailedClockedOwner<P>, ReclaimableClockedRecoveryError> {
        self.coordinator.into_owner()
    }
}

#[cfg(test)]
#[path = "reclaimable_clocked_recovery_tests.rs"]
mod tests;
