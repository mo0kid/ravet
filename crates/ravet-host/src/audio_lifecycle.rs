//! Bounded activated-processor handoff with audio-owner start/stop and retirement.
use clack_host::{
    host::HostHandlers,
    process::{StartedPluginAudioProcessor, StoppedPluginAudioProcessor},
};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::{marker::PhantomData, rc::Rc};

/// Consumer-defined identity plus a main-thread-activated, stopped processor.
/// The consumer retains its matching PluginInstance on the main thread until
/// this processor returns for instance.deactivate(). IDs/configuration/latency
/// matching are consumer policy; this handoff does not inspect ports or geometry.
pub struct HostedAudioItem<H: HostHandlers> {
    pub tag: u64,
    pub processor: StoppedPluginAudioProcessor<H>,
}
impl<H: HostHandlers> std::fmt::Debug for HostedAudioItem<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostedAudioItem")
            .field("tag", &self.tag)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedAudioOfferFailure {
    Full,
    EngineGone,
}
/// Rejection retains the activated/stopped processor on the control owner.
pub struct HostedAudioOfferError<H: HostHandlers> {
    pub reason: HostedAudioOfferFailure,
    pub item: HostedAudioItem<H>,
}
impl<H: HostHandlers> std::fmt::Debug for HostedAudioOfferError<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostedAudioOfferError")
            .field("reason", &self.reason)
            .field("item", &self.item)
            .finish()
    }
}
impl<H: HostHandlers> std::fmt::Display for HostedAudioOfferError<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "hosted audio offer rejected: {:?}", self.reason)
    }
}
impl<H: HostHandlers> std::error::Error for HostedAudioOfferError<H> {}
/// Unique main/control endpoint. Activate instances before offering, and
/// deactivate each matching instance after taking its stopped retirement.
/// Construction/offer/reclaim/drop are outside callbacks. Do not destroy main
/// instances while queued, running or retired audio processors retain them.
pub struct HostedAudioControl<H: HostHandlers> {
    pending: Producer<HostedAudioItem<H>>,
    retired: Consumer<HostedAudioItem<H>>,
}
impl<H: HostHandlers> HostedAudioControl<H> {
    pub fn offer(&mut self, item: HostedAudioItem<H>) -> Result<(), HostedAudioOfferError<H>> {
        if self.pending.is_abandoned() {
            return Err(HostedAudioOfferError {
                reason: HostedAudioOfferFailure::EngineGone,
                item,
            });
        }
        self.pending
            .push(item)
            .map_err(|PushError::Full(item)| HostedAudioOfferError {
                reason: HostedAudioOfferFailure::Full,
                item,
            })
    }
    pub fn take_retired(&mut self) -> Option<HostedAudioItem<H>> {
        self.retired.pop().ok()
    }
}
struct AudioQueues<H: HostHandlers> {
    pending: Consumer<HostedAudioItem<H>>,
    retired: Producer<HostedAudioItem<H>>,
}
/// Transferable stopped engine. Move to the audio owner, then start there.
/// Finish returns this state for main-thread deactivation; pending items can
/// be taken without starting. Drop only outside callbacks, retaining matching
/// main-thread instances until all activated tokens are reclaimed/dropped.
pub struct HostedAudioReady<H: HostHandlers> {
    current: HostedAudioItem<H>,
    queues: AudioQueues<H>,
}
/// Initial start failed; complete stopped engine ownership is recoverable.
pub struct HostedAudioStartError<H: HostHandlers> {
    pub ready: HostedAudioReady<H>,
}
impl<H: HostHandlers> std::fmt::Debug for HostedAudioStartError<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostedAudioStartError")
            .field("tag", &self.ready.current.tag)
            .finish_non_exhaustive()
    }
}
impl<H: HostHandlers> std::fmt::Display for HostedAudioStartError<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "hosted processor {} rejected initial start",
            self.ready.current.tag
        )
    }
}
impl<H: HostHandlers> std::error::Error for HostedAudioStartError<H> {}
impl<H: HostHandlers> HostedAudioReady<H> {
    /// CLAP start runs on this thread. Running engine cannot move to another
    /// thread; it must finish here before returning stopped tokens to control.
    pub fn start(self) -> Result<HostedAudioRunning<H>, HostedAudioStartError<H>> {
        let Self {
            current: HostedAudioItem { tag, processor },
            queues,
        } = self;
        match processor.start_processing() {
            Ok(processor) => Ok(HostedAudioRunning {
                current: Some((tag, processor)),
                queues: Some(queues),
                owner: PhantomData,
            }),
            Err(error) => Err(HostedAudioStartError {
                ready: Self {
                    current: HostedAudioItem {
                        tag,
                        processor: error.into_stopped_processor(),
                    },
                    queues,
                },
            }),
        }
    }
    /// Main-thread shutdown: drain every unstarted proposal and deactivate its
    /// matching instance. Do not use concurrently with audio processing.
    pub fn take_pending(&mut self) -> Option<HostedAudioItem<H>> {
        self.queues.pending.pop().ok()
    }
    /// Stop control offers before shutdown, then drain pending proposals. Extract
    /// the current stopped token only after the pending queue is empty. Otherwise
    /// retain complete ownership. Retired items remain on the control endpoint.
    pub fn into_current(self) -> Result<HostedAudioItem<H>, Self> {
        if self.queues.pending.is_empty() {
            Ok(self.current)
        } else {
            Err(self)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedAudioSwap {
    Unchanged,
    Applied { previous: u64, current: u64 },
    StartRejected { tag: u64 },
    RetirementFull,
    ControlGone,
}
/// Audio-owner engine. The !Send/!Sync marker keeps native start/process/stop
/// on its starting thread. Normal swaps allocate/drop no framework storage.
/// Caller supplies prepared Clack audio/events/transport to process(). This
/// is lifecycle ownership, not graph routing, device scheduling, restart request
/// service, port/config matching, crash isolation or a native deadline guarantee.
/// finish/drop occur outside callbacks; plugin callbacks keep their own contract.
///
/// ```compile_fail
/// use ravet_host::{HostedAudioRunning, RequestHost};
/// fn require_send<T: Send>() {}
/// require_send::<HostedAudioRunning<RequestHost>>();
/// ```
pub struct HostedAudioRunning<H: HostHandlers> {
    current: Option<(u64, StartedPluginAudioProcessor<H>)>,
    queues: Option<AudioQueues<H>>,
    owner: PhantomData<Rc<()>>,
}
impl<H: HostHandlers> HostedAudioRunning<H> {
    pub fn tag(&self) -> u64 {
        self.current.as_ref().expect("running token").0
    }
    /// Process caller-prepared native buffers/events on this audio owner. Caller
    /// validates the activated port/config/frame/event contract before applying
    /// pending swaps; no implicit conversion, silence, sleep or fault recovery.
    pub fn process(
        &mut self,
        inputs: &clack_host::prelude::InputAudioBuffers,
        outputs: &mut clack_host::prelude::OutputAudioBuffers,
        input_events: &clack_host::prelude::InputEvents,
        output_events: &mut clack_host::prelude::OutputEvents,
        steady_time: Option<u64>,
        transport: Option<&clack_host::events::event_types::TransportEvent>,
    ) -> Result<clack_host::process::ProcessStatus, clack_host::plugin::PluginInstanceError> {
        self.current.as_mut().expect("running token").1.process(
            inputs,
            outputs,
            input_events,
            output_events,
            steady_time,
            transport,
        )
    }
    /// Reset native processing on the audio owner; caller resets its steady-time
    /// mapping explicitly. This does not change current identity or queues.
    pub fn reset(&mut self) {
        self.current.as_mut().expect("running token").1.reset();
    }
    /// One FIFO replacement at a consumer-selected valid block boundary. Start
    /// the candidate before stopping the old processor. Rejected starts retire
    /// the stopped candidate and retain old processing/history. Full retirement
    /// blocks dequeue/start/stop. No old-tail drain, migration or crossfade.
    pub fn apply_pending(&mut self) -> HostedAudioSwap {
        let queues = self.queues.as_mut().expect("running queues");
        if queues.retired.is_abandoned() {
            return HostedAudioSwap::ControlGone;
        }
        if queues.pending.is_empty() {
            return HostedAudioSwap::Unchanged;
        }
        if queues.retired.is_full() {
            return HostedAudioSwap::RetirementFull;
        }
        let HostedAudioItem { tag, processor } = queues.pending.pop().expect("queued item");
        let (retired, status) = match processor.start_processing() {
            Ok(processor) => {
                let (previous, old) = self
                    .current
                    .replace((tag, processor))
                    .expect("running token");
                (
                    HostedAudioItem {
                        tag: previous,
                        processor: old.stop_processing(),
                    },
                    HostedAudioSwap::Applied {
                        previous,
                        current: tag,
                    },
                )
            }
            Err(error) => (
                HostedAudioItem {
                    tag,
                    processor: error.into_stopped_processor(),
                },
                HostedAudioSwap::StartRejected { tag },
            ),
        };
        // SPSC: the consumer can only free producer slots after this check.
        // No other producer can fill the reserved retirement capacity.
        assert!(
            queues.retired.push(retired).is_ok(),
            "checked SPSC retirement slot"
        );
        status
    }
    /// Stop on the audio owner outside callbacks and transfer complete stopped
    /// state back to control. Main-thread instance deactivation is separate.
    pub fn finish(mut self) -> HostedAudioReady<H> {
        let (tag, processor) = self.current.take().expect("running token");
        HostedAudioReady {
            current: HostedAudioItem {
                tag,
                processor: processor.stop_processing(),
            },
            queues: self.queues.take().expect("running queues"),
        }
    }
}
impl<H: HostHandlers> Drop for HostedAudioRunning<H> {
    fn drop(&mut self) {
        // !Send makes ordinary Rust destruction stay on the native audio owner.
        // Explicit finish is preferred to return matching deactivation tokens.
        if let Some((_, processor)) = self.current.take() {
            drop(processor.stop_processing());
        }
    }
}
/// Allocate bounded 1..4-slot proposal/retirement queues outside callbacks.
/// Invalid capacity returns initial activated/stopped ownership unchanged.
pub fn hosted_audio_handoff<H: HostHandlers>(
    initial: HostedAudioItem<H>,
    capacity: usize,
) -> Result<(HostedAudioControl<H>, HostedAudioReady<H>), HostedAudioItem<H>> {
    if !(1..=4).contains(&capacity) {
        return Err(initial);
    }
    let (pending_tx, pending_rx) = RingBuffer::new(capacity);
    let (retired_tx, retired_rx) = RingBuffer::new(capacity);
    Ok((
        HostedAudioControl {
            pending: pending_tx,
            retired: retired_rx,
        },
        HostedAudioReady {
            current: initial,
            queues: AudioQueues {
                pending: pending_rx,
                retired: retired_tx,
            },
        },
    ))
}
