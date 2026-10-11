//! Controller edit gestures for Graph-bank children (decision 0276).
//!
//! `RavetVst3EditGestureProviderV1` is an additive companion paired with the
//! exact instance ABI-v2 table. A child opts in with
//! [`Vst3GraphBankBuilder::edit_gestures`](super::Vst3GraphBankBuilder::edit_gestures).
//! `build` then enables capture natively after the child is prepared and before
//! any lease begins. From then on the child's `IComponentHandler` accepts
//! `beginEdit`, `performEdit` and `endEdit` on the platform main thread. Every
//! other child keeps the original handler, which refuses edits with
//! `kNotImplemented`.
//!
//! Flow, all bounded and allocated at `build`:
//!
//! 1. **Native capture (MAIN).** The handler validates each call and appends it
//!    to a fixed ring of [`NATIVE_CAPACITY`] records. The id must be a writable
//!    parameter of the catalogue refreshed at `build`, and a `performEdit` value
//!    must be finite and in `0..=1`; otherwise the plug-in gets
//!    `kInvalidArgument`. A full ring returns `kResultFalse` to the plug-in and
//!    is counted ([`EditState::overflowed`]). Off-main calls and calls after
//!    child retirement are refused and counted.
//!
//!    Begin/end pairing is preserved: at most [`OPEN_GESTURES`] ids may be
//!    open, and each open gesture keeps one ring slot reserved, so the
//!    `endEdit` of an accepted `beginEdit` is never refused for capacity. A
//!    second `beginEdit` of an open id, and an `endEdit` without an open
//!    `beginEdit`, are refused with `kResultFalse` and counted
//!    ([`EditState::unpaired`]); they are never recorded, so published begins
//!    and ends are always balanced per id, except in these cases:
//!    - Closing or dropping the editor with gestures open records a
//!      synthesised `End` for each of them ([`EditEvent::synthesized`]); a
//!      later `endEdit` from the plug-in for that id is then refused as
//!      unpaired.
//!    - Gestures still open when the child retires are not closed (the bank
//!      and its queues are gone).
//!    - A panic in a `drain_edits` callback loses the rest of that batch, and
//!      a pump `Abi` error consumes a native batch; either can drop a begin
//!      or an end from the published stream.
//!
//!    While a gesture stays open, the plug-in's next `endEdit` for that id
//!    closes it. At most 64 can be open, so the cost is bounded at 64 reserved
//!    slots.
//! 2. **Pump (MAIN).**
//!    [`Vst3GraphBankControl::pump_edits`](super::Vst3GraphBankControl::pump_edits)
//!    moves records out of the native ring only while there is room for them in
//!    BOTH the AUDIO value ring and the host event queue. A record is therefore
//!    never taken from the native ring unless it can be both applied and
//!    published. When either side is full the records wait natively, and once
//!    the native ring is full the plug-in's next edit is refused and counted.
//! 3. **AUDIO delivery.** Each `performEdit` value becomes one parameter point at
//!    sample offset 0 of the next block the node successfully dispatches to the
//!    plug-in, in the node's existing prepared parameter queue, ahead of that
//!    block's framework or lane points.
//!    - Values for the same id that arrive before that block coalesce, last
//!      wins ([`EditState::coalesced`]); all of them would land on the same
//!      sample.
//!    - **The edit wins at offset 0.** If the block itself carries a point for
//!      the same id at offset 0, the edit value replaces that block point,
//!      which is counted in [`EditState::overridden`]. The block's later
//!      points for that id still apply, so sustained offset-0 automation of
//!      the same id resumes on the next block. This intentionally differs from
//!      the framework `SameOffsetPolicy` and `MergePolicy`: edits are merged
//!      after them and independently of their settings.
//!    - If the native 64-distinct-id or 2048-point limit would be exceeded, the
//!      remaining edit values are deferred to the next block
//!      ([`EditState::deferred_blocks`]).
//!    - Values leave the pending set only when the native dispatch succeeds. A
//!      refused (silent) block or a failed dispatch keeps them pending (a
//!      failed dispatch also faults the node, which then needs a rebuild).
//!    - The pending set holds at most [`MAXIMUM_EDIT_VALUES`] distinct ids;
//!      coalescing is a linear scan, so the worst case per block is
//!      `values x values` id comparisons (65,536 at the maximum).
//!    - No allocation, lock or free happens on AUDIO.
//! 4. **Host publication (MAIN).**
//!    [`Vst3GraphBankControl::drain_edits`](super::Vst3GraphBankControl::drain_edits)
//!    passes every pumped [`EditEvent`] in capture order, with its exact
//!    gap-free native sequence. When the child also has a framework
//!    translation, an edit of a routed id carries the matching framework event:
//!    `ParameterGesture` begin or end, or a physical `ParameterValue` through
//!    the route's declared scale. That framework event is for the host's own
//!    model (automation recording, UI mirrors). Do not feed it back into the
//!    same node: the value was already applied on AUDIO.
//!
//! After the node's bundle has returned to MAIN (node dropped, gathered by
//! `poll_returns` or reclamation), `pump_edits` refuses with
//! [`Error::Abandoned`] and moves nothing; records then wait natively until
//! the plug-in's edits are refused for capacity. A pump that reports
//! [`Error::Abi`] (a malformed record from the trusted provider) has already
//! consumed that native batch and leaves the edit path out of sequence: treat
//! it as fatal for the child's edits. `drain_edits` removes events from the
//! queue before calling `f`; a panic in `f` loses the rest of that batch.
//!
//! `restartComponent` follows an explicit flag policy once capture is enabled.
//! Only [`PARAMETER_VALUES_CHANGED`], [`LATENCY_CHANGED`] and
//! [`PARAMETER_TITLES_CHANGED`] are accepted. Any other defined flag refuses
//! the whole call with `kResultFalse`. Both accepted and refused restarts are
//! published as [`EditKind::Restart`]. `kResultOk` means "recorded for the
//! host", not "applied": the bank does not reconfigure a running child. An
//! accepted latency change is reported with `rebuild_required`, and the node
//! keeps the latency cached at `build` until the bank is rebuilt. After a
//! values or titles change the writable-id snapshot taken at `build` is not
//! refreshed, so ids the plug-in adds later stay `kInvalidArgument` until a
//! rebuild.
use super::framework::{FrameworkTranslation, ParameterScale};
use crate::vst3::instances::{RawProvider as InstanceProvider, parameters::Point, processing};
use ravet_foundation::{EventData, ParameterGesturePhase, ParameterId};
use std::{
    collections::VecDeque,
    ffi::c_void,
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

/// Native per-child capture ring (`RAVET_VST3_EDIT_GESTURE_CAPACITY`).
pub const NATIVE_CAPACITY: usize = 256;
/// Largest host event queue per child.
pub const MAXIMUM_EDIT_CAPACITY: usize = 4096;
/// Largest AUDIO value ring and pending set of distinct ids per child; bounds
/// the per-block coalescing cost.
pub const MAXIMUM_EDIT_VALUES: usize = 256;
/// Gestures that may be open (begun, not ended) per child at once.
pub const OPEN_GESTURES: usize = 64;
/// `RavetVst3EditGesture::flags` bit on an `End` synthesised at editor close.
pub const SYNTHESIZED: u32 = 1;
/// `kParamValuesChanged`.
pub const PARAMETER_VALUES_CHANGED: u32 = 4;
/// `kLatencyChanged`.
pub const LATENCY_CHANGED: u32 = 8;
/// `kParamTitlesChanged`.
pub const PARAMETER_TITLES_CHANGED: u32 = 16;
/// Flags a gesture-enabled child accepts from `restartComponent`.
pub const SUPPORTED_RESTART: u32 =
    PARAMETER_VALUES_CHANGED | LATENCY_CHANGED | PARAMETER_TITLES_CHANGED;
/// Points per block in the merged queue (the processing ABI limit).
const MERGED_POINTS: usize = 2048;
/// Distinct ids per block in the native parameter queue.
const BLOCK_IDS: usize = 64;

const STATUS_OK: i32 = 0;
const STATUS_INVALID: i32 = 1;
const STATUS_THREAD: i32 = 5;
const STATUS_LIFECYCLE: i32 = 10;

/// `RavetVst3EditGesture` (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RawGesture {
    pub struct_size: u32,
    pub kind: u32,
    pub id: u32,
    pub flags: u32,
    pub value: f64,
    pub sequence: u64,
}
/// `RavetVst3EditGestureState` (104 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawState {
    pub struct_size: u32,
    pub flags: u32,
    pub capacity: u32,
    pub pending: u32,
    pub open: u32,
    pub reserved: u32,
    pub accepted: u64,
    pub drained: u64,
    pub overflowed: u64,
    pub invalid: u64,
    pub unknown: u64,
    pub off_thread: u64,
    pub retired: u64,
    pub restart_accepted: u64,
    pub restart_refused: u64,
    pub unpaired: u64,
}
type Enable = unsafe extern "C" fn(*mut c_void) -> i32;
type Drain = unsafe extern "C" fn(*mut c_void, *mut RawGesture, u32, *mut u32) -> i32;
type GetState = unsafe extern "C" fn(*const c_void, *mut RawState) -> i32;
/// `RavetVst3EditGestureProviderV1` (40 bytes on 64-bit targets). Nullable
/// entries are rejected before any call.
#[repr(C)]
pub struct RawProvider {
    pub struct_size: u32,
    pub version: u32,
    pub instance_origin: *const InstanceProvider,
    pub enable: Option<Enable>,
    pub drain: Option<Drain>,
    pub state: Option<GetState>,
}
/// Verified edit-gesture companion table, restricted to the main thread.
#[derive(Clone, Copy)]
pub struct Provider<'a> {
    pub(super) raw: &'a RawProvider,
    no_send_sync: PhantomData<Rc<()>>,
}
impl<'a> Provider<'a> {
    /// # Safety
    /// The table and its exact origin are trusted compliant native code that
    /// stays loaded for `'a`. Entries contain foreign exceptions, check the
    /// platform main thread and implement the documented capture, refusal and
    /// drain contract. A table borrow does not keep an unloaded DLL alive.
    pub unsafe fn from_raw(raw: &'a RawProvider) -> Result<Self, Error> {
        if raw.struct_size as usize != size_of::<RawProvider>()
            || raw.version != 1
            || raw.instance_origin.is_null()
            || raw.enable.is_none()
            || raw.drain.is_none()
            || raw.state.is_none()
        {
            return Err(Error::Abi);
        }
        Ok(Self {
            raw,
            no_send_sync: PhantomData,
        })
    }
}

/// Edit-gesture companion error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Malformed table, foreign origin, or a malformed native record or state.
    Abi,
    /// Called off the provider's main thread.
    Thread,
    /// Refused in the current state: no refreshed catalogue or controller,
    /// a live AUDIO lease during enable, or another provider entry running.
    Lifecycle,
    /// Capacities outside `1..=4096` (events) or `1..=256` (values).
    Shape,
    /// The node's AUDIO end has returned to MAIN; edits can no longer be
    /// delivered, so nothing is moved.
    Abandoned,
    /// Preparation allocation failed.
    Allocation,
    /// Any other native status.
    Status(i32),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VST3 edit-gesture error: {self:?}")
    }
}
impl std::error::Error for Error {}
fn status(status: i32) -> Result<(), Error> {
    match status {
        STATUS_OK => Ok(()),
        STATUS_THREAD => Err(Error::Thread),
        STATUS_LIFECYCLE => Err(Error::Lifecycle),
        STATUS_INVALID => Err(Error::Abi),
        other => Err(Error::Status(other)),
    }
}

/// Per-child bounded capacities, allocated on MAIN at `build`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditOptions {
    /// Host event queue read by `drain_edits`, `1..=4096`.
    pub events: usize,
    /// AUDIO value ring and the per-node pending set of distinct ids,
    /// `1..=256` ([`MAXIMUM_EDIT_VALUES`]).
    pub values: usize,
}
impl Default for EditOptions {
    /// 256 events and 256 values, matching the native ring.
    fn default() -> Self {
        Self {
            events: NATIVE_CAPACITY,
            values: NATIVE_CAPACITY,
        }
    }
}
impl EditOptions {
    pub(super) fn check(&self) -> Result<(), Error> {
        if !(1..=MAXIMUM_EDIT_CAPACITY).contains(&self.events)
            || !(1..=MAXIMUM_EDIT_VALUES).contains(&self.values)
        {
            return Err(Error::Shape);
        }
        Ok(())
    }
    /// Bytes counted in the bank's 64 MiB lane budget.
    pub(super) fn bytes(&self) -> usize {
        // Value ring plus pending set, merged points, host queue and scratch.
        self.values * 2 * size_of::<EditValue>()
            + MERGED_POINTS * size_of::<Point>()
            + self.events * size_of::<EditEvent>()
            + NATIVE_CAPACITY * size_of::<RawGesture>()
    }
}

/// What the controller did, in capture order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditKind {
    /// `beginEdit(id)`.
    Begin { id: u32 },
    /// `performEdit(id, normalized)`; delivered to AUDIO at offset 0 of the
    /// next dispatched block.
    Perform { id: u32, normalized: f64 },
    /// `endEdit(id)`.
    End { id: u32 },
    /// `restartComponent(flags)`. `accepted` is false when the call was
    /// refused because it carried an unsupported flag. `rebuild_required` is
    /// set for an accepted [`LATENCY_CHANGED`]: the node keeps its cached
    /// latency until the bank is rebuilt.
    Restart {
        flags: u32,
        accepted: bool,
        rebuild_required: bool,
    },
}
/// One published controller edit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditEvent {
    /// Exact native capture sequence: gap-free from 0 for each child.
    pub sequence: u64,
    pub kind: EditKind,
    /// The equivalent framework event when the child's translation declares a
    /// route for this native id: `ParameterGesture` begin/end, or a physical
    /// `ParameterValue` through the route's scale. `None` otherwise.
    pub framework: Option<EventData>,
    /// An `End` that RAVET recorded when the editor closed with this gesture
    /// still open, not one the plug-in sent.
    pub synthesized: bool,
}

/// Copied native and node counters for one gesture-enabled child.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditState {
    /// Undrained records in the native ring.
    pub native_pending: u32,
    /// Open gestures (each reserving one native end slot).
    pub open: u32,
    /// Refused unpaired calls: a begin of an already open id, or an end
    /// without an open begin.
    pub unpaired: u64,
    /// Gestures and accepted restarts recorded natively.
    pub accepted: u64,
    /// Calls refused because the native ring was full.
    pub overflowed: u64,
    /// Calls refused for a non-finite or out-of-range value, or malformed flags.
    pub invalid: u64,
    /// Calls refused for an id that is not a writable catalogue parameter.
    pub unknown: u64,
    /// Calls refused off the platform main thread.
    pub off_thread: u64,
    /// Calls refused after child retirement.
    pub retired: u64,
    pub restart_accepted: u64,
    pub restart_refused: u64,
    /// Events waiting in the host queue.
    pub queued: usize,
    /// Values pumped but not yet taken by AUDIO.
    pub values_in_flight: usize,
    /// Distinct-id values taken by AUDIO and waiting for a successful dispatch.
    pub values_pending: u64,
    /// Records pumped so far.
    pub pumped: u64,
    /// Values placed in a dispatched block.
    pub applied: u64,
    /// Values replaced by a later value for the same id before delivery.
    pub coalesced: u64,
    /// Block points replaced by an edit value for the same id at offset 0
    /// (the edit wins).
    pub overridden: u64,
    /// Blocks that deferred some values to a later block (native id or point
    /// limits).
    pub deferred_blocks: u64,
}

/// AUDIO-side counters shared with MAIN.
#[derive(Default)]
pub(super) struct EditCounters {
    applied: AtomicU64,
    coalesced: AtomicU64,
    overridden: AtomicU64,
    deferred: AtomicU64,
    pending: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct EditValue {
    id: u32,
    value: f64,
}

#[derive(Clone, Copy)]
struct Route {
    native: u32,
    graph: ParameterId,
    scale: ParameterScale,
}

/// MAIN end: native drain, AUDIO producer and host queue.
pub(super) struct EditMain {
    raw: &'static RawProvider,
    values: rtrb::Producer<EditValue>,
    queue: VecDeque<EditEvent>,
    capacity: usize,
    scratch: Vec<RawGesture>,
    next: u64,
    pumped: u64,
    routes: Vec<Route>,
}
/// AUDIO end, owned by the node bundle.
pub(super) struct EditAudio {
    values: rtrb::Consumer<EditValue>,
    pending: Vec<EditValue>,
    count: usize,
    merged: processing::InputPoints,
    /// Staged outcome of the last `merge`, applied by `commit` only after a
    /// successful dispatch.
    staged: Staged,
}
#[derive(Clone, Copy)]
struct Staged {
    active: bool,
    /// Bit set of pending values that were NOT placed (kept after commit).
    keep: [u64; MAXIMUM_EDIT_VALUES / 64],
    applied: u64,
    overridden: u64,
    deferred: bool,
}
impl Staged {
    const IDLE: Self = Self {
        active: false,
        keep: [0; MAXIMUM_EDIT_VALUES / 64],
        applied: 0,
        overridden: 0,
        deferred: false,
    };
}

/// MAIN: enable native capture and build both ends. Called with the stopped,
/// prepared child before any lease begins.
pub(super) fn prepare(
    provider: Provider<'static>,
    child: *mut c_void,
    options: EditOptions,
    translation: Option<&FrameworkTranslation>,
) -> Result<(EditMain, EditAudio), Error> {
    options.check()?;
    let mut routes = Vec::new();
    if let Some(translation) = translation {
        routes
            .try_reserve_exact(translation.parameters.len())
            .map_err(|_| Error::Allocation)?;
        routes.extend(translation.parameters.iter().map(|r| Route {
            native: r.native_id,
            graph: r.graph_id,
            scale: r.scale,
        }));
        routes.sort_unstable_by_key(|r| r.native);
    }
    let mut queue = VecDeque::new();
    queue
        .try_reserve_exact(options.events)
        .map_err(|_| Error::Allocation)?;
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(NATIVE_CAPACITY)
        .map_err(|_| Error::Allocation)?;
    scratch.resize(NATIVE_CAPACITY, RawGesture::default());
    let mut pending = Vec::new();
    pending
        .try_reserve_exact(options.values)
        .map_err(|_| Error::Allocation)?;
    pending.resize(options.values, EditValue::default());
    let merged = processing::InputPoints::new(MERGED_POINTS).map_err(|_| Error::Allocation)?;
    let (producer, consumer) = rtrb::RingBuffer::new(options.values);
    // SAFETY: verified exact table; live stopped child on MAIN.
    status(unsafe { provider.raw.enable.unwrap()(child) })?;
    Ok((
        EditMain {
            raw: provider.raw,
            values: producer,
            queue,
            capacity: options.events,
            scratch,
            next: 0,
            pumped: 0,
            routes,
        },
        EditAudio {
            values: consumer,
            pending,
            count: 0,
            merged,
            staged: Staged::IDLE,
        },
    ))
}

impl EditMain {
    fn route(&self, native: u32) -> Option<&Route> {
        self.routes
            .binary_search_by_key(&native, |r| r.native)
            .ok()
            .map(|index| &self.routes[index])
    }
    /// Check one native record against the exact next sequence and the ABI.
    fn admit(&self, raw: RawGesture) -> Result<EditEvent, Error> {
        if raw.struct_size as usize != size_of::<RawGesture>() || raw.sequence != self.next {
            return Err(Error::Abi);
        }
        let plain = |raw: &RawGesture| raw.flags == 0 && raw.value == 0.;
        if raw.kind <= 3 && raw.id == u32::MAX {
            // kNoParamId is never a writable catalogue id.
            return Err(Error::Abi);
        }
        let kind = match raw.kind {
            1 if plain(&raw) => EditKind::Begin { id: raw.id },
            2 if raw.flags == 0 && raw.value.is_finite() && (0. ..=1.).contains(&raw.value) => {
                EditKind::Perform {
                    id: raw.id,
                    normalized: raw.value,
                }
            }
            3 if raw.value == 0. && raw.flags & !SYNTHESIZED == 0 => EditKind::End { id: raw.id },
            4 | 5
                if raw.id == 0
                    && raw.value == 0.
                    && raw.flags != 0
                    && raw.flags & !0xfff == 0
                    && (raw.kind == 5) == (raw.flags & !SUPPORTED_RESTART != 0) =>
            {
                EditKind::Restart {
                    flags: raw.flags,
                    accepted: raw.kind == 4,
                    rebuild_required: raw.kind == 4 && raw.flags & LATENCY_CHANGED != 0,
                }
            }
            _ => return Err(Error::Abi),
        };
        let framework = match kind {
            EditKind::Begin { id } | EditKind::End { id } => {
                self.route(id).map(|r| EventData::ParameterGesture {
                    id: r.graph,
                    phase: if matches!(kind, EditKind::Begin { .. }) {
                        ParameterGesturePhase::Begin
                    } else {
                        ParameterGesturePhase::End
                    },
                })
            }
            EditKind::Perform { id, normalized } => {
                self.route(id).map(|r| EventData::ParameterValue {
                    id: r.graph,
                    value: match r.scale {
                        ParameterScale::Normalized => normalized,
                        ParameterScale::Linear { minimum, maximum } => {
                            minimum + normalized * (maximum - minimum)
                        }
                    } as f32,
                })
            }
            EditKind::Restart { .. } => None,
        };
        Ok(EditEvent {
            sequence: raw.sequence,
            kind,
            framework,
            synthesized: raw.kind == 3 && raw.flags & SYNTHESIZED != 0,
        })
    }
    /// Move native records while BOTH the AUDIO ring and the host queue have
    /// room. Returns the number of records moved.
    pub(super) fn pump(&mut self, child: *mut c_void) -> Result<u32, Error> {
        if self.values.is_abandoned() {
            return Err(Error::Abandoned);
        }
        let mut moved = 0;
        loop {
            let room = self
                .values
                .slots()
                .min(self.capacity - self.queue.len())
                .min(NATIVE_CAPACITY);
            if room == 0 {
                return Ok(moved);
            }
            let mut count = 0;
            // SAFETY: verified exact table, live child on MAIN, distinct
            // initialized scratch of at least `room` records and a count.
            status(unsafe {
                self.raw.drain.unwrap()(child, self.scratch.as_mut_ptr(), room as u32, &mut count)
            })?;
            let count = count as usize;
            if count > room {
                return Err(Error::Abi);
            }
            for index in 0..count {
                let event = self.admit(self.scratch[index])?;
                if let EditKind::Perform { id, normalized } = event.kind
                    && self
                        .values
                        .push(EditValue {
                            id,
                            value: normalized,
                        })
                        .is_err()
                {
                    return Err(Error::Abi);
                }
                self.queue.push_back(event);
                self.next += 1;
                self.pumped += 1;
                moved += 1;
            }
            if count < room {
                return Ok(moved);
            }
        }
    }
    /// Take up to `limit` oldest published events.
    pub(super) fn take(&mut self, limit: usize, out: &mut Vec<EditEvent>) {
        while out.len() < limit
            && let Some(event) = self.queue.pop_front()
        {
            out.push(event);
        }
    }
    pub(super) fn queued(&self) -> usize {
        self.queue.len()
    }
    pub(super) fn state(
        &self,
        child: *const c_void,
        counters: &EditCounters,
    ) -> Result<EditState, Error> {
        let mut raw = RawState {
            struct_size: size_of::<RawState>() as u32,
            ..RawState::default()
        };
        // SAFETY: verified exact table, live child on MAIN, initialized record.
        status(unsafe { self.raw.state.unwrap()(child, &mut raw) })?;
        if raw.struct_size as usize != size_of::<RawState>()
            || raw.flags & !3 != 0
            || raw.capacity as usize != NATIVE_CAPACITY
            || raw.pending > raw.capacity
            || raw.open as usize > OPEN_GESTURES
            || raw.reserved != 0
        {
            return Err(Error::Abi);
        }
        let capacity = self.values.buffer().capacity();
        Ok(EditState {
            native_pending: raw.pending,
            open: raw.open,
            unpaired: raw.unpaired,
            accepted: raw.accepted,
            overflowed: raw.overflowed,
            invalid: raw.invalid,
            unknown: raw.unknown,
            off_thread: raw.off_thread,
            retired: raw.retired,
            restart_accepted: raw.restart_accepted,
            restart_refused: raw.restart_refused,
            queued: self.queue.len(),
            values_in_flight: capacity - self.values.slots(),
            pumped: self.pumped,
            applied: counters.applied.load(Ordering::Relaxed),
            coalesced: counters.coalesced.load(Ordering::Relaxed),
            overridden: counters.overridden.load(Ordering::Relaxed),
            values_pending: counters.pending.load(Ordering::Relaxed),
            deferred_blocks: counters.deferred.load(Ordering::Relaxed),
        })
    }
}

impl EditAudio {
    /// Move ring values into the pending set, last value per id wins. Stops
    /// without popping when a new id finds the pending set full. Linear in the
    /// pending set (at most [`MAXIMUM_EDIT_VALUES`]).
    pub(super) fn take(&mut self, counters: &EditCounters) {
        self.staged = Staged::IDLE;
        while let Ok(value) = self.values.peek() {
            let value = *value;
            if let Some(slot) = self.pending[..self.count]
                .iter_mut()
                .find(|p| p.id == value.id)
            {
                slot.value = value.value;
                counters.coalesced.fetch_add(1, Ordering::Relaxed);
            } else if self.count < self.pending.len() {
                self.pending[self.count] = value;
                self.count += 1;
            } else {
                break;
            }
            let _ = self.values.pop();
        }
        counters.pending.store(self.count as u64, Ordering::Relaxed);
    }
    /// Points for this block: pending edit values at offset 0 ahead of the
    /// block's own points, within the native id and point limits. An edit
    /// replaces the block's own point for the same id at offset 0 (the edit
    /// wins). Values that do not fit stay pending. Nothing leaves the pending
    /// set until [`Self::commit`]. Returns `base` unchanged when nothing
    /// applies.
    pub(super) fn merge<'s>(&'s mut self, base: &'s [Point]) -> &'s [Point] {
        self.staged = Staged::IDLE;
        if self.count == 0 {
            return base;
        }
        // Distinct block ids (at most 64 in an admitted block) and whether each
        // has a point at offset 0.
        let mut ids = [0u32; BLOCK_IDS];
        let mut at_zero = [false; BLOCK_IDS];
        let mut distinct = 0;
        for point in base {
            match ids[..distinct].iter().position(|&id| id == point.id) {
                Some(index) => at_zero[index] |= point.offset == 0,
                None if distinct < BLOCK_IDS => {
                    ids[distinct] = point.id;
                    at_zero[distinct] = point.offset == 0;
                    distinct += 1;
                }
                // Not an admitted block; leave the edits pending.
                None => return base,
            }
        }
        self.merged.clear();
        let mut staged = Staged {
            active: true,
            ..Staged::IDLE
        };
        // Block offset-0 points replaced by an edit (one bit per block id).
        let mut replaced = 0u64;
        let mut added = 0usize;
        for index in 0..self.count {
            let value = self.pending[index];
            let known = ids[..distinct].iter().position(|&id| id == value.id);
            let admitted = match known {
                // Replacing keeps the point count; extra points need room.
                Some(at) if at_zero[at] => true,
                Some(_) => base.len() + added < MERGED_POINTS,
                None => base.len() + added < MERGED_POINTS && distinct < BLOCK_IDS,
            };
            let pushed = admitted
                && self
                    .merged
                    .push(Point {
                        id: value.id,
                        offset: 0,
                        value: value.value,
                    })
                    .is_ok();
            if pushed {
                staged.applied += 1;
                match known {
                    Some(at) if at_zero[at] => {
                        replaced |= 1 << at;
                        staged.overridden += 1;
                    }
                    Some(_) => added += 1,
                    None => {
                        // Record the new id itself, so a later edit (id 0 in
                        // particular) never matches an unwritten slot.
                        ids[distinct] = value.id;
                        at_zero[distinct] = false;
                        added += 1;
                        distinct += 1;
                    }
                }
            } else {
                staged.keep[index / 64] |= 1 << (index % 64);
                staged.deferred = true;
            }
        }
        for point in base {
            if point.offset == 0
                && let Some(at) = ids[..distinct].iter().position(|&id| id == point.id)
                && replaced & (1 << at) != 0
            {
                continue;
            }
            if self.merged.push(*point).is_err() {
                // Unreachable for an admitted block; keep every edit pending.
                self.merged.clear();
                return base;
            }
        }
        self.staged = staged;
        self.merged.points()
    }
    /// After a successful dispatch: remove the placed values and count them.
    /// Without a staged merge (nothing placed) this only refreshes counters.
    pub(super) fn commit(&mut self, counters: &EditCounters) {
        let staged = std::mem::replace(&mut self.staged, Staged::IDLE);
        if !staged.active {
            return;
        }
        let mut kept = 0;
        for index in 0..self.count {
            if staged.keep[index / 64] & (1 << (index % 64)) != 0 {
                self.pending[kept] = self.pending[index];
                kept += 1;
            }
        }
        self.count = kept;
        counters
            .applied
            .fetch_add(staged.applied, Ordering::Relaxed);
        counters
            .overridden
            .fetch_add(staged.overridden, Ordering::Relaxed);
        if staged.deferred {
            counters.deferred.fetch_add(1, Ordering::Relaxed);
        }
        counters.pending.store(self.count as u64, Ordering::Relaxed);
    }
}
