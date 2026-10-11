//! Typed ownership facade over the canonical prepared Graph scheduler.
//!
//! Status: draft, isolated packet (ADR 0260 is proposed, not integrated). Nothing here
//! claims public availability, workspace-lint cleanliness or a Miri pass; the
//! provenance arguments in the `SAFETY` comments below are reasoning only.
use crate::{
    Connection, Destination, Graph, GraphAudioSession, GraphNode, GraphSample, GraphStatus,
    MAXIMUM_GRAPH_EVENTS, NodeId, Source,
};
use ravet_foundation::{
    AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor,
    Tail,
};
use std::{cell::UnsafeCell, marker::PhantomData, ptr::NonNull};
const OWNER_BYTES: usize = 4 * 1024 * 1024;

/// Constructor admission failures. The failure retains the exact original tuple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypedChainError {
    Layout,
    Frames,
    Events,
    Capacity,
    Graph(crate::GraphError),
}
/// No processor is prepared or consumed on an ordinary constructor refusal.
pub struct TypedChainFailure<T> {
    pub stages: T,
    pub error: TypedChainError,
}
/// Sealed heterogeneous processor tuples of arity 1..8.
/// Private-construction permits keep internal owner/wrapper operations inaccessible
/// even when a generic caller has this public supertrait bound.
///
/// The internal `store` helper takes a mandatory private `Permit` argument that no
/// external code can construct. Calling it with only the tuple therefore fails with
/// E0061 (wrong argument count). That diagnostic exists only because of the Permit
/// parameter: if the parameter were removed from `store`, the same call would
/// type-check, so this negative example discriminates the safeguard.
///
/// ```compile_fail,E0061
/// use ravet_graph::StageTuple;
/// fn escape<T: StageTuple<f32>>(stages: T) {
///     let _ = T::store(stages);
/// }
/// ```
///
/// Positive companion: the generic bound itself is valid and usable.
///
/// ```
/// use ravet_graph::StageTuple;
/// fn bound_is_valid<T: StageTuple<f32>>(stages: T) -> T {
///     stages
/// }
/// let _ = bound_is_valid::<(ravet_gain::GainProcessor,)>;
/// ```
pub trait StageTuple<S: GraphSample>: sealed::Tuple<S> {}
/// Statically indexed concrete stage type. Unsupported indices do not implement this trait.
pub trait StageAt<S: GraphSample, const I: usize>:
    StageTuple<S> + sealed::Access<S, I, P = Self::Processor>
{
    type Processor: Processor<S> + Send + 'static;
}

pub struct Record<P> {
    processor: P,
    bypassed: bool,
}
mod sealed {
    use super::*;
    // Private-construction capability: public sealed supertrait methods cannot
    // expose storage, clone wrappers, or disclose pointers through a generic bound.
    pub struct Permit {
        _private: (),
    }
    pub(super) fn permit() -> Permit {
        Permit { _private: () }
    }
    pub struct Stable<P> {
        pub(super) cells: Vec<UnsafeCell<Record<P>>>,
    }
    impl<P> Stable<P> {
        /// Reserve exactly one element. The allocation-failure branch is not
        /// reachable in ordinary tests and has no fault-injection coverage; it
        /// returns `Capacity` and the caller recovers the original tuple.
        fn allocate() -> Result<Self, TypedChainError> {
            let mut cells = Vec::new();
            cells
                .try_reserve_exact(1)
                .map_err(|_| TypedChainError::Capacity)?;
            Ok(Self { cells })
        }
        fn capacity_bytes(&self) -> Option<usize> {
            self.cells
                .capacity()
                .checked_mul(std::mem::size_of::<UnsafeCell<Record<P>>>())
        }
        /// Sole source of every record pointer (wrappers and getters).
        ///
        /// `Vec` indexing yields a shared `&UnsafeCell<Record<P>>` over the heap
        /// element and `UnsafeCell::get` turns it into a write-capable raw pointer
        /// to that same element. No pointer is ever derived from `&mut Vec`,
        /// `get_mut` or `as_mut_ptr`, so no unique tag over the element exists
        /// alongside the shared-read-write pointers. The element's address is
        /// stable: one push in `store` before any pointer exists, no later
        /// push/pop/reallocation until `recover`, which runs after the Graph (and
        /// every wrapper) is gone. Moving the `Stable` value moves only the Vec
        /// header, not the heap element.
        fn pointer(&self) -> *mut Record<P> {
            self.cells[0].get()
        }
        fn recover(mut self) -> P {
            self.cells.pop().expect("one cell").into_inner().processor
        }
    }
    pub trait Tuple<S: GraphSample>: Sized {
        type Storage;
        const COUNT: usize;
        /// Allocate one record cell per stage before moving any processor.
        /// Pre-store refusals return the original tuple unchanged. The post-
        /// reservation `actual > OWNER_BYTES` branch cannot trigger for an exact
        /// one-element reservation and the allocator-failure branch for tuple
        /// index greater than zero is not fault-injectable here; both are kept as
        /// defensive refusals without dedicated tests.
        fn store(self, _permit: Permit) -> Result<Self::Storage, TypedChainFailure<Self>>;
        fn nodes(
            storage: &Self::Storage,
            layout: AudioLayout,
            _permit: Permit,
        ) -> Vec<GraphNode<S>>;
        fn recover(storage: Self::Storage, _permit: Permit) -> Self;
    }
    pub trait Access<S: GraphSample, const I: usize>: Tuple<S> {
        type P: Processor<S> + Send + 'static;
        fn pointer(storage: &Self::Storage, _permit: Permit) -> *mut Record<Self::P>;
    }
    macro_rules! tuple {
        ($( $p:ident : $i:tt ),+) => {
            impl<S: GraphSample, $($p: Processor<S> + Send + 'static),+> Tuple<S> for ($($p,)+) {
                type Storage = ($(Stable<$p>,)+);
                const COUNT: usize = [$(stringify!($p)),+].len();
                #[allow(non_snake_case)]
                fn store(self, _permit: Permit) -> Result<Self::Storage, TypedChainFailure<Self>> {
                    let declared = 0usize $(.checked_add(std::mem::size_of::<UnsafeCell<Record<$p>>>()).unwrap_or(usize::MAX))+;
                    if declared > OWNER_BYTES { return Err(TypedChainFailure { stages: self, error: TypedChainError::Capacity }); }
                    $(let $p = match Stable::<$p>::allocate() { Ok(v) => v, Err(error) => return Err(TypedChainFailure { stages: self, error }) };)+
                    let actual = 0usize $(.checked_add($p.capacity_bytes().unwrap_or(usize::MAX)).unwrap_or(usize::MAX))+;
                    if actual > OWNER_BYTES { return Err(TypedChainFailure { stages: self, error: TypedChainError::Capacity }); }
                    let ($($p,)+) = ($(($p, self.$i),)+);
                    $(let (mut $p, processor) = $p; $p.cells.push(UnsafeCell::new(Record { processor, bypassed: false }));)+
                    Ok(($($p,)+))
                }
                fn nodes(storage: &Self::Storage, layout: AudioLayout, _permit: Permit) -> Vec<GraphNode<S>> {
                    vec![$(GraphNode { id: NodeId($i), layout, processor: Box::new(Wrapper::<S, $p> { record: NonNull::new(storage.$i.pointer()).expect("cell"), sample: PhantomData }) }),+]
                }
                fn recover(storage: Self::Storage, _permit: Permit) -> Self { ($(storage.$i.recover(),)+) }
            }
            impl<S: GraphSample, $($p: Processor<S> + Send + 'static),+> StageTuple<S> for ($($p,)+) {}
        };
    }
    tuple!(A:0);
    tuple!(A:0,B:1);
    tuple!(A:0,B:1,C:2);
    tuple!(A:0,B:1,C:2,D:3);
    tuple!(A:0,B:1,C:2,D:3,E:4);
    tuple!(A:0,B:1,C:2,D:3,E:4,F:5);
    tuple!(A:0,B:1,C:2,D:3,E:4,F:5,G:6);
    tuple!(A:0,B:1,C:2,D:3,E:4,F:5,G:6,H:7);
    macro_rules! access {
        (($($p:ident),+), $n:tt, $selected:ident) => {
            impl<S: GraphSample, $($p: Processor<S> + Send + 'static),+> Access<S,$n> for ($($p,)+) {
                type P = $selected;
                fn pointer(storage: &Self::Storage, _permit: Permit) -> *mut Record<$selected> { storage.$n.pointer() }
            }
            impl<S: GraphSample, $($p: Processor<S> + Send + 'static),+> StageAt<S,$n> for ($($p,)+) { type Processor = $selected; }
        }
    }
    // One explicit implementation per valid tuple index; no runtime downcast.
    access!((A), 0, A);
    access!((A, B), 0, A);
    access!((A, B), 1, B);
    access!((A, B, C), 0, A);
    access!((A, B, C), 1, B);
    access!((A, B, C), 2, C);
    access!((A, B, C, D), 0, A);
    access!((A, B, C, D), 1, B);
    access!((A, B, C, D), 2, C);
    access!((A, B, C, D), 3, D);
    access!((A, B, C, D, E), 0, A);
    access!((A, B, C, D, E), 1, B);
    access!((A, B, C, D, E), 2, C);
    access!((A, B, C, D, E), 3, D);
    access!((A, B, C, D, E), 4, E);
    access!((A, B, C, D, E, F), 0, A);
    access!((A, B, C, D, E, F), 1, B);
    access!((A, B, C, D, E, F), 2, C);
    access!((A, B, C, D, E, F), 3, D);
    access!((A, B, C, D, E, F), 4, E);
    access!((A, B, C, D, E, F), 5, F);
    access!((A, B, C, D, E, F, G), 0, A);
    access!((A, B, C, D, E, F, G), 1, B);
    access!((A, B, C, D, E, F, G), 2, C);
    access!((A, B, C, D, E, F, G), 3, D);
    access!((A, B, C, D, E, F, G), 4, E);
    access!((A, B, C, D, E, F, G), 5, F);
    access!((A, B, C, D, E, F, G), 6, G);
    access!((A, B, C, D, E, F, G, H), 0, A);
    access!((A, B, C, D, E, F, G, H), 1, B);
    access!((A, B, C, D, E, F, G, H), 2, C);
    access!((A, B, C, D, E, F, G, H), 3, D);
    access!((A, B, C, D, E, F, G, H), 4, E);
    access!((A, B, C, D, E, F, G, H), 5, F);
    access!((A, B, C, D, E, F, G, H), 6, G);
    access!((A, B, C, D, E, F, G, H), 7, H);
}
/// Private graph node adapter for exactly one record cell.
///
/// Invariants relied on by every `SAFETY` comment below:
/// - W1 (validity): `record` was produced by `Stable::pointer` from the cell that
///   the facade's `storage` owns. That heap element is not moved, resized or
///   popped while the Graph holding this wrapper exists, and the Graph is always
///   dropped before `storage` is recovered or dropped (field order, `into_stages`).
///   The wrapper has no `Drop` and never reads or destroys the processor.
/// - W2 (provenance): the pointer is a shared-read-write `UnsafeCell::get`
///   pointer. The facade's getters derive their pointers the same way, and the
///   `&mut Storage` held by a session covers only the Vec headers, never the
///   heap records, so no unique tag over a record is created beside this pointer.
/// - W3 (exclusivity): a wrapper method runs only from a `Graph` call, and a
///   Graph is reachable only through `&mut TypedChain` or the session's
///   `&mut self`. Every facade getter (`&P`, `&mut P`, bypass flag) needs a
///   conflicting borrow of that same chain or session, so none can be live then,
///   and no reference to a record is stored between calls.
///
/// These are reasoning-based arguments; no Miri run supports them.
struct Wrapper<S, P> {
    record: NonNull<Record<P>>,
    sample: PhantomData<S>,
}
// SAFETY: unique private wrapper, cell allocation never moves; every invocation
// is under the exclusive enclosing Graph/facade/session borrow. P is transferred
// only when P: Send. No Sync promise, independently callable wrapper or clone.
// The record is reached only through this wrapper while the Graph runs (W3), so
// sending the wrapper with the Graph transfers the only live access path.
unsafe impl<S, P: Send> Send for Wrapper<S, P> {}
impl<S: GraphSample, P: Processor<S> + Send> Processor<S> for Wrapper<S, P> {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        // SAFETY: W1 keeps `record` valid; W3 makes this `&mut Record<P>` the only
        // live reference to the record for the duration of this call (`&mut self`
        // plus the exclusive Graph borrow). It is a temporary and is not stored.
        unsafe { self.record.as_mut().processor.prepare(c) }
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        // SAFETY: as in `prepare`: W1 validity, W3 exclusivity, temporary `&mut`.
        unsafe { self.record.as_mut().processor.start_audio() }
    }
    fn stop_audio(&mut self) {
        // SAFETY: as in `prepare`. A manual stop runs from `&mut TypedChain`, so
        // no getter borrow can overlap it.
        unsafe { self.record.as_mut().processor.stop_audio() }
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        // SAFETY: all record pointers derive from the same UnsafeCell::get (W2). This
        // temporary unique reference ends before any facade getter is available;
        // callers hold `&mut` of the chain or session for the whole Graph call (W3),
        // so neither `stage()`, `stage_mut_stopped()` nor the bypass accessors can
        // be live. The reference is not stored and does not outlive this function.
        let record = unsafe { self.record.as_mut() };
        if !record.bypassed {
            return record.processor.process(b);
        }
        for channel in 0..b.audio.output_channels() {
            for frame in 0..b.audio.frames() {
                let x = b.audio.input(channel).expect("canonical validated effect")[frame];
                b.audio
                    .output_mut(channel)
                    .expect("canonical validated effect")[frame] = x;
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        let _ = self.try_reset();
    }
    fn try_reset(&mut self) -> Result<(), ProcessError> {
        // SAFETY: W1 validity and W3 exclusive temporary access, as in prepare.
        unsafe { self.record.as_mut().processor.try_reset() }
    }
    fn deactivate(&mut self) {
        // SAFETY: as in `prepare`. This is also reached from `Graph::drop`, which
        // runs before `storage` is dropped (W1), so the record is still allocated.
        unsafe { self.record.as_mut().processor.deactivate() }
    }
    fn latency_samples(&self) -> u32 {
        // SAFETY: W1 validity. Only the Graph calls this (during prepare, under its
        // `&mut` borrow, W3), so a shared reference to the record cannot overlap
        // a mutable one; it is temporary.
        unsafe { self.record.as_ref().processor.latency_samples() }
    }
    fn tail(&self) -> Tail {
        // SAFETY: as in `latency_samples`.
        unsafe { self.record.as_ref().processor.tail() }
    }
}
/// Typed, effect-only serial Graph with 1..8 sealed concrete stages.
///
/// Record-Vec payload is checked against 4MiB before/after reservation; existing
/// Graph allocations and arbitrary processor-owned storage have separate limits.
/// The internal graph drops before the stable cells, including during unwind.
/// Readonly and stopped-exclusive getters derive from the same cell provenance
/// as callbacks. No references are retained across callback/getter boundaries.
/// Bypass freezes a child but retains its declared latency/tail: audible bypass
/// is **not** latency compensated. Generated output events and auxiliary buses
/// are rejected; accepted input events use canonical Graph broadcast.
///
/// # Audio sessions: prefer the guard
///
/// The inherent `start_audio()` returns a non-Send guard that stops the owner on
/// drop (reverse stage order) on the starting thread. Prefer it. A forgotten guard
/// (`mem::forget`, a leak) is **misuse, not valid use**: it leaves the audio owner
/// live exactly like an unpaired manual start (see below), and the guard that
/// would have stopped it no longer exists.
///
/// # Manual `Processor` hooks are a caller obligation
///
/// `Processor::start_audio` on this chain is the canonical manual hook; it returns
/// no guard. The caller must pair it with `Processor::stop_audio` **on the thread
/// that started it** before transfer to another thread, `deactivate`, `into_stages`
/// or drop. Unlike the guard, nothing stops a manual owner automatically: the
/// facade adds no owner-thread cleanup, no thread check at `process`, and a
/// manually started chain is `Send`. While a manual owner (or a forgotten guard)
/// is live:
///
/// - `prepare` and `stage_mut_stopped` **refuse** with `AudioSessionActive`;
/// - `deactivate` (inherent and `Processor`), `into_stages` and `Drop` do **not**
///   refuse: they **panic** through the canonical Graph assertion;
/// - `Processor::stop_audio` called on another thread panics and leaves the owner
///   live, so it can still be stopped on the owning thread afterwards;
/// - `process`, `reset`, getters and bypass flags keep working.
///
/// Unwind caveats, with no safeguard claimed. A caught `deactivate` panic happens
/// before any state change: the chain stays prepared with the owner live, and the
/// owner thread can still `stop_audio`. A panic from `into_stages` or `Drop`
/// destroys the facade while unwinding: the Graph is dropped, then each stage is
/// dropped exactly once by field drop glue, **without** `stop_audio` or
/// `deactivate`, and no owners are returned. If that panic happens while the
/// thread is already unwinding, the second panic aborts the process; the facade
/// does not prevent that.
///
/// # Compile-time exclusions
///
/// Unsupported static index:
///
/// ```compile_fail,E0277
/// use ravet_graph::TypedChain;
/// use ravet_gain::GainProcessor;
/// let chain = TypedChain::<f32,_>::new((GainProcessor::new(0.).unwrap(),),1,64,0).ok().unwrap();
/// chain.stage::<1>(); // tuple1 has index0 only
/// ```
/// Positive companion (valid index):
///
/// ```
/// use ravet_graph::TypedChain;
/// use ravet_gain::GainProcessor;
/// let chain = TypedChain::<f32,_>::new((GainProcessor::new(0.).unwrap(),),1,64,0).ok().unwrap();
/// let _ = chain.stage::<0>();
/// ```
/// A readonly stage borrow excludes `set_bypassed`:
///
/// ```compile_fail,E0502
/// use ravet_graph::{StageAt,TypedChain};
/// use ravet_foundation::Processor;
/// fn overlap<T: StageAt<f32,0>>(chain:&mut TypedChain<f32,T>) {
///     let stage=chain.stage::<0>();
///     chain.set_bypassed::<0>(true);
///     let _=stage.latency_samples();
/// }
/// ```
/// A readonly stage borrow excludes `process`:
///
/// ```compile_fail,E0502
/// use ravet_graph::{StageAt,TypedChain};
/// use ravet_foundation::{ProcessBlock,Processor};
/// fn overlap<T: StageAt<f32,0>>(chain:&mut TypedChain<f32,T>, block:&mut ProcessBlock<'_,'_,'_,f32>) {
///     let stage=chain.stage::<0>();
///     let _=chain.process(block);
///     let _=stage.latency_samples();
/// }
/// ```
/// Positive companion: finish the readonly borrow before mutating or rendering.
///
/// ```
/// use ravet_graph::{StageAt,TypedChain};
/// use ravet_foundation::{ProcessBlock,Processor};
/// fn ordered<T: StageAt<f32,0>>(chain:&mut TypedChain<f32,T>, block:&mut ProcessBlock<'_,'_,'_,f32>) {
///     let latency=chain.stage::<0>().latency_samples();
///     chain.set_bypassed::<0>(true);
///     let _=chain.process(block);
///     let _=latency;
/// }
/// ```
/// A live audio session excludes the stopped mutable getter:
///
/// ```compile_fail,E0499
/// use ravet_graph::{StageAt,TypedChain};
/// use ravet_foundation::Processor;
/// fn overlap<T: StageAt<f32,0>>(chain:&mut TypedChain<f32,T>) {
///     let session=chain.start_audio().ok().unwrap();
///     let _=chain.stage_mut_stopped::<0>();
///     drop(session);
/// }
/// ```
/// Positive companion: drop the session first.
///
/// ```
/// use ravet_graph::{StageAt,TypedChain};
/// fn ordered<T: StageAt<f32,0>>(chain:&mut TypedChain<f32,T>) {
///     let session=chain.start_audio().ok().unwrap();
///     drop(session);
///     let _=chain.stage_mut_stopped::<0>();
/// }
/// ```
/// A live `&mut P` from the stopped getter excludes `process`:
///
/// ```compile_fail,E0499
/// use ravet_graph::{StageAt,TypedChain};
/// use ravet_foundation::{ProcessBlock,Processor};
/// fn overlap<T: StageAt<f32,0>>(chain:&mut TypedChain<f32,T>, block:&mut ProcessBlock<'_,'_,'_,f32>) {
///     let stage=chain.stage_mut_stopped::<0>().ok().unwrap();
///     let _=chain.process(block);
///     let _=stage.latency_samples();
/// }
/// ```
/// ... excludes a second `stage` borrow:
///
/// ```compile_fail,E0502
/// use ravet_graph::{StageAt,TypedChain};
/// use ravet_foundation::Processor;
/// fn overlap<T: StageAt<f32,0>>(chain:&mut TypedChain<f32,T>) {
///     let stage=chain.stage_mut_stopped::<0>().ok().unwrap();
///     let _=chain.stage::<0>();
///     let _=stage.latency_samples();
/// }
/// ```
/// ... and excludes `prepare`:
///
/// ```compile_fail,E0499
/// use ravet_graph::{StageAt,TypedChain};
/// use ravet_foundation::{ProcessConfig,Processor};
/// fn overlap<T: StageAt<f32,0>>(chain:&mut TypedChain<f32,T>, config:ProcessConfig) {
///     let stage=chain.stage_mut_stopped::<0>().ok().unwrap();
///     let _=chain.prepare(config);
///     let _=stage.latency_samples();
/// }
/// ```
/// Positive companion: use the exclusive stage, then prepare.
///
/// ```
/// use ravet_graph::{StageAt,TypedChain};
/// use ravet_foundation::{ProcessConfig,Processor};
/// fn ordered<T: StageAt<f32,0>>(chain:&mut TypedChain<f32,T>, config:ProcessConfig) {
///     let stage=chain.stage_mut_stopped::<0>().ok().unwrap();
///     let _=stage.latency_samples();
///     let _=chain.prepare(config);
/// }
/// ```
/// The chain is `Send` (for `Send` stages) but not `Sync`:
///
/// ```compile_fail,E0277
/// use ravet_graph::TypedChain;
/// use ravet_gain::GainProcessor;
/// fn sync<T:Sync>() {}
/// sync::<TypedChain<f32,(GainProcessor,)>>();
/// ```
///
/// ```
/// use ravet_graph::TypedChain;
/// use ravet_gain::GainProcessor;
/// fn send<T:Send>() {}
/// send::<TypedChain<f32,(GainProcessor,)>>();
/// ```
pub struct TypedChain<S: GraphSample, T: StageTuple<S>> {
    graph: Option<Graph<S>>, // MUST drop before storage, including unwinding.
    storage: Option<T::Storage>,
    layout: AudioLayout,
    maximum_frames: usize,
    maximum_events: usize,
    first_fault: Option<GraphStatus>,
}
impl<S: GraphSample, T: StageTuple<S>> TypedChain<S, T> {
    /// Admission checks return the original tuple before any processor is moved.
    /// The `Graph::new` error recovery below is not reachable for at most eight
    /// nodes and 576 connections; it is retained as defensive recovery and has no
    /// dedicated test.
    pub fn new(
        stages: T,
        channels: usize,
        maximum_frames: usize,
        maximum_events: usize,
    ) -> Result<Self, TypedChainFailure<T>>
    where
        S: From<f32>,
    {
        let error = if channels == 0 || channels > 64 {
            Some(TypedChainError::Layout)
        } else if maximum_frames == 0 || maximum_frames > 65536 {
            Some(TypedChainError::Frames)
        } else if maximum_events > MAXIMUM_GRAPH_EVENTS {
            Some(TypedChainError::Events)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(TypedChainFailure { stages, error });
        }
        let layout = AudioLayout::new(channels, channels).expect("admitted");
        let storage = T::store(stages, sealed::permit())?;
        let nodes = T::nodes(&storage, layout, sealed::permit());
        let mut connections = Vec::with_capacity(channels * (T::COUNT + 1));
        for c in 0..channels {
            connections.push(Connection {
                source: Source::Input(c),
                destination: Destination::Node {
                    id: NodeId(0),
                    channel: c,
                },
                gain: S::from(1.0f32),
            });
            for i in 1..T::COUNT {
                connections.push(Connection {
                    source: Source::Node {
                        id: NodeId((i - 1) as u32),
                        channel: c,
                    },
                    destination: Destination::Node {
                        id: NodeId(i as u32),
                        channel: c,
                    },
                    gain: S::from(1.0f32),
                });
            }
            connections.push(Connection {
                source: Source::Node {
                    id: NodeId((T::COUNT - 1) as u32),
                    channel: c,
                },
                destination: Destination::Output(c),
                gain: S::from(1.0f32),
            });
        }
        match Graph::new(layout, nodes, &connections) {
            Ok(graph) => Ok(Self {
                graph: Some(graph),
                storage: Some(storage),
                layout,
                maximum_frames,
                maximum_events,
                first_fault: None,
            }),
            Err(error) => Err(TypedChainFailure {
                stages: T::recover(storage, sealed::permit()),
                error: TypedChainError::Graph(error),
            }),
        }
    }
    /// Borrow a concrete stage. This borrow excludes all rendering and mutation.
    pub fn stage<const I: usize>(&self) -> &<T as StageAt<S, I>>::Processor
    where
        T: StageAt<S, I>,
    {
        // SAFETY: immutable enclosing borrow prevents every mutable callback and
        // getter; same UnsafeCell::get pointer as unique wrapper, temporary &P.
        // `&self` keeps the Graph unreachable (every Graph call needs `&mut self`),
        // so no wrapper `&mut Record` exists; the returned `&P` covers only the
        // `processor` field and its lifetime is tied to this `&self`. The
        // `&Storage` borrowed here covers the Vec header, not the heap record, which
        // is reached only through the cell pointer (W2). Several `&P` may coexist.
        // A stage reference may be shared across threads when P: Sync; its live
        // enclosing borrow still excludes every Graph callback and mutable getter.
        // The chain itself is `!Sync`, so the whole facade cannot be shared.
        unsafe { &(*T::pointer(self.storage.as_ref().expect("owned"), sealed::permit())).processor }
    }
    /// Deactivate CONTROL first; successful reprepare is required after mutation.
    /// Refuses (`AudioSessionActive`) while a manual or forgotten audio owner is live.
    pub fn stage_mut_stopped<const I: usize>(
        &mut self,
    ) -> Result<&mut <T as StageAt<S, I>>::Processor, PrepareError>
    where
        T: StageAt<S, I>,
    {
        if self.graph.as_ref().expect("owned").audio_owner.is_some() {
            return Err(PrepareError::AudioSessionActive);
        }
        self.deactivate();
        // SAFETY: `&mut self` is held for the returned lifetime, so no Graph call
        // (hence no wrapper `&mut Record`), getter or bypass accessor can overlap
        // the `&mut P`. No audio owner is live (checked above) and the graph has
        // just been deactivated. The pointer is a fresh `UnsafeCell::get` pointer
        // (W2); older wrapper pointers are not used while this borrow is live and
        // are only used again after it ends. The `&mut P` covers the `processor`
        // field only, never the heap record's `bypassed` flag or the Vec header.
        Ok(unsafe {
            &mut (*T::pointer(self.storage.as_ref().expect("owned"), sealed::permit())).processor
        })
    }
    pub fn set_bypassed<const I: usize>(&mut self, value: bool)
    where
        T: StageAt<S, I>,
    {
        // SAFETY: `&mut self` excludes every Graph call and getter. The write goes
        // through the `UnsafeCell::get` pointer to the `bypassed` field only; no
        // reference to the record or the processor is created or live.
        unsafe {
            (*T::pointer(self.storage.as_ref().expect("owned"), sealed::permit())).bypassed = value;
        }
    }
    pub fn is_bypassed<const I: usize>(&self) -> bool
    where
        T: StageAt<S, I>,
    {
        // SAFETY: `&self` excludes every `&mut` Graph/getter path; this is a plain
        // read of the `bypassed` field through the cell pointer, concurrent only
        // with shared reads (`&P` from `stage`), which touch a different field.
        unsafe { (*T::pointer(self.storage.as_ref().expect("owned"), sealed::permit())).bypassed }
    }
    /// Refuses (`AudioSessionActive`) while a manual or forgotten audio owner is live.
    pub fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        if self.graph.as_ref().expect("owned").audio_owner.is_some() {
            return Err(PrepareError::AudioSessionActive);
        }
        self.deactivate();
        if c.layout() != self.layout
            || !c.auxiliary_buses().is_empty()
            || c.maximum_output_events() != 0
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        if c.maximum_frames() > self.maximum_frames || c.maximum_events() > self.maximum_events {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        self.graph.as_mut().expect("owned").prepare(c)
    }
    /// Original first executed-node fault, until reset/reprepare/deactivation.
    pub fn first_fault(&self) -> Option<GraphStatus> {
        self.first_fault
    }
    pub fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, S>) -> GraphStatus {
        observe(
            &mut self.first_fault,
            self.graph.as_mut().expect("owned").process(b),
        )
    }
    pub fn reset(&mut self) {
        let _ = self.try_reset();
    }
    /// Forward refusal through the canonical Graph and retain its node identity.
    pub fn try_reset(&mut self) -> Result<(), GraphStatus> {
        match self.graph.as_mut().expect("owned").try_reset() {
            Ok(()) => {
                self.first_fault = None;
                Ok(())
            }
            Err(status) => {
                observe(&mut self.first_fault, status);
                Err(status)
            }
        }
    }
    /// CONTROL only. **Panics** (canonical Graph assertion) while a manual or
    /// forgotten audio owner is live; it does not refuse. A caught panic leaves the
    /// chain prepared with the owner live (nothing is changed before the assertion),
    /// so the owner thread can still call `Processor::stop_audio`.
    pub fn deactivate(&mut self) {
        self.graph.as_mut().expect("owned").deactivate();
        self.first_fault = None;
    }
    pub fn latency_samples(&self) -> u32 {
        self.graph.as_ref().expect("owned").latency_samples()
    }
    pub fn tail(&self) -> Tail {
        self.graph.as_ref().expect("owned").tail()
    }
    pub fn start_audio(&mut self) -> Result<TypedChainAudioSession<'_, S, T>, GraphStatus> {
        if let Err(status) = self.graph.as_mut().expect("owned").start_audio_nodes() {
            observe(&mut self.first_fault, status);
            return Err(status);
        }
        let graph = GraphAudioSession {
            graph: self.graph.as_mut().expect("owned"),
            owner: PhantomData,
        };
        Ok(TypedChainAudioSession {
            graph,
            storage: self.storage.as_mut().expect("owned"),
            first_fault: &mut self.first_fault,
        })
    }
    /// CONTROL only. Drop graph/wrappers first, then recover the original tuple.
    /// The Graph is only dropped: nothing calls `stop_audio` here. With a manual
    /// or forgotten audio owner live this **panics** inside the Graph drop, each
    /// stage is dropped once during the unwind without `stop_audio`/`deactivate`,
    /// and the original tuple is not returned. Stop the owner on its thread first.
    pub fn into_stages(mut self) -> T {
        drop(self.graph.take());
        T::recover(self.storage.take().expect("owned"), sealed::permit())
    }
}
fn observe(first: &mut Option<GraphStatus>, status: GraphStatus) -> GraphStatus {
    if matches!(
        status,
        GraphStatus::Fault { node: Some(_), .. } | GraphStatus::EventFault { .. }
    ) && first.is_none()
    {
        *first = Some(status);
    }
    status
}
impl<S: GraphSample, T: StageTuple<S>> Processor<S> for TypedChain<S, T> {
    fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
        TypedChain::prepare(self, c)
    }
    /// Manual hook: the caller must call `stop_audio` on this thread before
    /// transfer, `deactivate`, `into_stages` or drop (see the type docs). Failure
    /// maps `Fault { error, .. }` to that `error` (a retry on a faulted graph is
    /// `NotPrepared`); the original first fault remains in `first_fault()`.
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        match self.graph.as_mut().expect("owned").start_audio_nodes() {
            Ok(()) => Ok(()),
            Err(status) => {
                observe(&mut self.first_fault, status);
                Err(match status {
                    GraphStatus::Fault { error, .. } => error,
                    _ => ProcessError::BackendFailure,
                })
            }
        }
    }
    /// Must run on the thread that started the manual owner; a different thread
    /// panics through the canonical assertion and leaves the owner live.
    fn stop_audio(&mut self) {
        self.graph.as_mut().expect("owned").stop_audio_nodes();
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        match TypedChain::process(self, b) {
            GraphStatus::Continue => ProcessStatus::Continue,
            GraphStatus::Fault { node: None, error } if self.first_fault.is_none() => {
                ProcessStatus::Fault(error)
            }
            _ => ProcessStatus::Fault(ProcessError::BackendFailure),
        }
    }
    fn reset(&mut self) {
        TypedChain::reset(self);
    }
    fn try_reset(&mut self) -> Result<(), ProcessError> {
        TypedChain::try_reset(self).map_err(|status| match status {
            GraphStatus::Fault { error, .. } => error,
            _ => ProcessError::BackendFailure,
        })
    }
    /// Panics while a manual audio owner is live (see `TypedChain::deactivate`).
    fn deactivate(&mut self) {
        TypedChain::deactivate(self);
    }
    fn latency_samples(&self) -> u32 {
        TypedChain::latency_samples(self)
    }
    fn tail(&self) -> Tail {
        TypedChain::tail(self)
    }
}
/// Canonical non-Send audio-session guard with typed readonly access and bypass.
/// Finish/drop before CONTROL mutation, extraction or transfer. Do not forget it:
/// a forgotten guard leaves the audio owner live (see `TypedChain`), which is
/// misuse and not a supported way to hand the chain to another thread.
///
/// The guard is not `Send`:
///
/// ```compile_fail,E0277
/// use ravet_graph::TypedChainAudioSession;
/// use ravet_gain::GainProcessor;
/// fn send<T:Send>() {}
/// send::<TypedChainAudioSession<'static,f32,(GainProcessor,)>>();
/// ```
/// A live `stage()` borrow excludes `process` on the session:
///
/// ```compile_fail,E0502
/// use ravet_graph::{StageAt,TypedChainAudioSession};
/// use ravet_foundation::{ProcessBlock,Processor};
/// fn overlap<T: StageAt<f32,0>>(session:&mut TypedChainAudioSession<'_,f32,T>, block:&mut ProcessBlock<'_,'_,'_,f32>) {
///     let stage=session.stage::<0>();
///     let _=session.process(block);
///     let _=stage.latency_samples();
/// }
/// ```
/// ... and excludes `set_bypassed`:
///
/// ```compile_fail,E0502
/// use ravet_graph::{StageAt,TypedChainAudioSession};
/// use ravet_foundation::Processor;
/// fn overlap<T: StageAt<f32,0>>(session:&mut TypedChainAudioSession<'_,f32,T>) {
///     let stage=session.stage::<0>();
///     session.set_bypassed::<0>(true);
///     let _=stage.latency_samples();
/// }
/// ```
/// Positive companion: read first, then mutate and render.
///
/// ```
/// use ravet_graph::{StageAt,TypedChainAudioSession};
/// use ravet_foundation::{ProcessBlock,Processor};
/// fn ordered<T: StageAt<f32,0>>(session:&mut TypedChainAudioSession<'_,f32,T>, block:&mut ProcessBlock<'_,'_,'_,f32>) {
///     let latency=session.stage::<0>().latency_samples();
///     session.set_bypassed::<0>(true);
///     let _=session.process(block);
///     let _=latency;
/// }
/// ```
pub struct TypedChainAudioSession<'a, S: GraphSample, T: StageTuple<S>> {
    graph: GraphAudioSession<'a, S>,
    storage: &'a mut T::Storage,
    first_fault: &'a mut Option<GraphStatus>,
}
impl<S: GraphSample, T: StageTuple<S>> TypedChainAudioSession<'_, S, T> {
    pub fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, S>) -> GraphStatus {
        observe(self.first_fault, self.graph.process(b))
    }
    pub fn reset(&mut self) {
        let _ = self.try_reset();
    }
    /// Preserve the first fault until the complete reset succeeds.
    pub fn try_reset(&mut self) -> Result<(), GraphStatus> {
        match self.graph.try_reset() {
            Ok(()) => {
                *self.first_fault = None;
                Ok(())
            }
            Err(status) => {
                observe(self.first_fault, status);
                Err(status)
            }
        }
    }
    pub fn stage<const I: usize>(&self) -> &<T as StageAt<S, I>>::Processor
    where
        T: StageAt<S, I>,
    {
        // SAFETY: `&self` of the session excludes `process`/`reset`/`set_bypassed`
        // (all `&mut self`), so no wrapper `&mut Record` is live, and the session's
        // `&mut Graph` is not used. The `&mut Storage` held by the session is a
        // reborrow of the chain's storage and covers only the Vec headers; the
        // record is reached through the `UnsafeCell::get` pointer (W2), so no unique
        // tag over the record exists. The returned `&P` covers the processor field
        // only and is tied to this `&self`.
        unsafe { &(*T::pointer(self.storage, sealed::permit())).processor }
    }
    pub fn set_bypassed<const I: usize>(&mut self, value: bool)
    where
        T: StageAt<S, I>,
    {
        // SAFETY: `&mut self` excludes `stage()` references and Graph calls. The
        // write goes through the cell pointer to the `bypassed` field only; no
        // reference to the record or processor is created.
        unsafe {
            (*T::pointer(self.storage, sealed::permit())).bypassed = value;
        }
    }
    pub fn is_bypassed<const I: usize>(&self) -> bool
    where
        T: StageAt<S, I>,
    {
        // SAFETY: `&self` excludes every mutating path; plain read of the
        // `bypassed` field through the cell pointer (W2).
        unsafe { (*T::pointer(self.storage, sealed::permit())).bypassed }
    }
    pub fn latency_samples(&self) -> u32 {
        self.graph.latency_samples()
    }
    pub fn tail(&self) -> Tail {
        self.graph.tail()
    }
    pub fn finish(self) {}
}
