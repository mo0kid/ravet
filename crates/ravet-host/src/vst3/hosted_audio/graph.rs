//! Owned VST3 Graph bank: canonical [`Processor`] nodes over native AUDIO
//! leases, with a bounded MAIN retirement registry.
//!
//! [`Vst3GraphBankBuilder`] consumes a MAIN [`Library<'static>`] whose
//! caller-linked provider tables and executable implementation are static.
//! Children retain that Library through a private `Rc` parent; no
//! `Audio<'scope>` or borrowed Instance lifetime is erased. The bank creates and
//! checks every child, runs optional stopped setup (state recall, stopped
//! parameter edits), refreshes parameter/event/audio metadata, prepares every
//! hosted profile and captures latency/tail FIRST. Only then does it start all
//! children on MAIN and begin every native AUDIO lease LAST. A failure unwinds
//! admitted capabilities in reverse before stopping and retiring children.
//!
//! The returned [`Vst3GraphNode`] values are `Send`, not `Sync`, and own no
//! Library, `Rc`, Instance or controller: only the exact native AUDIO handle,
//! the static tables, copied profile/timing and prepared buffers/lanes. Node
//! `Drop` moves that whole bundle into a dedicated preallocated one-slot return
//! ring; it performs no SDK teardown and frees no buffer. The ring consumer
//! always remains in the MAIN registry, never in the disposable
//! [`Vst3GraphBankControl`]. Dropping the control marks the bank orphaned and
//! leaves the module, children and leases retained until
//! [`reclaim_vst3_graph_banks`] on MAIN. Reclamation first gathers returned
//! bundles without native calls, then ends ALL bank AUDIO leases before any
//! stop or retirement. A forgotten node, never-joined worker, refused return or
//! failed native end keeps the whole bank retained and diagnosed; it is never
//! guessed to have ended.
//!
//! The registry holds at most 64 banks of at most 128 children, with checked
//! nonwrapping generations. It is deliberately leaked for the process lifetime
//! and is bound to the platform main thread on Apple targets (to the first
//! accessing thread elsewhere, or under unit tests). Workers never access it.
//!
//! Generic hosted VST3 nodes have no guaranteed SDK history reset: checked
//! reset returns [`ProcessError::RebuildRequired`] without foreign calls and the
//! node stays faulted. Rebuild means stop/join workers, return every node, MAIN
//! reclaim, and construct a fresh bank.
//!
//! Portable Foundation event and transport mapping is not implemented by this
//! node: non-empty portable events refuse before native execution. Complete
//! typed native events, parameter points and block context use the explicit
//! per-node rich lanes ([`LaneOptions`]). No DAW, third-party plug-in,
//! foreign-heap or real-time deadline claim follows from these checks.
//!
//! Nodes are `Send` but not `Sync`, and the control cannot leave MAIN:
//!
//! ```compile_fail,E0277
//! use ravet_host::vst3::instances::hosted_audio::graph::Vst3GraphNode;
//! fn require_sync<T: Sync>() {}
//! require_sync::<Vst3GraphNode<f32>>();
//! ```
//! ```compile_fail,E0277
//! use ravet_host::vst3::instances::hosted_audio::graph::Vst3GraphBankControl;
//! fn require_send<T: Send>() {}
//! require_send::<Vst3GraphBankControl>();
//! ```
//! ```
//! use ravet_host::vst3::instances::hosted_audio::graph::Vst3GraphNode;
//! fn require_send<T: Send>() {}
//! require_send::<Vst3GraphNode<f64>>();
//! ```
use super::{
    AudioState, BusRequest, Error as HostedError, HostedAudioProfile, HostedSample, InChannel,
    MAXIMUM_CHANNELS, OutChannel, Precision, Provider as HostedProvider, Timing, dispatch,
};
use crate::vst3::instances::{Error as InstanceError, Instance, Library, parameters, processing};
use processing::{Context, EventBuses, Input, InputEvents, InputPoints, Output, OutputStorage};
use ravet_foundation::{
    AudioLayout, EventError, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Sample, Tail,
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    ffi::c_void,
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicPtr, AtomicU32, Ordering},
    },
    thread::ThreadId,
};

/// Process-lifetime MAIN registry bank slots.
pub const MAXIMUM_BANKS: usize = 64;
/// Children per bank.
pub const MAXIMUM_BANK_CHILDREN: usize = 128;
/// Complete rich-lane batches per node.
pub const MAXIMUM_LANE_BATCHES: usize = 4;
/// Aggregate reserved rich-lane capacity per bank, including every batch.
pub const MAXIMUM_LANE_BYTES: usize = 64 * 1024 * 1024;

/// Bounded copied node diagnostics, readable on MAIN.
pub mod diagnostic {
    /// Node drop found its return slot full; the bundle was retained (leaked)
    /// rather than freed and the whole bank stays retained.
    pub const RETURN_REFUSED: u32 = 1;
    /// Queued lane mode found no ready batch for a block.
    pub const LANE_MISSING: u32 = 2;
    /// Head batch sequence or owner differed from the expected next batch.
    pub const LANE_SEQUENCE: u32 = 4;
    /// Head batch frame count differed from the block.
    pub const LANE_FRAMES: u32 = 8;
    /// Native execution or reply validation failed; node faulted.
    pub const NATIVE_FAULT: u32 = 16;
    /// Checked/legacy reset refused: rebuild required.
    pub const RESET_REFUSED: u32 = 32;
    /// Completed batch could not be returned; batch retained, not freed.
    pub const DONE_REFUSED: u32 = 64;
    /// Non-empty portable events refused (portable mapping not implemented).
    pub const EVENTS_REFUSED: u32 = 128;
    /// Pre-dispatch admission refusal (shape/precision/capacity).
    pub const ADMISSION: u32 = 256;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankError {
    /// Registry access from a thread other than its MAIN owner.
    Thread,
    /// Reentrant registry access from a lane closure or foreign callback.
    Busy,
    /// Bank slots, children, generations, batches or lane budget exhausted.
    Capacity,
    /// Invalid options or mismatched tables.
    Shape,
    Allocation,
    /// The bank identity no longer names a live registry slot.
    Stale,
    /// Lane batch absent (none free or none completed).
    Empty,
    /// Caller setup closure refusal for a child.
    Setup {
        child: usize,
    },
    Instance {
        child: usize,
        error: InstanceError,
    },
    Parameters {
        child: usize,
        error: parameters::Error,
    },
    Processing {
        child: usize,
        error: processing::Error,
    },
    Hosted {
        child: usize,
        error: HostedError,
    },
}
impl std::fmt::Display for BankError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VST3 graph bank error: {self:?}")
    }
}
impl std::error::Error for BankError {}

/// Checked unique registry identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BankId {
    slot: u32,
    generation: u64,
}
impl BankId {
    pub fn generation(self) -> u64 {
        self.generation
    }
}

/// Per-node rich lane capacities: 1..=4 complete owned batches. Input and
/// output limits follow the processing ABI (events<=2048, payload<=65536,
/// points<=2048, output queues<=64).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneOptions {
    pub batches: usize,
    pub input_events: usize,
    pub input_payload: usize,
    pub input_points: usize,
    pub output_events: usize,
    pub output_payload: usize,
    pub output_queues: usize,
    pub output_points: usize,
}
impl LaneOptions {
    /// Full protocol capacities for `batches` batches.
    pub fn maximum(batches: usize) -> Self {
        Self {
            batches,
            input_events: 2048,
            input_payload: 65536,
            input_points: 2048,
            output_events: 2048,
            output_payload: 65536,
            output_queues: 64,
            output_points: 2048,
        }
    }
    fn bytes(&self) -> Option<usize> {
        use processing::RawEvent;
        let input = self
            .input_events
            .checked_mul(size_of::<RawEvent>())?
            .checked_add(self.input_payload.div_ceil(2) * 2)?
            .checked_add(
                self.input_points
                    .checked_mul(size_of::<parameters::Point>())?,
            )?;
        let output = self
            .output_events
            .checked_mul(size_of::<RawEvent>())?
            .checked_add(self.output_payload.div_ceil(2) * 2)?
            .checked_add(self.output_queues.checked_mul(4)?)?
            .checked_add(
                self.output_points
                    .checked_mul(size_of::<parameters::Point>())?,
            )?;
        input
            .checked_add(output)?
            .checked_add(size_of::<Batch>())?
            .checked_mul(self.batches)
    }
}

/// Explicit per-child selection. Every advertised audio bus needs a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vst3GraphNodeOptions {
    pub class_index: u32,
    pub inputs: Vec<BusRequest>,
    pub outputs: Vec<BusRequest>,
    /// Selected native event buses (bit per index), as for `refresh_processing`.
    pub event_inputs: u16,
    pub event_outputs: u16,
    /// Queued rich-lane mode. `None` keeps strict audio-only admission.
    pub lanes: Option<LaneOptions>,
}

type Setup = dyn FnOnce(&mut Instance<'static, 'static>) -> Result<(), BankError>;

struct Shared {
    diagnostic: AtomicU32,
}
impl Shared {
    fn mark(&self, bits: u32) {
        self.diagnostic.fetch_or(bits, Ordering::Relaxed);
    }
}

/// Exact node-owner identity: nonwrapping bank generation plus child index.
#[derive(Clone, Copy, PartialEq, Eq)]
struct LaneOwner {
    generation: u64,
    child: usize,
}
struct Batch {
    owner: LaneOwner,
    sequence: u64,
    frames: u32,
    events: InputEvents,
    points: InputPoints,
    context: Option<Context>,
    output: OutputStorage,
    outcome: Outcome,
}
enum Outcome {
    Pending,
    Published(processing::Report),
    Failed(HostedError),
}
struct LaneAudio {
    ready: rtrb::Consumer<Box<Batch>>,
    done: rtrb::Producer<Box<Batch>>,
    owner: LaneOwner,
    next: u64,
}
struct LaneMain {
    ready: rtrb::Producer<Box<Batch>>,
    done: rtrb::Consumer<Box<Batch>>,
    // Boxed batches move between rings by pointer; never copied or rebuilt.
    #[allow(clippy::vec_box)]
    free: Vec<Box<Batch>>,
    owner: LaneOwner,
    next: u64,
    buses: EventBuses,
}

/// Everything a node owns on AUDIO. Moved, never dropped, on the worker.
struct Bundle<S> {
    handle: NonNull<c_void>,
    state: AudioState<'static>,
    buses: EventBuses,
    scratch: Vec<S>,
    stride: usize,
    empty: InputEvents,
    discard: OutputStorage,
    lanes: Option<LaneAudio>,
}
// SAFETY: the handle is the sole exclusive native AUDIO capability and the
// tables are immutable process-static code/data whose AUDIO entries support a
// single bound worker. No Rc/Library/Instance/controller is reachable.
unsafe impl<S: Send> Send for Bundle<S> {}

struct ChildSlot {
    instance: Option<Instance<'static, 'static>>,
    lease: Option<NonNull<c_void>>,
    /// A native end of this lease failed; the child is poisoned until a retry
    /// proves the end, after which ordinary stop/retirement may resume.
    end_failed: bool,
    returns: rtrb::Consumer<Box<dyn Any + Send>>,
    returned: bool,
    shared: Arc<Shared>,
    lanes: Option<LaneMain>,
    profile: HostedAudioProfile,
    timing: Timing,
}
struct BankSlot {
    generation: u64,
    reserved: bool,
    orphaned: Rc<Cell<bool>>,
    children: Vec<ChildSlot>,
    library: Option<Rc<Library<'static>>>,
    failure: Option<BankError>,
}
struct Registry {
    banks: Vec<Option<BankSlot>>,
    next_generation: u64,
}
struct RegistryCell {
    owner: ThreadId,
    registry: RefCell<Registry>,
}
static REGISTRY: AtomicPtr<RegistryCell> = AtomicPtr::new(std::ptr::null_mut());

#[cfg(all(target_vendor = "apple", not(test)))]
fn platform_main() -> bool {
    unsafe extern "C" {
        fn pthread_main_np() -> i32;
    }
    // SAFETY: libSystem query without arguments or retained state.
    unsafe { pthread_main_np() == 1 }
}
#[cfg(not(all(target_vendor = "apple", not(test))))]
fn platform_main() -> bool {
    true
}
fn registry() -> Result<&'static RegistryCell, BankError> {
    if !platform_main() {
        return Err(BankError::Thread);
    }
    let current = std::thread::current().id();
    let mut cell = REGISTRY.load(Ordering::Acquire);
    if cell.is_null() {
        let mut banks = Vec::new();
        banks
            .try_reserve_exact(MAXIMUM_BANKS)
            .map_err(|_| BankError::Allocation)?;
        banks.resize_with(MAXIMUM_BANKS, || None);
        let fresh = Box::into_raw(Box::new(RegistryCell {
            owner: current,
            registry: RefCell::new(Registry {
                banks,
                next_generation: 1,
            }),
        }));
        match REGISTRY.compare_exchange(
            std::ptr::null_mut(),
            fresh,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => cell = fresh,
            Err(existing) => {
                // SAFETY: unpublished allocation created on this thread.
                drop(unsafe { Box::from_raw(fresh) });
                cell = existing;
            }
        }
    }
    // SAFETY: published exactly once (AcqRel/Acquire) and deliberately never
    // freed. Any thread may read the immutable `owner` field; only the recorded
    // owner thread ever touches the RefCell and its non-Send contents.
    let cell = unsafe { &*cell };
    if cell.owner != current {
        return Err(BankError::Thread);
    }
    Ok(cell)
}
fn with_registry<R>(f: impl FnOnce(&mut Registry) -> R) -> Result<R, BankError> {
    let cell = registry()?;
    let mut registry = cell
        .registry
        .try_borrow_mut()
        .map_err(|_| BankError::Busy)?;
    Ok(f(&mut registry))
}
fn live_slot(registry: &mut Registry, id: BankId) -> Result<&mut BankSlot, BankError> {
    registry
        .banks
        .get_mut(id.slot as usize)
        .and_then(Option::as_mut)
        .filter(|slot| slot.generation == id.generation && !slot.reserved)
        .ok_or(BankError::Stale)
}

/// Releases a construction reservation on error or panic unwind.
struct Reservation {
    id: BankId,
    armed: bool,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if self.armed {
            let id = self.id;
            let _ = with_registry(|r| {
                if let Some(slot) = r.banks.get_mut(id.slot as usize)
                    && slot
                        .as_ref()
                        .is_some_and(|s| s.generation == id.generation && s.reserved)
                {
                    *slot = None;
                }
            });
        }
    }
}

struct PreparedChild<S> {
    instance: Instance<'static, 'static>,
    profile: HostedAudioProfile,
    timing: Timing,
    state: AudioState<'static>,
    buses: EventBuses,
    scratch: Vec<S>,
    discard: OutputStorage,
    lane_audio: Option<LaneAudio>,
    lane_main: Option<LaneMain>,
}

/// MAIN construction of one owned bank from one static Library.
pub struct Vst3GraphBankBuilder<S: HostedSample> {
    library: Library<'static>,
    parameters: parameters::Provider<'static>,
    processing: processing::Provider<'static>,
    hosted: HostedProvider<'static>,
    sample_rate: f64,
    maximum_frames: u32,
    children: Vec<(Vst3GraphNodeOptions, Option<Box<Setup>>)>,
    sample: PhantomData<S>,
}
impl<S: HostedSample> Vst3GraphBankBuilder<S> {
    /// All companion tables must be the exact origins paired with the
    /// Library's instance provider.
    pub fn new(
        library: Library<'static>,
        parameters: parameters::Provider<'static>,
        processing: processing::Provider<'static>,
        hosted: HostedProvider<'static>,
        sample_rate: f64,
        maximum_frames: u32,
    ) -> Result<Self, BankError> {
        let instance = library.provider.raw;
        if !std::ptr::eq(parameters.raw.provider, instance)
            || !std::ptr::eq(processing.raw.provider, instance)
            || !std::ptr::eq(hosted.raw.instance_origin, instance)
            || !std::ptr::eq(hosted.raw.processing_origin, processing.raw)
            || !sample_rate.is_finite()
            || sample_rate <= 0.
            || sample_rate > 768000.
            || !(1..=65536).contains(&maximum_frames)
        {
            return Err(BankError::Shape);
        }
        Ok(Self {
            library,
            parameters,
            processing,
            hosted,
            sample_rate,
            maximum_frames,
            children: Vec::new(),
            sample: PhantomData,
        })
    }
    /// Add one child with no stopped setup. Returns its child index.
    pub fn node(&mut self, options: Vst3GraphNodeOptions) -> Result<usize, BankError> {
        self.push(options, None)
    }
    /// Add one child whose stopped setup (state recall, stopped parameter
    /// edits) runs on MAIN after creation and before metadata refresh and
    /// preparation. No AUDIO lease exists while it runs.
    pub fn node_with_setup(
        &mut self,
        options: Vst3GraphNodeOptions,
        setup: impl FnOnce(&mut Instance<'static, 'static>) -> Result<(), BankError> + 'static,
    ) -> Result<usize, BankError> {
        self.push(options, Some(Box::new(setup)))
    }
    fn push(
        &mut self,
        options: Vst3GraphNodeOptions,
        setup: Option<Box<Setup>>,
    ) -> Result<usize, BankError> {
        if self.children.len() == MAXIMUM_BANK_CHILDREN {
            return Err(BankError::Capacity);
        }
        if let Some(lanes) = options.lanes
            && (!(1..=MAXIMUM_LANE_BATCHES).contains(&lanes.batches)
                || lanes.input_events > 2048
                || lanes.input_payload > 65536
                || lanes.input_points > 2048
                || lanes.output_events > 2048
                || lanes.output_payload > 65536
                || lanes.output_queues > 64
                || lanes.output_points > 2048)
        {
            return Err(BankError::Shape);
        }
        self.children
            .try_reserve(1)
            .map_err(|_| BankError::Allocation)?;
        self.children.push((options, setup));
        Ok(self.children.len() - 1)
    }
    /// Create, set up, refresh and prepare every child, then start all and
    /// begin all leases. On failure no node escapes; native owners are released
    /// in reverse unless a failed lease end requires orphaned retention.
    pub fn build(self) -> Result<(Vst3GraphBankControl, Vec<Vst3GraphNode<S>>), BankError> {
        if self.children.is_empty() {
            return Err(BankError::Shape);
        }
        let lane_bytes = self.children.iter().try_fold(0usize, |sum, (o, _)| {
            o.lanes
                .map_or(Some(0), |l| l.bytes())
                .and_then(|b| sum.checked_add(b))
        });
        if lane_bytes.is_none_or(|b| b > MAXIMUM_LANE_BYTES) {
            return Err(BankError::Capacity);
        }
        // Reserve a slot and generation before any native call.
        let id = with_registry(|r| {
            let slot = r.banks.iter().position(Option::is_none)?;
            let generation = r.next_generation;
            r.next_generation = generation.checked_add(1)?;
            r.banks[slot] = Some(BankSlot {
                generation,
                reserved: true,
                orphaned: Rc::new(Cell::new(false)),
                children: Vec::new(),
                library: None,
                failure: None,
            });
            Some(BankId {
                slot: slot as u32,
                generation,
            })
        })?
        .ok_or(BankError::Capacity)?;
        let mut reservation = Reservation { id, armed: true };
        let library = Rc::new(self.library);
        let mut prepared: Vec<PreparedChild<S>> = Vec::new();
        prepared
            .try_reserve_exact(self.children.len())
            .map_err(|_| BankError::Allocation)?;
        for (child, (options, setup)) in self.children.into_iter().enumerate() {
            let owner = LaneOwner {
                generation: id.generation,
                child,
            };
            let item = prepare_child::<S>(
                &library,
                child,
                owner,
                options,
                setup,
                self.parameters,
                self.processing,
                self.hosted,
                self.sample_rate,
                self.maximum_frames,
            );
            match item {
                Ok(item) => prepared.push(item),
                Err(error) => {
                    // No lease exists: ordinary MAIN destruction in reverse.
                    while let Some(item) = prepared.pop() {
                        drop(item);
                    }
                    return Err(error);
                }
            }
        }
        // Retain the complete bank in the registry before any lease begins.
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(prepared.len())
            .map_err(|_| BankError::Allocation)?;
        let mut audio = Vec::new();
        audio
            .try_reserve_exact(prepared.len())
            .map_err(|_| BankError::Allocation)?;
        let mut children = Vec::new();
        children
            .try_reserve_exact(prepared.len())
            .map_err(|_| BankError::Allocation)?;
        for item in prepared {
            let (producer, consumer) = rtrb::RingBuffer::<Box<dyn Any + Send>>::new(1);
            let shared = Arc::new(Shared {
                diagnostic: AtomicU32::new(0),
            });
            // Every fallible node part is built before any lease begins, so
            // no lease can be stranded by a late construction error.
            audio.push(Pending {
                returns: producer,
                shared: Arc::clone(&shared),
                state: item.state,
                buses: item.buses,
                stride: self.maximum_frames as usize,
                scratch: item.scratch,
                empty: InputEvents::new(0, 0).map_err(|error| BankError::Processing {
                    child: children.len(),
                    error,
                })?,
                discard: item.discard,
                lanes: item.lane_audio,
                layout: AudioLayout::new(
                    item.state.input_channels as usize,
                    item.state.output_channels as usize,
                )
                .map_err(|_| BankError::Shape)?,
                timing: item.timing,
            });
            children.push(ChildSlot {
                instance: Some(item.instance),
                lease: None,
                end_failed: false,
                returns: consumer,
                returned: false,
                shared,
                lanes: item.lane_main,
                profile: item.profile,
                timing: item.timing,
            });
        }
        let orphaned = with_registry(|r| {
            let slot = r.banks[id.slot as usize].as_mut().expect("reserved slot");
            slot.children = children;
            slot.library = Some(library);
            slot.reserved = false;
            Rc::clone(&slot.orphaned)
        })?;
        reservation.armed = false;
        let control = Vst3GraphBankControl {
            id,
            orphaned,
            main: PhantomData,
        };
        // Start every child on MAIN, then begin every lease LAST.
        let handles = with_registry(|r| start_and_begin(r, id))?;
        let handles = match handles {
            Ok(handles) => handles,
            Err(error) => {
                // Unwound in start_and_begin; dropping the control orphans a
                // possibly retained bank and attempts its release.
                drop(control);
                let _ = reclaim_vst3_graph_banks(Some(id));
                return Err(error);
            }
        };
        for (pending, handle) in audio.into_iter().zip(handles) {
            nodes.push(Vst3GraphNode {
                bundle: Some(Box::new(Bundle {
                    handle,
                    state: pending.state,
                    buses: pending.buses,
                    scratch: pending.scratch,
                    stride: pending.stride,
                    empty: pending.empty,
                    discard: pending.discard,
                    lanes: pending.lanes,
                })),
                returns: pending.returns,
                shared: pending.shared,
                layout: pending.layout,
                sample_rate: pending.state.sample_rate,
                maximum_frames: pending.state.maximum_frames as usize,
                timing: pending.timing,
                prepared: None,
                fault: None,
                no_sync: PhantomData,
            });
        }
        Ok((control, nodes))
    }
}
struct Pending<S> {
    returns: rtrb::Producer<Box<dyn Any + Send>>,
    shared: Arc<Shared>,
    state: AudioState<'static>,
    buses: EventBuses,
    stride: usize,
    scratch: Vec<S>,
    empty: InputEvents,
    discard: OutputStorage,
    lanes: Option<LaneAudio>,
    layout: AudioLayout,
    timing: Timing,
}
#[allow(clippy::too_many_arguments)]
fn prepare_child<S: HostedSample>(
    library: &Rc<Library<'static>>,
    child: usize,
    owner: LaneOwner,
    options: Vst3GraphNodeOptions,
    setup: Option<Box<Setup>>,
    parameter_provider: parameters::Provider<'static>,
    processing_provider: processing::Provider<'static>,
    hosted: HostedProvider<'static>,
    sample_rate: f64,
    maximum_frames: u32,
) -> Result<PreparedChild<S>, BankError> {
    let mut instance = library
        .create_retained(options.class_index)
        .map_err(|error| BankError::Instance { child, error })?;
    if let Some(setup) = setup {
        setup(&mut instance).map_err(|_| BankError::Setup { child })?;
    }
    instance
        .refresh_parameters(parameter_provider)
        .map_err(|error| BankError::Parameters { child, error })?;
    instance
        .refresh_processing(
            processing_provider,
            options.event_inputs,
            options.event_outputs,
        )
        .map_err(|error| BankError::Processing { child, error })?;
    let hosted_error = |error| BankError::Hosted { child, error };
    instance
        .refresh_hosted_audio(hosted)
        .map_err(hosted_error)?;
    let profile = HostedAudioProfile::new(
        S::PRECISION,
        sample_rate,
        maximum_frames,
        &options.inputs,
        &options.outputs,
    )
    .map_err(hosted_error)?;
    let timing = match S::PRECISION {
        Precision::F32 => instance.prepare_hosted32(hosted, &profile),
        Precision::F64 => instance.prepare_hosted64(hosted, &profile),
    }
    .map_err(hosted_error)?;
    if timing.latency_samples > 65536 {
        return Err(BankError::Hosted {
            child,
            error: HostedError::Shape,
        });
    }
    let state = instance.hosted.audio().ok_or(BankError::Hosted {
        child,
        error: HostedError::Lifecycle,
    })?;
    let buses = instance
        .processing_metadata
        .map_or_else(EventBuses::default, |(_, b)| b);
    let stride = maximum_frames as usize;
    let mut scratch = Vec::new();
    let length = stride * profile.map().input_channels();
    scratch
        .try_reserve_exact(length)
        .map_err(|_| BankError::Allocation)?;
    scratch.resize(length, S::default());
    let processing_error = |error| BankError::Processing { child, error };
    let discard = OutputStorage::maximum().map_err(processing_error)?;
    let (lane_audio, lane_main) = match options.lanes {
        None => (None, None),
        Some(lanes) => {
            let (ready_producer, ready_consumer) = rtrb::RingBuffer::new(lanes.batches);
            let (done_producer, done_consumer) = rtrb::RingBuffer::new(lanes.batches);
            let mut free = Vec::new();
            free.try_reserve_exact(lanes.batches)
                .map_err(|_| BankError::Allocation)?;
            for _ in 0..lanes.batches {
                free.push(Box::new(Batch {
                    owner,
                    sequence: 0,
                    frames: 0,
                    events: InputEvents::new(lanes.input_events, lanes.input_payload)
                        .map_err(processing_error)?,
                    points: InputPoints::new(lanes.input_points).map_err(processing_error)?,
                    context: None,
                    output: OutputStorage::new(
                        lanes.output_events,
                        lanes.output_payload,
                        lanes.output_queues,
                        lanes.output_points,
                    )
                    .map_err(processing_error)?,
                    outcome: Outcome::Pending,
                }));
            }
            (
                Some(LaneAudio {
                    ready: ready_consumer,
                    done: done_producer,
                    owner,
                    next: 0,
                }),
                Some(LaneMain {
                    ready: ready_producer,
                    done: done_consumer,
                    free,
                    owner,
                    next: 0,
                    buses,
                }),
            )
        }
    };
    Ok(PreparedChild {
        instance,
        profile,
        timing,
        state,
        buses,
        scratch,
        discard,
        lane_audio,
        lane_main,
    })
}
/// Start all children on MAIN, then begin all native leases. Unwinds admitted
/// capabilities in reverse on failure, retaining any lease whose end failed.
fn start_and_begin(r: &mut Registry, id: BankId) -> Result<Vec<NonNull<c_void>>, BankError> {
    let slot = live_slot(r, id).expect("inserted bank");
    let count = slot.children.len();
    let mut failure = None;
    let mut started = 0;
    for (child, c) in slot.children.iter_mut().enumerate() {
        match c.instance.as_mut().expect("prepared child").start() {
            Ok(()) => started += 1,
            Err(error) => {
                failure = Some(BankError::Instance { child, error });
                break;
            }
        }
    }
    let mut handles = Vec::new();
    if failure.is_none() && handles.try_reserve_exact(count).is_err() {
        failure = Some(BankError::Allocation);
    }
    if failure.is_none() {
        for (child, c) in slot.children.iter_mut().enumerate() {
            let instance = c.instance.as_ref().expect("prepared child");
            let mut token = std::ptr::null_mut();
            let mut result = 0;
            // SAFETY: started stopped-CONTROL-complete child on MAIN; exclusive
            // output token. Every prepare/start for this library already ended.
            let status = unsafe {
                instance.library.provider.raw.begin_audio.unwrap()(
                    instance.handle.unwrap().as_ptr(),
                    &mut token,
                    &mut result,
                )
            };
            match (status, NonNull::new(token)) {
                (0, Some(token)) => {
                    c.lease = Some(token);
                    handles.push(token);
                }
                (0, None) => {
                    failure = Some(BankError::Instance {
                        child,
                        error: InstanceError::Metadata,
                    });
                    break;
                }
                (status, _) => {
                    failure = Some(BankError::Instance {
                        child,
                        error: InstanceError::Native { status, result },
                    });
                    break;
                }
            }
        }
    }
    let Some(failure) = failure else {
        return Ok(handles);
    };
    slot.failure = Some(failure);
    // Reverse unwind: end every begun lease before any stop.
    if end_leases(slot).is_ok() {
        for c in slot.children.iter_mut().take(started).rev() {
            let _ = c.instance.as_mut().expect("prepared child").stop();
        }
        // No node escaped: every child is trivially returned.
        for c in &mut slot.children {
            c.returned = true;
        }
    }
    Err(failure)
}
/// End every live lease in reverse order. A failed end poisons that child and
/// keeps its lease; no later stop/retirement may proceed for this bank.
fn end_leases(slot: &mut BankSlot) -> Result<(), BankError> {
    for (child, c) in slot.children.iter_mut().enumerate().rev() {
        let Some(lease) = c.lease else { continue };
        let instance = c.instance.as_mut().expect("leased child");
        let mut result = 0;
        // SAFETY: lease returned or never escaped; MAIN-only native end.
        let status = unsafe {
            instance.library.provider.raw.end_audio.unwrap()(lease.as_ptr(), &mut result)
        };
        if status != 0 {
            instance.poisoned = true;
            c.end_failed = true;
            let error = BankError::Instance {
                child,
                error: InstanceError::Native { status, result },
            };
            slot.failure.get_or_insert(error);
            return Err(error);
        }
        c.lease = None;
        if c.end_failed {
            instance.poisoned = false;
            c.end_failed = false;
        }
    }
    Ok(())
}

/// Bounded reclamation outcome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReclaimReport {
    /// Banks fully released (leases ended, children and Library retired).
    pub released: u32,
    /// Banks still retained (live control, outstanding node, or failed cleanup).
    pub retained: u32,
    /// Node bundles gathered by this call.
    pub returned: u32,
    /// Children whose node refused its return slot; retained permanently.
    pub stranded: u32,
    /// First cleanup failure observed by this call.
    pub failure: Option<(BankId, BankError)>,
}

/// MAIN reclamation over at most 64 banks of 128 children, without waiting or
/// spinning. `Some(id)` restricts work to one bank. Gathers returned nodes
/// without native calls; for orphaned fully returned banks ends ALL leases,
/// then stops and retires children and finally the Library (module exit).
pub fn reclaim_vst3_graph_banks(bank: Option<BankId>) -> Result<ReclaimReport, BankError> {
    with_registry(|r| {
        let mut report = ReclaimReport::default();
        for index in 0..MAXIMUM_BANKS {
            let Some(slot) = r.banks[index].as_mut() else {
                continue;
            };
            if slot.reserved
                || bank.is_some_and(|b| b.slot as usize != index || b.generation != slot.generation)
            {
                continue;
            }
            let id = BankId {
                slot: index as u32,
                generation: slot.generation,
            };
            let mut outstanding = false;
            for c in &mut slot.children {
                if c.returned {
                    continue;
                }
                if c.shared.diagnostic.load(Ordering::Relaxed) & diagnostic::RETURN_REFUSED != 0 {
                    report.stranded += 1;
                    outstanding = true;
                    continue;
                }
                match c.returns.pop() {
                    Ok(bundle) => {
                        drop(bundle);
                        c.returned = true;
                        report.returned += 1;
                    }
                    Err(_) => outstanding = true,
                }
            }
            if !slot.orphaned.get() || outstanding {
                report.retained += 1;
                continue;
            }
            match release(slot) {
                Ok(()) => {
                    r.banks[index] = None;
                    report.released += 1;
                }
                Err(error) => {
                    report.retained += 1;
                    report.failure.get_or_insert((id, error));
                }
            }
        }
        report
    })
}
fn release(slot: &mut BankSlot) -> Result<(), BankError> {
    end_leases(slot)?;
    let mut first = None;
    for (child, c) in slot.children.iter_mut().enumerate().rev() {
        if let Some(instance) = c.instance.as_mut()
            && instance.running
            && let Err(error) = instance.stop()
        {
            first.get_or_insert(BankError::Instance { child, error });
        }
    }
    for (child, c) in slot.children.iter_mut().enumerate().rev() {
        let Some(instance) = c.instance.take() else {
            continue;
        };
        if let Err(failure) = instance.retire() {
            let error = BankError::Instance {
                child,
                error: failure.error,
            };
            c.instance = Some(failure.owner);
            slot.failure.get_or_insert(error);
            return Err(error);
        }
    }
    if let Some(library) = slot.library.take() {
        match Rc::try_unwrap(library) {
            Ok(library) => {
                if let Err(failure) = library.retire() {
                    slot.library = Some(Rc::new(failure.owner));
                    let error = BankError::Instance {
                        child: usize::MAX,
                        error: failure.error,
                    };
                    slot.failure.get_or_insert(error);
                    return Err(error);
                }
            }
            Err(library) => {
                slot.library = Some(library);
                return Err(BankError::Busy);
            }
        }
    }
    if let Some(error) = first {
        slot.failure.get_or_insert(error);
    }
    Ok(())
}

/// Copied child return status from [`Vst3GraphBankControl::poll_returns`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReturnStatus {
    pub returned: u32,
    pub outstanding: u32,
    pub stranded: u32,
}

/// Typed MAIN lane input for one submitted batch.
pub struct LaneInput<'a> {
    pub events: &'a mut InputEvents,
    pub points: &'a mut InputPoints,
    pub context: &'a mut Option<Context>,
}
/// One completed lane batch, re-validated on MAIN.
pub struct LaneOutput<'a> {
    pub sequence: u64,
    pub frames: u32,
    pub output: Result<Output<'a>, HostedError>,
}

/// MAIN-only disposable bank handle. Drop marks the bank orphaned; native
/// owners stay retained until [`reclaim_vst3_graph_banks`].
pub struct Vst3GraphBankControl {
    id: BankId,
    orphaned: Rc<Cell<bool>>,
    main: PhantomData<Rc<()>>,
}
impl Vst3GraphBankControl {
    pub fn id(&self) -> BankId {
        self.id
    }
    fn with_slot<R>(
        &self,
        f: impl FnOnce(&mut BankSlot) -> Result<R, BankError>,
    ) -> Result<R, BankError> {
        with_registry(|r| live_slot(r, self.id).and_then(f))?
    }
    pub fn children(&self) -> Result<usize, BankError> {
        self.with_slot(|s| Ok(s.children.len()))
    }
    /// Cached prepared profile; no native call.
    pub fn profile(&self, child: usize) -> Result<HostedAudioProfile, BankError> {
        self.with_slot(|s| {
            s.children
                .get(child)
                .map(|c| c.profile)
                .ok_or(BankError::Shape)
        })
    }
    /// Cached prepared latency/tail; no native call.
    pub fn timing(&self, child: usize) -> Result<Timing, BankError> {
        self.with_slot(|s| {
            s.children
                .get(child)
                .map(|c| c.timing)
                .ok_or(BankError::Shape)
        })
    }
    /// Copied diagnostic bits; see [`diagnostic`].
    pub fn diagnostic(&self, child: usize) -> Result<u32, BankError> {
        self.with_slot(|s| {
            s.children
                .get(child)
                .map(|c| c.shared.diagnostic.load(Ordering::Relaxed))
                .ok_or(BankError::Shape)
        })
    }
    /// First recorded bank failure, if any.
    pub fn failure(&self) -> Result<Option<BankError>, BankError> {
        self.with_slot(|s| Ok(s.failure))
    }
    /// Gather returned node bundles without native calls.
    pub fn poll_returns(&self) -> Result<ReturnStatus, BankError> {
        self.with_slot(|s| {
            let mut status = ReturnStatus::default();
            for c in &mut s.children {
                if !c.returned
                    && c.shared.diagnostic.load(Ordering::Relaxed) & diagnostic::RETURN_REFUSED == 0
                    && let Ok(bundle) = c.returns.pop()
                {
                    drop(bundle);
                    c.returned = true;
                }
                if c.returned {
                    status.returned += 1;
                } else if c.shared.diagnostic.load(Ordering::Relaxed) & diagnostic::RETURN_REFUSED
                    != 0
                {
                    status.stranded += 1;
                } else {
                    status.outstanding += 1;
                }
            }
            Ok(status)
        })
    }
    /// Submit one complete rich batch for `child` with the next exact sequence
    /// and frame count. Fill runs on MAIN; a fill error returns the batch.
    /// The registry is borrowed during `fill`: registry calls inside it fail
    /// with [`BankError::Busy`].
    pub fn submit(
        &self,
        child: usize,
        frames: u32,
        fill: impl FnOnce(&mut LaneInput<'_>) -> Result<(), processing::Error>,
    ) -> Result<u64, BankError> {
        if frames > 65536 {
            return Err(BankError::Shape);
        }
        self.with_slot(|s| {
            let lanes = s
                .children
                .get_mut(child)
                .ok_or(BankError::Shape)?
                .lanes
                .as_mut()
                .ok_or(BankError::Shape)?;
            let sequence = lanes.next;
            let next = sequence.checked_add(1).ok_or(BankError::Capacity)?;
            let mut batch = lanes.free.pop().ok_or(BankError::Empty)?;
            batch.events.clear();
            batch.points.clear();
            batch.context = None;
            batch.outcome = Outcome::Pending;
            let filled = fill(&mut LaneInput {
                events: &mut batch.events,
                points: &mut batch.points,
                context: &mut batch.context,
            });
            if let Err(error) = filled {
                lanes.free.push(batch);
                return Err(BankError::Processing { child, error });
            }
            batch.owner = lanes.owner;
            batch.sequence = sequence;
            batch.frames = frames;
            match lanes.ready.push(batch) {
                Ok(()) => {
                    lanes.next = next;
                    Ok(sequence)
                }
                Err(rtrb::PushError::Full(batch)) => {
                    lanes.free.push(batch);
                    Err(BankError::Capacity)
                }
            }
        })
    }
    /// Receive the next completed batch, re-validated against its own storage,
    /// then recycle it. `Ok(None)` when no batch has completed.
    pub fn receive<R>(
        &self,
        child: usize,
        read: impl FnOnce(LaneOutput<'_>) -> R,
    ) -> Result<Option<R>, BankError> {
        self.with_slot(|s| {
            let lanes = s
                .children
                .get_mut(child)
                .ok_or(BankError::Shape)?
                .lanes
                .as_mut()
                .ok_or(BankError::Shape)?;
            let Ok(mut batch) = lanes.done.pop() else {
                return Ok(None);
            };
            let result = {
                let b = &mut *batch;
                let output = match b.outcome {
                    Outcome::Published(report) => {
                        let raw = report.raw();
                        let mut wire = b.output.wire();
                        wire.event_count = raw.event_count;
                        wire.payload_used = raw.payload_used;
                        wire.parameter_queue_count = raw.parameter_queue_count;
                        wire.parameter_point_count = raw.parameter_point_count;
                        b.output
                            .view(&wire, b.frames, &lanes.buses.output, report)
                            .map_err(HostedError::Processing)
                    }
                    Outcome::Failed(error) => Err(error),
                    Outcome::Pending => Err(HostedError::Lifecycle),
                };
                read(LaneOutput {
                    sequence: b.sequence,
                    frames: b.frames,
                    output,
                })
            };
            lanes.free.push(batch);
            Ok(Some(result))
        })
    }
    /// Explicitly orphan and attempt release of this bank now.
    pub fn retire(self) -> Result<ReclaimReport, BankError> {
        self.orphaned.set(true);
        reclaim_vst3_graph_banks(Some(self.id))
    }
}
impl Drop for Vst3GraphBankControl {
    fn drop(&mut self) {
        // No registry borrow or native call: safe during any MAIN callback.
        self.orphaned.set(true);
    }
}

/// Owned canonical Graph node for one hosted child. Packed channel layout is
/// the prepared [`super::PackedAudioMap`]; latency/tail are cached on MAIN.
pub struct Vst3GraphNode<S: HostedSample> {
    bundle: Option<Box<Bundle<S>>>,
    returns: rtrb::Producer<Box<dyn Any + Send>>,
    shared: Arc<Shared>,
    layout: AudioLayout,
    sample_rate: f64,
    maximum_frames: usize,
    timing: Timing,
    prepared: Option<usize>,
    fault: Option<ProcessError>,
    no_sync: PhantomData<Cell<()>>,
}
impl<S: HostedSample> Vst3GraphNode<S> {
    /// Packed node layout for [`ravet_foundation`] Graph construction.
    pub fn layout(&self) -> AudioLayout {
        self.layout
    }
    pub fn timing(&self) -> Timing {
        self.timing
    }
    fn refuse(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, S>,
        bits: u32,
        error: ProcessError,
    ) -> ProcessStatus
    where
        S: Sample,
    {
        self.shared.mark(bits);
        self.fault.get_or_insert(error);
        block.clear_outputs();
        ProcessStatus::Fault(error)
    }
}
impl<S: HostedSample + Sample> Processor<S> for Vst3GraphNode<S> {
    /// Admits only the exact prepared rate, packed layout and a maximum block
    /// no larger than the prepared one. No foreign call is made.
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.prepared = None;
        if config.sample_rate() != self.sample_rate {
            return Err(PrepareError::InvalidSampleRate);
        }
        if config.maximum_frames() > self.maximum_frames {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        if config.layout() != self.layout || !config.auxiliary_buses().is_empty() {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.prepared = Some(config.maximum_frames());
        Ok(())
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        if self.prepared.is_none() {
            return Err(ProcessError::NotPrepared);
        }
        match self.fault {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        let Some(maximum) = self.prepared else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::NotPrepared);
        };
        if let Some(error) = self.fault {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let frames = block.audio.frames();
        if frames > maximum {
            return self.refuse(
                block,
                diagnostic::ADMISSION,
                ProcessError::TooManyFrames { frames, maximum },
            );
        }
        let inputs = self.layout.input_channels();
        let outputs = self.layout.output_channels();
        if block.audio.input_channels() != inputs
            || block.audio.output_channels() != outputs
            || block.auxiliary.is_some()
        {
            return self.refuse(block, diagnostic::ADMISSION, ProcessError::LayoutMismatch);
        }
        if !block.events.is_empty() {
            let count = block.events.len();
            return self.refuse(
                block,
                diagnostic::EVENTS_REFUSED,
                ProcessError::Events(EventError::TooMany { count, maximum: 0 }),
            );
        }
        let bundle = &mut **self.bundle.as_mut().expect("live node bundle");
        if let Some(lanes) = &bundle.lanes {
            let bits = match lanes.ready.peek() {
                Err(_) => diagnostic::LANE_MISSING,
                Ok(head) if head.owner != lanes.owner || head.sequence != lanes.next => {
                    diagnostic::LANE_SEQUENCE
                }
                Ok(head) if head.frames as usize != frames => diagnostic::LANE_FRAMES,
                Ok(_) if lanes.done.slots() == 0 || lanes.next == u64::MAX => {
                    diagnostic::DONE_REFUSED
                }
                Ok(_) => 0,
            };
            if bits != 0 {
                return self.refuse(block, bits, ProcessError::NotPrepared);
            }
        }
        let stride = bundle.stride;
        for ch in 0..inputs {
            let source = block.audio.input(ch).expect("checked input channel");
            bundle.scratch[ch * stride..ch * stride + frames].copy_from_slice(&source[..frames]);
        }
        let mut native_inputs = [InChannel::<S> {
            samples: std::ptr::null(),
            capacity: 0,
        }; MAXIMUM_CHANNELS];
        for (ch, d) in native_inputs.iter_mut().enumerate().take(inputs) {
            *d = InChannel {
                samples: bundle.scratch[ch * stride..].as_ptr(),
                capacity: stride as u32,
            };
        }
        let mut native_outputs = [OutChannel::<S> {
            samples: std::ptr::null_mut(),
            capacity: 0,
        }; MAXIMUM_CHANNELS];
        for (ch, d) in native_outputs.iter_mut().enumerate().take(outputs) {
            let samples = block.audio.output_mut(ch).expect("checked output channel");
            *d = OutChannel {
                samples: samples.as_mut_ptr(),
                capacity: samples.len() as u32,
            };
        }
        let handle = bundle.handle.as_ptr();
        let (failed, retire) = match bundle.lanes.as_mut() {
            None => {
                let (result, retire) = dispatch(
                    &bundle.state,
                    handle,
                    &bundle.buses,
                    &native_inputs[..inputs],
                    &native_outputs[..outputs],
                    frames as u32,
                    &Input {
                        points: &[],
                        events: &bundle.empty,
                        context: None,
                    },
                    &mut bundle.discard,
                );
                (result.is_err(), retire)
            }
            Some(lanes) => {
                let mut batch = lanes.ready.pop().expect("checked head batch");
                let (outcome, retire) = {
                    let b = &mut *batch;
                    let (result, retire) = dispatch(
                        &bundle.state,
                        handle,
                        &bundle.buses,
                        &native_inputs[..inputs],
                        &native_outputs[..outputs],
                        frames as u32,
                        &Input {
                            points: b.points.points(),
                            events: &b.events,
                            context: b.context.as_ref(),
                        },
                        &mut b.output,
                    );
                    let outcome = match result {
                        Ok(output) => Outcome::Published(output.report()),
                        Err(error) => Outcome::Failed(error),
                    };
                    (outcome, retire)
                };
                let failed = !matches!(outcome, Outcome::Published(_));
                batch.outcome = outcome;
                lanes.next += 1;
                if let Err(rtrb::PushError::Full(batch)) = lanes.done.push(batch) {
                    // Reserved capacity was checked; never free on AUDIO.
                    std::mem::forget(batch);
                    self.shared.mark(diagnostic::DONE_REFUSED);
                }
                (failed, retire)
            }
        };
        if failed {
            let bits = if retire {
                diagnostic::NATIVE_FAULT
            } else {
                diagnostic::ADMISSION
            };
            return self.refuse(block, bits, ProcessError::BackendFailure);
        }
        ProcessStatus::Continue
    }
    /// Records the rebuild requirement; never promises cleared history.
    fn reset(&mut self) {
        let _ = self.try_reset();
    }
    /// Generic hosted VST3 has no guaranteed SDK history reset: refuses with
    /// [`ProcessError::RebuildRequired`] without foreign calls and stays faulted.
    fn try_reset(&mut self) -> Result<(), ProcessError> {
        self.shared.mark(diagnostic::RESET_REFUSED);
        self.fault = Some(ProcessError::RebuildRequired);
        Err(ProcessError::RebuildRequired)
    }
    /// Graph-level deactivation only; native lifecycle ends through MAIN reclaim.
    fn deactivate(&mut self) {
        self.prepared = None;
    }
    fn latency_samples(&self) -> u32 {
        self.timing.latency_samples
    }
    fn tail(&self) -> Tail {
        self.timing.tail
    }
}
impl<S: HostedSample> Drop for Vst3GraphNode<S> {
    /// Moves the whole bundle into the dedicated return slot: no SDK teardown,
    /// no buffer deallocation. A refused slot retains (leaks) the bundle and
    /// marks the bank permanently stranded instead of freeing it.
    fn drop(&mut self) {
        if let Some(bundle) = self.bundle.take() {
            let erased: Box<dyn Any + Send> = bundle;
            if let Err(rtrb::PushError::Full(erased)) = self.returns.push(erased) {
                self.shared.mark(diagnostic::RETURN_REFUSED);
                std::mem::forget(erased);
            }
        }
    }
}

#[cfg(test)]
mod tests;
