//! Prepared whole-graph handoff with control-thread retirement.
use crate::{Graph, GraphSample, GraphStatus};
use ravet_foundation::{ProcessBlock, ProcessConfig, Tail};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

/// A proposed graph cannot enter the bounded handoff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphOfferFailure {
    /// Queue capacity must be 1..4.
    Capacity,
    /// Graph is inactive/faulted/in an audio session or its config/latency differs.
    Configuration,
    /// Pending queue is full; the caller retains the rejected graph.
    Full,
    /// Audio engine has been dropped.
    EngineGone,
}
/// Rejected graph ownership remains on the control thread.
pub struct GraphOfferError<S: GraphSample = f32> {
    pub reason: GraphOfferFailure,
    pub graph: Box<Graph<S>>,
}
impl<S: GraphSample> std::fmt::Debug for GraphOfferError<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphOfferError")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
/// Result of the block-boundary handoff attempt, separate from graph DSP status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphSwapStatus {
    /// No pending graph.
    Unchanged,
    /// Exactly one FIFO graph replaced the previous graph.
    Applied,
    /// Control must reclaim retired graphs before another replacement.
    RetirementFull,
    /// Control endpoint has been dropped; keep rendering the active graph.
    ControlGone,
    /// Candidate audio start failed and was retired; old processing continues.
    StartRejected(GraphStatus),
    /// Invalid outer block rejected before consuming any proposed graph.
    InvalidBlock,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicGraphStatus {
    pub swap: GraphSwapStatus,
    pub processing: GraphStatus,
}
/// Unique control endpoint. Prepare/offer graphs and reclaim/drop retired graphs
/// outside audio callbacks. Pending and retired queues each have 1..4 slots.
pub struct GraphControl<S: GraphSample = f32> {
    pending: Producer<Box<Graph<S>>>,
    retired: Consumer<Box<Graph<S>>>,
    config: ProcessConfig,
    latency: u32,
}
impl<S: GraphSample> GraphControl<S> {
    /// Accept an already-prepared, nonfaulted graph with exact initial config and
    /// latency. No implicit reprepare, history migration, rate/layout conversion
    /// or host latency change. A rejected graph remains owned by the returned error.
    pub fn offer(&mut self, graph: Box<Graph<S>>) -> Result<(), GraphOfferError<S>> {
        let reason = if self.pending.is_abandoned() {
            Some(GraphOfferFailure::EngineGone)
        } else if graph.config != Some(self.config)
            || graph.faulted
            || graph.audio_owner.is_some()
            || graph.latency != self.latency
        {
            Some(GraphOfferFailure::Configuration)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(GraphOfferError { reason, graph });
        }
        self.pending
            .push(graph)
            .map_err(|PushError::Full(graph)| GraphOfferError {
                reason: GraphOfferFailure::Full,
                graph,
            })
    }
    /// Transfers one retired graph to the control owner for reuse or destruction.
    pub fn take_retired(&mut self) -> Option<Box<Graph<S>>> {
        self.retired.pop().ok()
    }
    /// Destroys at most the bounded retired queue capacity on this control thread.
    pub fn reclaim(&mut self) -> usize {
        let mut count = 0;
        for _ in 0..self.retired.buffer().capacity() {
            let Some(graph) = self.take_retired() else {
                break;
            };
            drop(graph);
            count += 1;
        }
        count
    }
}
/// Audio-owned active graph with one prepared FIFO replacement per valid block.
/// Process/reset never destroy graphs; retired ownership returns to control.
/// Construct and drop both endpoints only outside callbacks. Each graph keeps
/// its own 64 MiB cap; total in-flight graph/node memory is consumer-budgeted.
pub struct DynamicGraph<S: GraphSample = f32> {
    current: Box<Graph<S>>,
    pending: Consumer<Box<Graph<S>>>,
    retired: Producer<Box<Graph<S>>>,
    deferred_retirement: Option<Box<Graph<S>>>,
    config: ProcessConfig,
    audio_active: bool,
}
impl<S: GraphSample> DynamicGraph<S> {
    /// Allocates two bounded ownership queues around a prepared active graph.
    /// Returns initial graph ownership if preparation/capacity validation fails.
    pub fn new(
        initial: Box<Graph<S>>,
        capacity: usize,
    ) -> Result<(GraphControl<S>, Self), GraphOfferError<S>> {
        if !(1..=4).contains(&capacity) {
            return Err(GraphOfferError {
                reason: GraphOfferFailure::Capacity,
                graph: initial,
            });
        }
        let Some(config) = initial
            .config
            .filter(|_| !initial.faulted && initial.audio_owner.is_none())
        else {
            return Err(GraphOfferError {
                reason: GraphOfferFailure::Configuration,
                graph: initial,
            });
        };
        let latency = initial.latency;
        let (pending_tx, pending_rx) = RingBuffer::new(capacity);
        let (retired_tx, retired_rx) = RingBuffer::new(capacity);
        Ok((
            GraphControl {
                pending: pending_tx,
                retired: retired_rx,
                config,
                latency,
            },
            Self {
                current: initial,
                pending: pending_rx,
                retired: retired_tx,
                deferred_retirement: None,
                config,
                audio_active: false,
            },
        ))
    }
    /// Fixed prepared configuration shared by all accepted graphs. Reading it
    /// does not prepare/restart nodes or alter handoff/retirement ownership.
    pub fn configuration(&self) -> ProcessConfig {
        self.config
    }
    /// Enter explicit owner-confined node start/stop lifecycle. While guarded,
    /// candidates start before old nodes stop; failed candidates retire while
    /// old rendering continues. Normal unguarded portable behavior is unchanged.
    pub fn start_audio(&mut self) -> Result<DynamicGraphAudioSession<'_, S>, GraphStatus> {
        self.current.start_audio_nodes()?;
        self.audio_active = true;
        Ok(DynamicGraphAudioSession {
            graph: self,
            owner: std::marker::PhantomData,
        })
    }
    fn apply_pending(&mut self) -> GraphSwapStatus {
        if self.retired.is_abandoned() {
            return GraphSwapStatus::ControlGone;
        }
        if self.deferred_retirement.is_some() {
            if self.retired.is_full() {
                return GraphSwapStatus::RetirementFull;
            }
            let graph = self.deferred_retirement.take().unwrap();
            if let Err(PushError::Full(graph)) = self.retired.push(graph) {
                self.deferred_retirement = Some(graph);
                return GraphSwapStatus::RetirementFull;
            }
        }
        if self.pending.is_empty() {
            return GraphSwapStatus::Unchanged;
        }
        if self.retired.is_full() {
            return GraphSwapStatus::RetirementFull;
        }
        let Ok(mut next) = self.pending.pop() else {
            return GraphSwapStatus::Unchanged;
        };
        if self.audio_active {
            if let Err(error) = next.start_audio_nodes() {
                assert!(
                    self.retired.push(next).is_ok(),
                    "checked SPSC retirement slot"
                );
                return GraphSwapStatus::StartRejected(error);
            }
            self.current.stop_audio_nodes();
        }
        let previous = std::mem::replace(&mut self.current, next);
        // SPSC: a checked free producer slot cannot be filled by the consumer.
        // Retain ownership even on an unexpected refusal; never destroy on audio.
        if let Err(PushError::Full(graph)) = self.retired.push(previous) {
            self.deferred_retirement = Some(graph);
        }
        GraphSwapStatus::Applied
    }
    /// Validate outer input, replace at most one graph, then render. Replacement
    /// starts the new prepared history; there is no crossfade, old-tail drain or
    /// rollback. Invalid blocks silence output without consuming a queued graph.
    pub fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, S>) -> DynamicGraphStatus {
        if let Err(error) = self.config.validate_block(block) {
            block.clear_outputs();
            return DynamicGraphStatus {
                swap: GraphSwapStatus::InvalidBlock,
                processing: GraphStatus::Fault { node: None, error },
            };
        }
        let swap = self.apply_pending();
        DynamicGraphStatus {
            swap,
            processing: self.current.process(block),
        }
    }
    /// Reset only the currently selected graph; proposed graphs retain their
    /// already-prepared histories until selected. No graph destruction occurs.
    pub fn reset(&mut self) {
        let _ = self.try_reset();
    }
    /// Reset only the selected graph; proposals retain their prepared history.
    pub fn try_reset(&mut self) -> Result<(), GraphStatus> {
        self.current.try_reset()
    }
    /// Fixed latency enforced for every accepted graph.
    pub fn latency_samples(&self) -> u32 {
        self.current.latency_samples()
    }
    /// Selected graph's tail; replacement does not drain the previous tail.
    pub fn tail(&self) -> Tail {
        self.current.tail()
    }
}

/// Owner-confined dynamic graph session. Stop/drop before engine transfer/drop;
/// do not forget/leak this guard. Retirement remains control-owned.
///
/// ```compile_fail
/// use ravet_graph::DynamicGraphAudioSession;
/// fn require_send<T: Send>() {}
/// require_send::<DynamicGraphAudioSession<'static>>();
/// ```
pub struct DynamicGraphAudioSession<'a, S: GraphSample = f32> {
    graph: &'a mut DynamicGraph<S>,
    owner: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl<S: GraphSample> DynamicGraphAudioSession<'_, S> {
    pub fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, S>) -> DynamicGraphStatus {
        self.graph.process(block)
    }
    pub fn reset(&mut self) {
        let _ = self.try_reset();
    }
    /// Forward checked reset without resetting pending proposals.
    pub fn try_reset(&mut self) -> Result<(), GraphStatus> {
        self.graph.try_reset()
    }
    pub fn latency_samples(&self) -> u32 {
        self.graph.latency_samples()
    }
    pub fn tail(&self) -> Tail {
        self.graph.tail()
    }
    pub fn finish(self) {}
}
impl<S: GraphSample> Drop for DynamicGraphAudioSession<'_, S> {
    fn drop(&mut self) {
        self.graph.current.stop_audio_nodes();
        self.graph.audio_active = false;
    }
}
