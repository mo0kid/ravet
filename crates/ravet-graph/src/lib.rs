//! Prepared acyclic native f32/f64 routing over owned portable processors.
//!
//! Graph editing/preparation allocate on control threads; rendering/reset use
//! prepared scratch and delay storage. This scheduler is not a `Processor`
//! implementation: a later node's fault cannot roll back earlier node histories
//! or parameters. Such faults silence host output and disable the graph until
//! reset. Invalid outer blocks reject before any node execution.
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, EventError, MAXIMUM_CHANNELS, OutputEvents,
    ParameterId, PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor,
    Sample, Tail,
};
/// Maximum prepared input or generated output events per graph/node block.
/// Queue capacities elsewhere may be larger; they do not raise this render limit.
pub const MAXIMUM_GRAPH_EVENTS: usize = 1024;
/// Sealed native graph precision through foundation's `Sample` contract.
/// Route multiplication and summation retain the selected sample type.
pub trait GraphSample:
    Sample + Send + 'static + std::ops::Mul<Output = Self> + std::ops::AddAssign
{
    /// Whether a route gain is finite.
    fn finite(self) -> bool;
}
impl GraphSample for f32 {
    fn finite(self) -> bool {
        self.is_finite()
    }
}
impl GraphSample for f64 {
    fn finite(self) -> bool {
        self.is_finite()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId(pub u32);
/// Node processor ownership, layout and stable product-defined identity.
pub struct GraphNode<S: GraphSample = f32> {
    pub id: NodeId,
    pub layout: AudioLayout,
    pub processor: Box<dyn Processor<S> + Send>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Input(usize),
    Node { id: NodeId, channel: usize },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Output(usize),
    Node { id: NodeId, channel: usize },
}
/// Parallel connections add in declaration order. Gains must be finite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Connection<S: GraphSample = f32> {
    pub source: Source,
    pub destination: Destination,
    pub gain: S,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphError {
    Capacity,
    DuplicateId(NodeId),
    MissingNode(NodeId),
    InvalidChannel,
    InvalidGain,
    Cycle,
    /// Event routes can be edited only after deactivation.
    Active,
}
impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "graph error: {self:?}")
    }
}
impl std::error::Error for GraphError {}
/// A node failure can follow successful earlier node execution. Outer errors
/// have node=None and do not execute nodes. All faults clear host outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphStatus {
    Continue,
    Fault {
        node: Option<NodeId>,
        error: ProcessError,
    },
    /// Event merge overflow may follow earlier successful node execution.
    EventFault {
        source: Option<NodeId>,
        destination: Option<NodeId>,
        error: EventError,
    },
}
#[derive(Clone, Copy)]
enum InputSource {
    Host(usize),
    Node(usize, usize),
}
#[derive(Clone, Copy)]
enum OutputDestination {
    Host(usize),
    Node(usize, usize),
}
struct Route<S: GraphSample> {
    source: InputSource,
    destination: OutputDestination,
    gain: S,
    delay: Vec<S>,
    cursor: usize,
}
impl<S: GraphSample> Route<S> {
    fn sample(&mut self, value: S) -> S {
        if self.delay.is_empty() {
            return value * self.gain;
        }
        let delayed = self.delay[self.cursor];
        self.delay[self.cursor] = value;
        self.cursor = (self.cursor + 1) % self.delay.len();
        delayed * self.gain
    }
}
struct Node<S: GraphSample> {
    definition: GraphNode<S>,
    inputs: Vec<Vec<S>>,
    outputs: Vec<Vec<S>>,
    input_events: Vec<Event>,
    output_events: Vec<Event>,
    output_event_count: usize,
}
/// Static graph, at most 128 nodes and 4096 connections. No feedback cycles.
/// Preparation bounds total scratch/delay to 64 MiB (16,777,216 f32 or
/// 8,388,608 f64 values),
/// path latency to 65536 samples, maximum block 65536 and events 1024.
/// Events broadcast by default; explicit filtered routes can include generated
/// node/host events. Storage counts audio and event scratch together. Event
/// offsets are not delayed by audio latency compensation.
pub struct Graph<S: GraphSample = f32> {
    layout: AudioLayout,
    nodes: Vec<Node<S>>,
    routes: Vec<Route<S>>,
    event_routes: Option<Vec<events::EventRoute>>,
    host_events: Vec<Event>,
    order: Vec<usize>,
    incoming: Vec<Vec<usize>>,
    outgoing: Vec<usize>,
    host_input: Vec<Vec<S>>,
    config: Option<ProcessConfig>,
    latency: u32,
    tail: Tail,
    faulted: bool,
    audio_owner: Option<std::thread::ThreadId>,
}
impl<S: GraphSample> Graph<S> {
    pub fn new(
        layout: AudioLayout,
        nodes: Vec<GraphNode<S>>,
        connections: &[Connection<S>],
    ) -> Result<Self, GraphError> {
        if nodes.len() > 128 || connections.len() > 4096 {
            return Err(GraphError::Capacity);
        }
        for (i, node) in nodes.iter().enumerate() {
            if nodes[..i].iter().any(|n| n.id == node.id) {
                return Err(GraphError::DuplicateId(node.id));
            }
        }
        let index = |id| {
            nodes
                .iter()
                .position(|n| n.id == id)
                .ok_or(GraphError::MissingNode(id))
        };
        let mut incoming = vec![Vec::new(); nodes.len()];
        let mut outgoing = Vec::new();
        let mut routes = Vec::new();
        let mut degrees = vec![0usize; nodes.len()];
        for connection in connections {
            if !connection.gain.finite() {
                return Err(GraphError::InvalidGain);
            }
            let source = match connection.source {
                Source::Input(c) => {
                    if c >= layout.input_channels() {
                        return Err(GraphError::InvalidChannel);
                    }
                    InputSource::Host(c)
                }
                Source::Node { id, channel } => {
                    let i = index(id)?;
                    if channel >= nodes[i].layout.output_channels() {
                        return Err(GraphError::InvalidChannel);
                    }
                    InputSource::Node(i, channel)
                }
            };
            let destination = match connection.destination {
                Destination::Output(c) => {
                    if c >= layout.output_channels() {
                        return Err(GraphError::InvalidChannel);
                    }
                    outgoing.push(routes.len());
                    OutputDestination::Host(c)
                }
                Destination::Node { id, channel } => {
                    let i = index(id)?;
                    if channel >= nodes[i].layout.input_channels() {
                        return Err(GraphError::InvalidChannel);
                    }
                    incoming[i].push(routes.len());
                    if matches!(source, InputSource::Node(..)) {
                        degrees[i] += 1;
                    }
                    OutputDestination::Node(i, channel)
                }
            };
            routes.push(Route {
                source,
                destination,
                gain: connection.gain,
                delay: Vec::new(),
                cursor: 0,
            });
        }
        let mut order = Vec::new();
        let mut done = vec![false; nodes.len()];
        for _ in 0..nodes.len() {
            let i = (0..nodes.len())
                .find(|&i| !done[i] && degrees[i] == 0)
                .ok_or(GraphError::Cycle)?;
            done[i] = true;
            order.push(i);
            for route in &routes {
                if matches!(route.source,InputSource::Node(n,_) if n==i)
                    && let OutputDestination::Node(n, _) = route.destination
                {
                    degrees[n] -= 1;
                }
            }
        }
        Ok(Self {
            layout,
            nodes: nodes
                .into_iter()
                .map(|definition| Node {
                    definition,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    input_events: Vec::new(),
                    output_events: Vec::new(),
                    output_event_count: 0,
                })
                .collect(),
            routes,
            event_routes: None,
            host_events: Vec::new(),
            order,
            incoming,
            outgoing,
            host_input: Vec::new(),
            config: None,
            latency: 0,
            tail: Tail::None,
            faulted: false,
            audio_owner: None,
        })
    }
    /// Prepare all nodes and compensate differing route arrival latencies.
    /// Active sessions reject before changes. Other failure deactivates all
    /// nodes and leaves graph inactive. No automatic sample conversion, bus
    /// expansion, dynamic graph edit or tail draining.
    pub fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        if self.audio_owner.is_some() {
            return Err(PrepareError::AudioSessionActive);
        }
        self.deactivate();
        let result = self.prepare_inner(config);
        if result.is_err() {
            self.deactivate();
        }
        result
    }
    fn prepare_inner(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        if config.layout() != self.layout
            || !config.auxiliary_buses().is_empty()
            || (self.event_routes.is_none() && config.maximum_output_events() != 0)
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        if config.maximum_frames() > 65536
            || config.maximum_events() > MAXIMUM_GRAPH_EVENTS
            || config.maximum_output_events() > MAXIMUM_GRAPH_EVENTS
        {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        let event_bytes = if self.event_routes.is_some() {
            (self.nodes.len() * (config.maximum_events() + config.maximum_output_events())
                + config.maximum_output_events())
                * std::mem::size_of::<Event>()
        } else {
            0
        };
        let maximum_samples = (64 * 1024 * 1024usize)
            .checked_sub(event_bytes)
            .ok_or(PrepareError::InvalidMaximumFrames)?
            / std::mem::size_of::<S>();
        self.host_events = Vec::with_capacity(if self.event_routes.is_some() {
            config.maximum_output_events()
        } else {
            0
        });
        let channels = self
            .nodes
            .iter()
            .map(|n| n.definition.layout.input_channels() + n.definition.layout.output_channels())
            .sum::<usize>()
            + self.layout.input_channels();
        let mut samples = channels
            .checked_mul(config.maximum_frames())
            .ok_or(PrepareError::InvalidMaximumFrames)?;
        if samples > maximum_samples {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        self.host_input =
            vec![vec![S::default(); config.maximum_frames()]; self.layout.input_channels()];
        for node in &mut self.nodes {
            let c = ProcessConfig::new(
                config.sample_rate(),
                config.maximum_frames(),
                config.maximum_events(),
                node.definition.layout,
            )?
            .with_output_events(if self.event_routes.is_some() {
                config.maximum_output_events()
            } else {
                0
            });
            node.input_events = Vec::with_capacity(if self.event_routes.is_some() {
                config.maximum_events()
            } else {
                0
            });
            node.output_events = vec![
                Event {
                    frame_offset: 0,
                    data: EventData::ParameterValue {
                        id: ParameterId(0),
                        value: 0.
                    }
                };
                c.maximum_output_events()
            ];
            node.output_event_count = 0;
            node.definition.processor.prepare(c)?;
            node.inputs = vec![
                vec![S::default(); config.maximum_frames()];
                node.definition.layout.input_channels()
            ];
            node.outputs = vec![
                vec![S::default(); config.maximum_frames()];
                node.definition.layout.output_channels()
            ];
        }
        let mut latency = [0u32; 128];
        let mut arrival = [0u32; 128];
        let mut tails = [Tail::None; 128];
        for &i in &self.order {
            let latest = self.incoming[i]
                .iter()
                .map(|&r| source_latency(self.routes[r].source, &latency))
                .max()
                .unwrap_or(0);
            arrival[i] = latest;
            latency[i] = latest
                .checked_add(self.nodes[i].definition.processor.latency_samples())
                .filter(|v| *v <= 65536)
                .ok_or(PrepareError::InvalidMaximumFrames)?;
            let mut input_tail = Tail::None;
            for &r in &self.incoming[i] {
                let route = &self.routes[r];
                let extra = latest - source_latency(route.source, &latency);
                input_tail = max_tail(
                    input_tail,
                    add_tail(
                        source_tail(route.source, &tails),
                        Tail::Samples(u64::from(extra)),
                    ),
                );
            }
            tails[i] = add_tail(input_tail, self.nodes[i].definition.processor.tail());
        }
        self.latency = self
            .outgoing
            .iter()
            .map(|&r| source_latency(self.routes[r].source, &latency))
            .max()
            .unwrap_or(0);
        self.tail = Tail::None;
        for route in &mut self.routes {
            let target = match route.destination {
                OutputDestination::Host(_) => self.latency,
                OutputDestination::Node(i, _) => arrival[i],
            };
            let extra = target - source_latency(route.source, &latency);
            samples = samples
                .checked_add(extra as usize)
                .ok_or(PrepareError::InvalidMaximumFrames)?;
            if samples > maximum_samples {
                return Err(PrepareError::InvalidMaximumFrames);
            }
            route.delay = vec![S::default(); extra as usize];
            route.cursor = 0;
            if matches!(route.destination, OutputDestination::Host(_)) {
                self.tail = max_tail(
                    self.tail,
                    add_tail(
                        source_tail(route.source, &tails),
                        Tail::Samples(u64::from(extra)),
                    ),
                );
            }
        }
        self.config = Some(config);
        self.faulted = false;
        Ok(())
    }
    /// Enter an explicit audio-owner session after control-thread preparation.
    /// Start nodes in topological order; failed start rolls earlier nodes back
    /// in reverse order and faults the graph until reset. The non-Send guard
    /// renders/resets here, then stops all nodes before graph transfer/deactivation.
    /// Default portable hooks are no-ops; ordinary process() behavior is unchanged.
    pub fn start_audio(&mut self) -> Result<GraphAudioSession<'_, S>, GraphStatus> {
        self.start_audio_nodes()?;
        Ok(GraphAudioSession {
            graph: self,
            owner: std::marker::PhantomData,
        })
    }
    fn start_audio_nodes(&mut self) -> Result<(), GraphStatus> {
        if self.config.is_none() || self.faulted || self.audio_owner.is_some() {
            return Err(GraphStatus::Fault {
                node: None,
                error: ProcessError::NotPrepared,
            });
        }
        let mut rollback = GraphAudioStartRollback {
            graph: self,
            started: 0,
            committed: false,
        };
        for position in 0..rollback.graph.order.len() {
            let index = rollback.graph.order[position];
            if let Err(error) = rollback.graph.nodes[index]
                .definition
                .processor
                .start_audio()
            {
                return Err(GraphStatus::Fault {
                    node: Some(rollback.graph.nodes[index].definition.id),
                    error,
                });
            }
            rollback.started += 1;
        }
        rollback.graph.audio_owner = Some(std::thread::current().id());
        rollback.committed = true;
        Ok(())
    }
    fn stop_audio_nodes(&mut self) {
        if let Some(owner) = self.audio_owner {
            assert_eq!(
                owner,
                std::thread::current().id(),
                "stop graph on its audio owner"
            );
            for &index in self.order.iter().rev() {
                self.nodes[index].definition.processor.stop_audio();
            }
            self.audio_owner = None;
        }
    }
    pub fn latency_samples(&self) -> u32 {
        self.latency
    }
    pub fn tail(&self) -> Tail {
        self.tail
    }
    /// Render all nodes topologically. Unconnected inputs are zero, routes add
    /// in declaration order, output channel aliases are handled by input copying.
    /// A node fault disables later execution until reset; preceding histories and
    /// requested values cannot be rolled back. Panics are the caller's ABI boundary.
    pub fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, S>) -> GraphStatus {
        let error = match self.config {
            Some(c) if !self.faulted => c.validate_block(block).err(),
            _ => Some(ProcessError::NotPrepared),
        };
        if let Some(error) = error {
            block.clear_outputs();
            return GraphStatus::Fault { node: None, error };
        }
        let frames = block.audio.frames();
        self.host_events.clear();
        for node in &mut self.nodes {
            node.output_event_count = 0;
        }
        for (channel, data) in self.host_input.iter_mut().enumerate() {
            data[..frames]
                .copy_from_slice(block.audio.input(channel).expect("validated graph layout"));
        }
        for order in 0..self.order.len() {
            let i = self.order[order];
            for input in &mut self.nodes[i].inputs {
                input[..frames].fill(S::default());
            }
            for connection in 0..self.incoming[i].len() {
                let r = self.incoming[i][connection];
                let OutputDestination::Node(_, channel) = self.routes[r].destination else {
                    unreachable!()
                };
                for frame in 0..frames {
                    let value = self.source_sample(self.routes[r].source, frame);
                    let value = self.routes[r].sample(value);
                    self.nodes[i].inputs[channel][frame] += value;
                }
            }
            if self.event_routes.is_some()
                && let Err((route, error)) =
                    self.collect_node_events(i, block.events, self.config.unwrap().maximum_events())
            {
                return self.event_fault(block, route, error);
            }
            let node = &mut self.nodes[i];
            let input_views: [&[S]; MAXIMUM_CHANNELS] =
                std::array::from_fn(|c| node.inputs.get(c).map_or(&[] as &[S], |v| &v[..frames]));
            let mut iter = node.outputs.iter_mut();
            let mut output_views: [&mut [S]; MAXIMUM_CHANNELS] = std::array::from_fn(|_| {
                iter.next().map_or(&mut [] as &mut [S], |v| {
                    v[..frames].fill(S::default());
                    &mut v[..frames]
                })
            });
            let audio = AudioBuffer::separate(
                &input_views[..node.inputs.len()],
                &mut output_views[..node.definition.layout.output_channels()],
                frames,
            )
            .expect("prepared node lengths");
            let input_events = if self.event_routes.is_some() {
                node.input_events.as_slice()
            } else {
                block.events
            };
            let mut node_block = ProcessBlock::new(audio, input_events, block.transport);
            if self.event_routes.is_some() {
                node_block.output_events = Some(OutputEvents::new(&mut node.output_events, frames));
            }
            match node.definition.processor.process(&mut node_block) {
                ProcessStatus::Continue => {}
                ProcessStatus::Silence => node_block.clear_outputs(),
                ProcessStatus::Fault(error) => {
                    self.faulted = true;
                    block.clear_outputs();
                    return GraphStatus::Fault {
                        node: Some(node.definition.id),
                        error,
                    };
                }
            }
            node.output_event_count = node_block
                .output_events
                .as_ref()
                .map_or(0, |events| events.as_slice().len());
        }
        if self.event_routes.is_some()
            && let Err((route, error)) =
                self.collect_host_events(block.events, self.config.unwrap().maximum_output_events())
        {
            return self.event_fault(block, route, error);
        }
        block.clear_outputs();
        for connection in 0..self.outgoing.len() {
            let r = self.outgoing[connection];
            let OutputDestination::Host(channel) = self.routes[r].destination else {
                unreachable!()
            };
            for frame in 0..frames {
                let value = self.source_sample(self.routes[r].source, frame);
                let value = self.routes[r].sample(value);
                block.audio.output_mut(channel).expect("validated output")[frame] += value;
            }
        }
        if let Some(output) = &mut block.output_events {
            for &event in &self.host_events {
                output.push(event).expect("bounded validated merged events");
            }
        }
        GraphStatus::Continue
    }
    fn source_sample(&self, source: InputSource, frame: usize) -> S {
        match source {
            InputSource::Host(c) => self.host_input[c][frame],
            InputSource::Node(i, c) => self.nodes[i].outputs[c][frame],
        }
    }
    /// Clear all node histories and route delays, re-enabling a faulted graph.
    /// Requested parameters are preserved by each node's reset contract.
    pub fn reset(&mut self) {
        let _ = self.try_reset();
    }
    /// Reset in execution order, remaining faulted on any refusal.
    /// Earlier nodes may already have reset; history changes cannot roll back.
    /// Route delays and the fault latch clear only after whole success.
    pub fn try_reset(&mut self) -> Result<(), GraphStatus> {
        self.faulted = true;
        for &index in &self.order {
            let node = &mut self.nodes[index];
            node.definition
                .processor
                .try_reset()
                .map_err(|error| GraphStatus::Fault {
                    node: Some(node.definition.id),
                    error,
                })?;
        }
        self.host_events.clear();
        for node in &mut self.nodes {
            node.input_events.clear();
            node.output_event_count = 0;
        }
        for route in &mut self.routes {
            route.delay.fill(S::default());
            route.cursor = 0;
        }
        self.faulted = false;
        Ok(())
    }
    pub fn deactivate(&mut self) {
        assert!(
            self.audio_owner.is_none(),
            "finish graph audio session before deactivation"
        );
        self.config = None;
        self.latency = 0;
        self.tail = Tail::None;
        self.faulted = false;
        for node in &mut self.nodes {
            node.definition.processor.deactivate();
        }
    }
}
struct GraphAudioStartRollback<'a, S: GraphSample> {
    graph: &'a mut Graph<S>,
    started: usize,
    committed: bool,
}
impl<S: GraphSample> Drop for GraphAudioStartRollback<'_, S> {
    fn drop(&mut self) {
        if !self.committed {
            for &index in self.graph.order[..self.started].iter().rev() {
                self.graph.nodes[index].definition.processor.stop_audio();
            }
            self.graph.faulted = true;
        }
    }
}
/// Borrowed audio-owner graph session; stop/drop before returning the graph to
/// control. Do not forget/leak this guard. Its non-Send/non-Sync marker confines
/// normal Rust rendering/reset/stop to the starting owner. Panics remain caller
/// ABI policy; hooks retain implementor callback obligations.
///
/// ```compile_fail
/// use ravet_graph::GraphAudioSession;
/// fn require_send<T: Send>() {}
/// require_send::<GraphAudioSession<'static>>();
/// ```
pub struct GraphAudioSession<'a, S: GraphSample = f32> {
    graph: &'a mut Graph<S>,
    owner: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl<S: GraphSample> GraphAudioSession<'_, S> {
    pub fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, S>) -> GraphStatus {
        self.graph.process(block)
    }
    pub fn reset(&mut self) {
        let _ = self.try_reset();
    }
    /// Forward checked history reset; refusal leaves the graph faulted.
    pub fn try_reset(&mut self) -> Result<(), GraphStatus> {
        self.graph.try_reset()
    }
    pub fn latency_samples(&self) -> u32 {
        self.graph.latency_samples()
    }
    pub fn tail(&self) -> Tail {
        self.graph.tail()
    }
    /// Finish native audio hooks now, retaining prepared histories/storage.
    pub fn finish(self) {}
}
impl<S: GraphSample> Drop for GraphAudioSession<'_, S> {
    fn drop(&mut self) {
        self.graph.stop_audio_nodes();
    }
}
impl<S: GraphSample> Drop for Graph<S> {
    fn drop(&mut self) {
        self.deactivate();
    }
}
fn source_latency(source: InputSource, latency: &[u32; 128]) -> u32 {
    match source {
        InputSource::Host(_) => 0,
        InputSource::Node(i, _) => latency[i],
    }
}
fn source_tail(source: InputSource, tails: &[Tail; 128]) -> Tail {
    match source {
        InputSource::Host(_) => Tail::None,
        InputSource::Node(i, _) => tails[i],
    }
}
fn add_tail(a: Tail, b: Tail) -> Tail {
    let normalize = |t| if t == Tail::Samples(0) { Tail::None } else { t };
    match (normalize(a), normalize(b)) {
        (Tail::Infinite, _) | (_, Tail::Infinite) => Tail::Infinite,
        (Tail::None, b) => b,
        (a, Tail::None) => a,
        (Tail::Samples(a), Tail::Samples(b)) => a.checked_add(b).map_or(Tail::Infinite, |v| {
            if v == 0 { Tail::None } else { Tail::Samples(v) }
        }),
    }
}
fn max_tail(a: Tail, b: Tail) -> Tail {
    let normalize = |t| if t == Tail::Samples(0) { Tail::None } else { t };
    match (normalize(a), normalize(b)) {
        (Tail::Infinite, _) | (_, Tail::Infinite) => Tail::Infinite,
        (Tail::None, b) => b,
        (a, Tail::None) => a,
        (Tail::Samples(a), Tail::Samples(b)) => Tail::Samples(a.max(b)),
    }
}

mod dynamic;
pub use dynamic::{
    DynamicGraph, DynamicGraphAudioSession, DynamicGraphStatus, GraphControl, GraphOfferError,
    GraphOfferFailure, GraphSwapStatus,
};

#[cfg(feature = "convolution-transitions")]
mod convolution_transition;
#[cfg(feature = "convolution-transitions")]
pub use convolution_transition::{
    AlignmentPolicy, ConvolutionTransition, ConvolutionTransitionControl, CrossfadeLaw,
    CrossfadePolicy, LatencyPolicy, MidFadePolicy, PrimingPolicy, ReturnReason, ReturnedEngine,
    TRANSITION_MAXIMUM_ALIGNMENT_FRAMES, TRANSITION_MAXIMUM_CONTINUITY_BYTES,
    TRANSITION_MAXIMUM_FADE_FRAMES, TRANSITION_MAXIMUM_PRIMING_FRAMES, TRANSITION_REPORT_CAPACITY,
    TRANSITION_RETURN_CAPACITY, TransitionContinuity, TransitionEngine, TransitionEvent,
    TransitionOfferError, TransitionOfferFailure, TransitionOptions, TransitionReport,
    TransitionSample, TransitionSetupError, TransitionSetupFailure, TransitionStatus,
    convolution_transition, convolution_transition_with_continuity,
};

mod events;
pub use events::{EventConnection, EventEndpoint, EventFilter};
#[allow(unsafe_code)]
mod typed;
pub use typed::{
    StageAt, StageTuple, TypedChain, TypedChainAudioSession, TypedChainError, TypedChainFailure,
};
mod mono;
pub use mono::{MonoDuplicator, MonoFactoryError, MonoFactoryFailure, SharedMonoKernel};
