//! Explicit buffered f32 graph-worker output; no device clock or worker is inferred.
use ravet_audio_file::{AudioConsumer, AudioProducer};
use ravet_foundation::{
    AudioBuffer, Event, MAXIMUM_CHANNELS, OutputEvents, PrepareError, ProcessBlock, ProcessConfig,
    ProcessError, ProcessStatus, Processor, Transport,
};
use ravet_graph::{
    DynamicGraph, DynamicGraphAudioSession, Graph, GraphAudioSession, GraphStatus, GraphSwapStatus,
    MAXIMUM_GRAPH_EVENTS,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphOutputError {
    Configuration,
    Preparation(PrepareError),
    Start(GraphStatus),
}
impl std::fmt::Display for GraphOutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "graph output error: {self:?}")
    }
}
impl std::error::Error for GraphOutputError {}
/// Progress distinguishes a newly rendered block from retained backpressure.
/// Published frames are a queue prefix, not hardware playback or latency evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphOutputProgress {
    pub rendered_frames: usize,
    pub published_frames: usize,
    pub pending_frames: usize,
    /// True only when this call rendered a fresh block and accepted its input.
    /// False while draining a prior suffix: retain input for another pump.
    /// Graph faults return Err and may have nonrollback input/history effects.
    pub input_events_consumed: bool,
    /// Valid output prefix in the caller's storage, emitted once on a fresh render.
    /// Zero during retained audio publication. Offsets refer to the rendered block,
    /// without implying a device-clock or audible-delivery relationship.
    pub output_event_count: usize,
    /// Queued source frames consumed only on this fresh render. Zero for no-input
    /// profiles or pending publication; this is not a capture/device clock.
    pub input_frames_consumed: usize,
    /// Missing queued input frames zero-filled for this fresh render.
    pub input_underflow_frames: usize,
    /// Dynamic handoff outcome only on a fresh render; None for fixed graphs or
    /// pending audio publication. Swap success is separate from graph processing.
    pub swap: Option<GraphSwapStatus>,
}
/// Control-prepared f32 graph and worker scratch; new uses no input while
/// with_input consumes an explicitly prepared input queue. Move this to an
/// explicitly owned worker; start its non-Send session there and finish before
/// returning it to main for graph/node retirement. No thread/device is created.
/// Config requires matching input/output queue channels, zero aux buses,
/// 0..1024 events per direction, 1..65536 frames,
/// and producer-matching 1..64 channels. Scratch uses at most 32 MiB outside the
/// graph's own memory cap. Preparation consumes a fresh graph, preparing it once.
/// Queue backpressure retains exact whole-frame suffixes without rerendering.
pub struct PreparedGraphOutput {
    graph: Box<Graph>,
    io: WorkerIo,
}
struct WorkerIo {
    consumer: Option<AudioConsumer>,
    input_planar: Vec<Vec<f32>>,
    input_interleaved: Vec<f32>,
    fault: Option<GraphStatus>,
    last_swap: Option<GraphSwapStatus>,
    producer: AudioProducer,
    config: ProcessConfig,
    planar: Vec<Vec<f32>>,
    interleaved: Vec<f32>,
    pending: std::ops::Range<usize>,
}
impl PreparedGraphOutput {
    pub fn new(
        graph: Box<Graph>,
        producer: AudioProducer,
        config: ProcessConfig,
    ) -> Result<Self, GraphOutputError> {
        Self::prepare(graph, None, producer, config)
    }
    /// Prepare queued f32 input and output with matching explicit graph/ring
    /// channels and rate policy. Input underflow fills zero; only fresh renders
    /// consume input, never retained output suffixes. No clocks/resampling/waits
    /// or devices are inferred. Combined worker scratch is capped at 32 MiB.
    pub fn with_input(
        graph: Box<Graph>,
        consumer: AudioConsumer,
        producer: AudioProducer,
        config: ProcessConfig,
    ) -> Result<Self, GraphOutputError> {
        Self::prepare(graph, Some(consumer), producer, config)
    }
    fn prepare(
        mut graph: Box<Graph>,
        consumer: Option<AudioConsumer>,
        producer: AudioProducer,
        config: ProcessConfig,
    ) -> Result<Self, GraphOutputError> {
        WorkerIo::validate(&consumer, &producer, config)?;
        graph
            .prepare(config)
            .map_err(GraphOutputError::Preparation)?;
        Ok(Self {
            graph,
            io: WorkerIo::allocate(consumer, producer, config),
        })
    }
    /// Start this fixed graph on its explicit worker owner. Finish/drop before
    /// returning storage to main for graph/native retirement.
    pub fn start_audio(&mut self) -> Result<GraphOutputSession<'_>, GraphOutputError> {
        let graph = self.graph.start_audio().map_err(GraphOutputError::Start)?;
        Ok(self.io.session(WorkerGraphSession::Fixed(graph)))
    }
    /// Consume stopped worker storage on control. The caller retires the graph
    /// and separately retained native main controls; queued samples remain owned
    /// by the consumer until drain/drop. No automatic queue discard is implied.
    pub fn into_graph(self) -> Box<Graph> {
        self.graph
    }
}
/// Buffered prepared dynamic graph, reusing fixed-worker queue/event/scratch
/// semantics. The engine must already be prepared at the exact configuration;
/// no graph is reprepared. Offer/reclaim through its separately retained control.
/// Swaps occur only on fresh renders after prior audio suffixes publish. Queued
/// audio retains the old graph's copied samples, without crossfade/tail drain.
/// Worker start/stop precedes main engine/native retirement and endpoint drop.
pub struct PreparedDynamicGraphOutput {
    graph: Box<DynamicGraph>,
    io: WorkerIo,
}
impl PreparedDynamicGraphOutput {
    pub fn new(
        graph: Box<DynamicGraph>,
        producer: AudioProducer,
        config: ProcessConfig,
    ) -> Result<Self, GraphOutputError> {
        Self::prepare(graph, None, producer, config)
    }
    /// Add matching queued f32 input. Scratch, preflight, underflow and fault latch
    /// follow the fixed worker contract. Input is consumed once per fresh render.
    pub fn with_input(
        graph: Box<DynamicGraph>,
        consumer: AudioConsumer,
        producer: AudioProducer,
        config: ProcessConfig,
    ) -> Result<Self, GraphOutputError> {
        Self::prepare(graph, Some(consumer), producer, config)
    }
    fn prepare(
        graph: Box<DynamicGraph>,
        consumer: Option<AudioConsumer>,
        producer: AudioProducer,
        config: ProcessConfig,
    ) -> Result<Self, GraphOutputError> {
        if graph.configuration() != config {
            return Err(GraphOutputError::Configuration);
        }
        WorkerIo::validate(&consumer, &producer, config)?;
        Ok(Self {
            graph,
            io: WorkerIo::allocate(consumer, producer, config),
        })
    }
    /// Confine dynamic native lifecycle to this worker until session finish/drop.
    pub fn start_audio(&mut self) -> Result<GraphOutputSession<'_>, GraphOutputError> {
        let graph = self.graph.start_audio().map_err(GraphOutputError::Start)?;
        Ok(self.io.session(WorkerGraphSession::Dynamic(graph)))
    }
    /// Consume stopped storage on main; retire active/pending graphs and reclaim
    /// the separate control endpoint/native owners outside worker callbacks.
    pub fn into_graph(self) -> Box<DynamicGraph> {
        self.graph
    }
}
impl WorkerIo {
    fn validate(
        consumer: &Option<AudioConsumer>,
        producer: &AudioProducer,
        config: ProcessConfig,
    ) -> Result<(), GraphOutputError> {
        if config.layout().input_channels() != consumer.as_ref().map_or(0, AudioConsumer::channels)
            || config.layout().output_channels() != producer.channels()
            || config.maximum_events() > MAXIMUM_GRAPH_EVENTS
            || config.maximum_output_events() > MAXIMUM_GRAPH_EVENTS
            || !config.auxiliary_buses().is_empty()
            || config.maximum_frames() > 65536
            || (config.layout().input_channels() + config.layout().output_channels())
                * config.maximum_frames()
                * std::mem::size_of::<f32>()
                * 2
                > 32 * 1024 * 1024
        {
            return Err(GraphOutputError::Configuration);
        }
        Ok(())
    }
    fn allocate(
        consumer: Option<AudioConsumer>,
        producer: AudioProducer,
        config: ProcessConfig,
    ) -> Self {
        Self {
            consumer,
            input_planar: vec![vec![0.; config.maximum_frames()]; config.layout().input_channels()],
            input_interleaved: vec![0.; config.maximum_frames() * config.layout().input_channels()],
            fault: None,
            last_swap: None,
            producer,
            config,
            planar: vec![vec![0.; config.maximum_frames()]; config.layout().output_channels()],
            interleaved: vec![0.; config.maximum_frames() * config.layout().output_channels()],
            pending: 0..0,
        }
    }
    fn session<'a>(&'a mut self, graph: WorkerGraphSession<'a>) -> GraphOutputSession<'a> {
        GraphOutputSession {
            graph,
            consumer: self.consumer.as_mut(),
            input_planar: &mut self.input_planar,
            input_interleaved: &mut self.input_interleaved,
            fault: &mut self.fault,
            last_swap: &mut self.last_swap,
            producer: &mut self.producer,
            config: self.config,
            planar: &mut self.planar,
            interleaved: &mut self.interleaved,
            pending: &mut self.pending,
        }
    }
}
enum WorkerGraphSession<'a> {
    Fixed(GraphAudioSession<'a>),
    Dynamic(DynamicGraphAudioSession<'a>),
}
impl WorkerGraphSession<'_> {
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_>,
    ) -> (GraphStatus, Option<GraphSwapStatus>) {
        match self {
            Self::Fixed(graph) => (graph.process(block), None),
            Self::Dynamic(graph) => {
                let status = graph.process(block);
                (status.processing, Some(status.swap))
            }
        }
    }
    fn reset(&mut self) {
        match self {
            Self::Fixed(graph) => graph.reset(),
            Self::Dynamic(graph) => graph.reset(),
        }
    }
}
/// Non-Send borrowed graph worker session. Rendering/publishing are bounded and
/// heap-free; worker wake/sleep, graph reset, queue/device rate matching and
/// shutdown/join remain explicit. Queue underruns are handled by RingOutput.
///
/// ```compile_fail
/// use ravet_device::GraphOutputSession;
/// fn require_send<T: Send>() {}
/// require_send::<GraphOutputSession<'static>>();
/// ```
pub struct GraphOutputSession<'a> {
    graph: WorkerGraphSession<'a>,
    consumer: Option<&'a mut AudioConsumer>,
    input_planar: &'a mut [Vec<f32>],
    input_interleaved: &'a mut [f32],
    fault: &'a mut Option<GraphStatus>,
    last_swap: &'a mut Option<GraphSwapStatus>,
    producer: &'a mut AudioProducer,
    config: ProcessConfig,
    planar: &'a mut [Vec<f32>],
    interleaved: &'a mut [f32],
    pending: &'a mut std::ops::Range<usize>,
}
impl GraphOutputSession<'_> {
    /// Render one fixed block only after the prior suffix has been published.
    /// Transport applies only to a new render; it is not a device timestamp.
    /// Graph faults publish none of the faulty block and may follow native
    /// history advancement. No retry, reset, spin, sleep or I/O occurs here.
    pub fn pump(&mut self, transport: Transport) -> Result<GraphOutputProgress, GraphStatus> {
        self.pump_events(&[], transport)
    }
    /// Supply ordered sample-offset input for one fresh render. While draining
    /// retained audio, inputs/transport are untouched and progress acknowledges
    /// no event consumption. Keep those inputs until a later fresh render.
    /// Validation occurs through the graph before processing; faults may follow
    /// earlier node mutation and must not be treated as a safe delivery retry.
    /// No cross-thread queue, device-clock conversion or event retiming is inferred.
    pub fn pump_events(
        &mut self,
        events: &[Event],
        transport: Transport,
    ) -> Result<GraphOutputProgress, GraphStatus> {
        self.pump_io(events, transport, &mut [])
    }
    /// Supply bounded output storage of exactly the prepared capacity for each
    /// fresh render. Only the returned output_event_count prefix is valid; storage
    /// tails and pending-call storage remain untouched. Output is produced once
    /// per new render, even when queue publication is partial or currently zero.
    /// Caller owns control-thread delivery and any correlation with queued audio.
    /// Errors provide no valid output prefix and may follow native input/history
    /// mutation and queued input consumption. Malformed outer blocks reject before
    /// dequeuing input. Graph faults retain their error until reset, with no further
    /// queue consumption/publication; earlier input/history cannot be rolled back.
    /// No telemetry queue, replay, coalescing or acknowledgement is inferred.
    pub fn pump_io(
        &mut self,
        events: &[Event],
        transport: Transport,
        storage: &mut [Event],
    ) -> Result<GraphOutputProgress, GraphStatus> {
        let channels = self.config.layout().output_channels();
        let mut rendered_frames = 0;
        let mut output_event_count = 0;
        let mut swap = None;
        let mut input_frames_consumed = 0;
        let mut input_underflow_frames = 0;
        if self.pending.start == self.pending.end {
            let frames = self.config.maximum_frames();
            if let Some(status) = *self.fault {
                return Err(status);
            }
            // Validate outer event/storage contracts before consuming queued audio.
            {
                let mut iter = self.planar.iter_mut();
                let mut outputs: [&mut [f32]; MAXIMUM_CHANNELS] = std::array::from_fn(|_| {
                    iter.next()
                        .map_or(&mut [] as &mut [f32], |samples| samples.as_mut_slice())
                });
                let mut iter = self.input_planar.iter();
                let inputs: [&[f32]; MAXIMUM_CHANNELS] = std::array::from_fn(|_| {
                    iter.next()
                        .map_or(&[] as &[f32], |samples| samples.as_slice())
                });
                let audio = AudioBuffer::separate(
                    &inputs[..self.config.layout().input_channels()],
                    &mut outputs[..channels],
                    frames,
                )
                .expect("prepared graph IO");
                let mut block = ProcessBlock::new(audio, events, transport);
                if !storage.is_empty() || self.config.maximum_output_events() != 0 {
                    block.output_events = Some(OutputEvents::new(storage, frames));
                }
                self.config
                    .validate_block(&block)
                    .map_err(|error| GraphStatus::Fault { node: None, error })?;
            }
            if let Some(consumer) = self.consumer.as_mut() {
                self.input_interleaved.fill(0.);
                input_frames_consumed = consumer
                    .pop(self.input_interleaved)
                    .expect("prepared input frames");
                input_underflow_frames = frames - input_frames_consumed;
                let input_channels = self.config.layout().input_channels();
                for frame in 0..frames {
                    for channel in 0..input_channels {
                        self.input_planar[channel][frame] =
                            self.input_interleaved[frame * input_channels + channel];
                    }
                }
            }
            let mut input_iter = self.input_planar.iter();
            let inputs: [&[f32]; MAXIMUM_CHANNELS] = std::array::from_fn(|_| {
                input_iter
                    .next()
                    .map_or(&[] as &[f32], |samples| samples.as_slice())
            });
            let mut iter = self.planar.iter_mut();
            let mut outputs: [&mut [f32]; MAXIMUM_CHANNELS] = std::array::from_fn(|_| {
                iter.next()
                    .map_or(&mut [] as &mut [f32], |samples| samples.as_mut_slice())
            });
            let audio = AudioBuffer::separate(
                &inputs[..self.config.layout().input_channels()],
                &mut outputs[..channels],
                frames,
            )
            .expect("prepared output");
            let mut block = ProcessBlock::new(audio, events, transport);
            if !storage.is_empty() || self.config.maximum_output_events() != 0 {
                block.output_events = Some(OutputEvents::new(storage, frames));
            }
            let (status, render_swap) = self.graph.process(&mut block);
            swap = render_swap;
            *self.last_swap = render_swap;
            if status != GraphStatus::Continue {
                *self.fault = Some(status);
                return Err(status);
            }
            output_event_count = block
                .output_events
                .as_ref()
                .map_or(0, |output| output.len());
            for frame in 0..frames {
                for channel in 0..channels {
                    self.interleaved[frame * channels + channel] = self.planar[channel][frame];
                }
            }
            *self.pending = 0..frames;
            rendered_frames = frames;
        }
        let published_frames = self
            .producer
            .push(&self.interleaved[self.pending.start * channels..self.pending.end * channels])
            .expect("prepared complete frames");
        self.pending.start += published_frames;
        Ok(GraphOutputProgress {
            rendered_frames,
            published_frames,
            pending_frames: self.pending.len(),
            input_events_consumed: rendered_frames != 0,
            output_event_count,
            input_frames_consumed,
            input_underflow_frames,
            swap,
        })
    }
    /// Most recent dynamic graph processing handoff outcome, including a swap
    /// followed by a processing fault. None before processing or for fixed graphs.
    /// Pending publication, outer rejection and fault-latch calls preserve this
    /// historical value; use progress.swap for successful current-call outcomes.
    pub fn last_swap(&self) -> Option<GraphSwapStatus> {
        *self.last_swap
    }
    /// Reset graph histories on their owner; queued and retained audio is not
    /// discarded. Stop/join both endpoints before explicitly clearing a queue.
    pub fn reset(&mut self) {
        self.graph.reset();
        *self.fault = None;
    }
    pub fn finish(self) {}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RingOutputStatistics {
    pub source_frames: u64,
    pub underflow_frames: u64,
}
/// Relaxed independent queue-consumption counters, not a coherent clock or
/// latency measurement. Clone during preparation, before callback ownership.
#[derive(Clone, Debug)]
pub struct RingOutputMonitor(Arc<[AtomicU64; 2]>);
impl RingOutputMonitor {
    pub fn statistics(&self) -> RingOutputStatistics {
        RingOutputStatistics {
            source_frames: self.0[0].load(Ordering::Relaxed),
            underflow_frames: self.0[1].load(Ordering::Relaxed),
        }
    }
}
/// Prepared queue consumer usable with the existing f32 output device adapter.
/// A worker must render at this explicit rate; no resampling/clock matching is
/// inferred. Valid processing consumes a whole-frame prefix and silences every
/// missing sample. Invalid blocks do not dequeue. Drop belongs after callbacks.
pub struct RingOutput {
    consumer: AudioConsumer,
    sample_rate: f64,
    config: Option<ProcessConfig>,
    scratch: Vec<f32>,
    monitor: RingOutputMonitor,
}
impl RingOutput {
    pub fn new(consumer: AudioConsumer, sample_rate: f64) -> Result<Self, PrepareError> {
        if !sample_rate.is_finite() || sample_rate <= 0. {
            return Err(PrepareError::InvalidSampleRate);
        }
        Ok(Self {
            consumer,
            sample_rate,
            config: None,
            scratch: Vec::new(),
            monitor: RingOutputMonitor(Arc::new([AtomicU64::new(0), AtomicU64::new(0)])),
        })
    }
    pub fn statistics(&self) -> RingOutputStatistics {
        self.monitor.statistics()
    }
    pub fn monitor(&self) -> RingOutputMonitor {
        self.monitor.clone()
    }
}
impl Processor for RingOutput {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.sample_rate() != self.sample_rate
            || config.layout().input_channels() != 0
            || config.layout().output_channels() != self.consumer.channels()
            || config.maximum_events() != 0
            || config.maximum_output_events() != 0
            || !config.auxiliary_buses().is_empty()
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        if config.maximum_frames() > 65536 {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        self.scratch = vec![0.; config.maximum_frames() * self.consumer.channels()];
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        let Some(config) = self.config else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::NotPrepared);
        };
        if let Err(error) = config.validate_block(block) {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let frames = block.audio.frames();
        let channels = self.consumer.channels();
        let samples = &mut self.scratch[..frames * channels];
        samples.fill(0.);
        let source = self
            .consumer
            .pop(samples)
            .expect("prepared complete frames");
        for channel in 0..channels {
            let output = block.audio.output_mut(channel).expect("prepared output");
            for (frame, sample) in output.iter_mut().enumerate() {
                *sample = samples[frame * channels + channel];
            }
        }
        self.monitor.0[0].fetch_add(source as u64, Ordering::Relaxed);
        self.monitor.0[1].fetch_add((frames - source) as u64, Ordering::Relaxed);
        if source == 0 {
            ProcessStatus::Silence
        } else {
            ProcessStatus::Continue
        }
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.config = None;
    }
}
