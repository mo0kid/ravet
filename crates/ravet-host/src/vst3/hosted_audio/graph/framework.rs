//! Explicit translation of ordinary RAVET framework events and transport into
//! a hosted VST3 Graph node.
//!
//! A node translates only when its bank builder received a
//! [`FrameworkTranslation`] for it through
//! [`Vst3GraphBankBuilder::translate`](super::Vst3GraphBankBuilder::translate).
//! Without one, the node keeps the strict W255 behaviour: any non-empty
//! framework event block refuses before native execution.
//!
//! Every mapping is declared, validated on MAIN and stored in prepared bounded
//! storage before the first AUDIO lease begins:
//!
//! - **Parameters.** Each [`ParameterRoute`] names a RAVET [`ParameterId`], a
//!   VST3 `ParamID` and a [`ParameterScale`]. No ID equality is inferred; a route
//!   with equal IDs is still an explicit declaration. Routes must name an
//!   existing writable (not read-only, not hidden) parameter of the refreshed
//!   catalogue. Each [`EventData::ParameterValue`] becomes one
//!   normalized point at its exact sample offset in the native
//!   `IParameterChanges` queue for that ID. Points are never sorted or merged.
//!   A second value for the same native ID at the same offset follows
//!   [`FrameworkTranslation::same_offset`]: [`SameOffsetPolicy::Refuse`] (the
//!   default) refuses the block, and [`SameOffsetPolicy::LastWins`] replaces the
//!   earlier value and counts it as ignored. Unmapped IDs follow
//!   [`FrameworkTranslation::unmapped_parameters`].
//! - **Notes.** A [`NoteRoute`] selects one selected native event input bus.
//!   [`EventData::NoteOn`]/[`EventData::NoteOff`] become SDK `NoteOnEvent` /
//!   `NoteOffEvent` with `noteId == -1` under [`NoteIdPolicy::Unidentified`] and
//!   refuse under [`NoteIdPolicy::RequireIdentified`]. The translator is
//!   stateless: a note-off without an earlier note-on is passed through, and the
//!   plug-in owns voice state.
//!   [`EventData::NoteWithId`] keeps its ID (0..=`i32::MAX`). Channel must be
//!   below the bus channel count; velocity is copied unchanged; tuning, length,
//!   `ppqPosition` and flags are zero (nothing is inferred from transport).
//! - **Note expression.** With [`NoteRoute::expressions`], volume, pan, tuning,
//!   vibrato, expression and brightness become `NoteExpressionValueEvent`
//!   (type IDs 0..=5). VST3 addresses expressions by note ID only, so the target
//!   must carry an ID and leave channel and key as wildcards. Values use the SDK
//!   normalized units: volume `/4`, tuning `/240 + 0.5`, others unchanged. Once
//!   a capability is declared, a malformed target or value refuses the block
//!   even under an ignoring policy; only undeclared capabilities are ignored.
//! - **Poly pressure.** With [`NoteRoute::poly_pressure`], the
//!   [`NoteExpression::Pressure`] expression becomes `PolyPressureEvent`. The
//!   target needs a concrete channel and key; its optional ID becomes `noteId`
//!   (or -1). Pressure narrows to the SDK `f32`.
//! - **Transport.** With [`FrameworkTranslation::transport`], block-start
//!   [`Transport`] becomes a `ProcessContext` through [`transport_context`],
//!   with the SDK state flags set exactly for the fields present. Without it,
//!   transport is not passed and is not counted as ignored.
//! - **Mixed blocks.** In queued rich-lane mode, framework events and the lane
//!   batch's events and points follow [`MergePolicy`]. A context supplied by both
//!   the transport translation and the lane batch refuses the block.
//!
//! Every other framework kind (MIDI, UMP, modulation, gestures, scoped values,
//! scoped release, choke, completion) follows
//! [`FrameworkTranslation::unsupported`]; so do notes without a [`NoteRoute`]
//! and expressions that were not enabled. Ignored events are counted, never
//! silently dropped: see
//! [`Vst3GraphBankControl::ignored`](super::Vst3GraphBankControl::ignored).
//!
//! Routes are checked against a snapshot of the catalogue taken at `build`. A
//! later plug-in restart that changes parameter flags is not revalidated.
//!
//! Translation runs on AUDIO into prepared storage with no allocation, locks or
//! deallocation. Offsets and order are rechecked. Any refusal, including
//! capacity overflow, refuses the whole block before the plug-in runs:
//!
//! - Outputs are silenced and the node returns [`ProcessStatus::Silence`], not
//!   a fault. Neither the node nor an enclosing `ravet_graph::Graph` latches;
//!   the next valid block runs normally.
//! - The reason is a [`Refusal`]. It is counted, recorded in a bounded ring
//!   drained on MAIN, and marked in the node diagnostics. See
//!   [`Vst3GraphBankControl::refused`](super::Vst3GraphBankControl::refused)
//!   and [`Vst3GraphBankControl::drain_refusals`](super::Vst3GraphBankControl::drain_refusals).
//! - In queued mode the matching lane batch returns to MAIN as
//!   [`Error::Framework`](super::super::Error::Framework), keeping sequences
//!   aligned.
//!
//! A Graph does not see the refusal in its status: it receives
//! `GraphStatus::Continue`. Callers that need to react must read the counter,
//! the ring or the lane result. [Decision 0273] gives the reasons.
//!
//! [Decision 0273]: https://github.com/ravet-audio/ravet/blob/main/docs/decisions/0273-hosted-vst3-graph-framework-events.md
use super::LaneOptions;
use crate::vst3::instances::{
    parameters::{self, Point},
    processing::{self, Context, Event, Header, InputEvents, InputPoints, Kind, Note, RawEvent},
};
#[cfg(doc)]
use ravet_foundation::ProcessStatus;
use ravet_foundation::{
    EventData, EventError, NoteExpression, NoteId, NotePhase, NoteTarget, ParameterId,
    ProcessError, Transport,
};

/// Maximum declared parameter routes per node.
pub const MAXIMUM_PARAMETER_ROUTES: usize = 2048;
/// Maximum translated events, and separately points, per node block.
pub const MAXIMUM_TRANSLATED: usize = 2048;
/// Distinct native parameter IDs the native queue admits per block.
pub const MAXIMUM_PARAMETER_IDS: usize = 64;
/// Unread refusal records kept per node. Further refusals are still counted
/// by `Vst3GraphBankControl::refused`, but their details are dropped.
pub const REFUSAL_RECORDS: usize = 16;

/// Explicit conversion from a RAVET parameter value to a VST3 normalized value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParameterScale {
    /// The framework value is already normalized and must lie in `0..=1`.
    Normalized,
    /// Declared linear plain range: `(value - minimum) / (maximum - minimum)`.
    /// Values outside `minimum..=maximum` refuse. Both bounds are finite and
    /// `minimum < maximum`. This is a caller declaration, not a query of the
    /// plug-in's own (possibly nonlinear) mapping; stepped quantization stays
    /// with the plug-in.
    Linear { minimum: f64, maximum: f64 },
}
impl ParameterScale {
    fn valid(self) -> bool {
        match self {
            Self::Normalized => true,
            Self::Linear { minimum, maximum } => {
                minimum.is_finite()
                    && maximum.is_finite()
                    && minimum < maximum
                    && (maximum - minimum).is_finite()
            }
        }
    }
    fn normalize(self, value: f64) -> Option<f64> {
        if !value.is_finite() {
            return None;
        }
        let normalized = match self {
            Self::Normalized => value,
            Self::Linear { minimum, maximum } => {
                if !(minimum..=maximum).contains(&value) {
                    return None;
                }
                (value - minimum) / (maximum - minimum)
            }
        };
        (0. ..=1.).contains(&normalized).then_some(normalized)
    }
}

/// One declared RAVET-to-VST3 parameter identity and scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterRoute {
    pub graph_id: ParameterId,
    pub native_id: u32,
    pub scale: ParameterScale,
}
impl ParameterRoute {
    pub fn new(graph_id: ParameterId, native_id: u32, scale: ParameterScale) -> Self {
        Self {
            graph_id,
            native_id,
            scale,
        }
    }
}

/// What a node does with an event it has no declared mapping for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPolicy {
    /// Refuse the whole block before native execution.
    Refuse,
    /// Skip the event and count it; the rest of the block is translated.
    Ignore,
}

/// Note identity policy for the VST3 `noteId` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteIdPolicy {
    /// Plain NoteOn/NoteOff carry `noteId == -1`; NoteWithId keeps its ID.
    Unidentified,
    /// Only NoteWithId is admitted; plain NoteOn/NoteOff refuse the block.
    RequireIdentified,
}

/// Selected native event input bus and declared note capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteRoute {
    /// Native event input bus index; it must be selected in
    /// `Vst3GraphNodeOptions::event_inputs`.
    pub bus: u32,
    pub ids: NoteIdPolicy,
    /// Admit note expressions other than pressure (declared, not queried).
    pub expressions: bool,
    /// Admit [`NoteExpression::Pressure`] as SDK poly pressure.
    pub poly_pressure: bool,
}

/// What a node does with a second value for the same native parameter ID at
/// the same sample offset in one block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SameOffsetPolicy {
    /// Refuse the whole block ([`Refusal::SameOffset`]).
    Refuse,
    /// Replace the earlier value with the later one. The superseded value is
    /// counted as ignored. Only framework values coalesce; a framework value
    /// equal in ID and offset to a lane point is still a [`Refusal::MergeConflict`].
    LastWins,
}

/// Order of framework and rich-lane input inside one queued-mode block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergePolicy {
    /// Refuse a block in which both sources carry events or points.
    Refuse,
    /// Merge by sample offset; at equal offsets framework input comes first.
    FrameworkFirst,
    /// Merge by sample offset; at equal offsets lane input comes first.
    LaneFirst,
}

/// Per-node framework translation, declared on MAIN before `build`.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameworkTranslation {
    /// Unique graph IDs and unique native IDs; at most
    /// [`MAXIMUM_PARAMETER_ROUTES`].
    pub parameters: Vec<ParameterRoute>,
    /// Policy for [`EventData::ParameterValue`] without a route.
    pub unmapped_parameters: EventPolicy,
    /// Policy for a repeated native ID at the same offset.
    pub same_offset: SameOffsetPolicy,
    pub notes: Option<NoteRoute>,
    /// Policy for kinds this translation cannot represent or did not enable.
    pub unsupported: EventPolicy,
    /// Translate block-start transport into a `ProcessContext`. When false,
    /// transport is not passed to the plug-in and is not counted as ignored.
    pub transport: bool,
    /// Queued rich-lane mixing; ignored without lanes.
    pub merge: MergePolicy,
    /// Prepared translated note/expression capacity per block, `0..=2048`.
    pub events: usize,
    /// Prepared translated parameter point capacity per block, `0..=2048`.
    /// The native queue additionally admits at most
    /// [`MAXIMUM_PARAMETER_IDS`] distinct IDs per block.
    pub points: usize,
}
impl FrameworkTranslation {
    /// No routes, refusing policies, no transport, refused mixing.
    pub fn new(events: usize, points: usize) -> Self {
        Self {
            parameters: Vec::new(),
            unmapped_parameters: EventPolicy::Refuse,
            same_offset: SameOffsetPolicy::Refuse,
            notes: None,
            unsupported: EventPolicy::Refuse,
            transport: false,
            merge: MergePolicy::Refuse,
            events,
            points,
        }
    }
    fn merged(&self, lanes: Option<LaneOptions>) -> Option<LaneOptions> {
        lanes.filter(|_| self.merge != MergePolicy::Refuse)
    }
    /// Local shape check, independent of plug-in metadata.
    pub(super) fn check(&self, lanes: Option<LaneOptions>) -> Result<(), TranslationError> {
        if self.parameters.len() > MAXIMUM_PARAMETER_ROUTES
            || self.events > MAXIMUM_TRANSLATED
            || self.points > MAXIMUM_TRANSLATED
        {
            return Err(TranslationError::Capacity);
        }
        if let Some(lanes) = self.merged(lanes)
            && (self.events + lanes.input_events > MAXIMUM_TRANSLATED
                || self.points + lanes.input_points > MAXIMUM_TRANSLATED)
        {
            return Err(TranslationError::Capacity);
        }
        if self.notes.is_some_and(|n| n.bus >= 16) {
            return Err(TranslationError::NoteBus);
        }
        for (index, route) in self.parameters.iter().enumerate() {
            let earlier = &self.parameters[..index];
            if route.native_id == u32::MAX {
                return Err(TranslationError::NativeId { index });
            }
            if !route.scale.valid() {
                return Err(TranslationError::Scale { index });
            }
            if earlier.iter().any(|r| r.graph_id == route.graph_id) {
                return Err(TranslationError::DuplicateGraphId { index });
            }
            if earlier.iter().any(|r| r.native_id == route.native_id) {
                return Err(TranslationError::DuplicateNativeId { index });
            }
        }
        Ok(())
    }
    /// Prepared bytes, counted in the bank's shared 64 MiB lane budget.
    pub(super) fn bytes(&self, lanes: Option<LaneOptions>) -> Option<usize> {
        let merged = self.merged(lanes).map_or(Some(0), |l| {
            (self.events + l.input_events)
                .checked_mul(size_of::<RawEvent>())?
                .checked_add(l.input_payload.div_ceil(2) * 2)?
                .checked_add((self.points + l.input_points).checked_mul(size_of::<Point>())?)
        })?;
        self.events
            .checked_mul(size_of::<RawEvent>())?
            .checked_add(self.points.checked_mul(size_of::<Point>())?)?
            .checked_add(self.parameters.len().checked_mul(size_of::<Route>())?)?
            .checked_add(REFUSAL_RECORDS.checked_mul(size_of::<Refusal>())?)?
            .checked_add(merged)
    }
}

/// Translation declaration refused on MAIN.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranslationError {
    /// The note route bus index is 16 or more.
    NoteBus,
    /// Route `index` uses the reserved native ID `u32::MAX`.
    NativeId {
        index: usize,
    },
    /// Route `index` declares an invalid scale (non-finite or unordered bounds).
    Scale {
        index: usize,
    },
    /// Route `index` repeats an earlier route's RAVET parameter ID.
    DuplicateGraphId {
        index: usize,
    },
    /// Route `index` repeats an earlier route's native parameter ID.
    DuplicateNativeId {
        index: usize,
    },
    /// Route or per-block capacity beyond the bounded limits.
    Capacity,
    Allocation,
    /// Route `index` names no parameter of the refreshed catalogue.
    UnknownParameter {
        index: usize,
    },
    /// Route `index` names a read-only or hidden parameter.
    ReadOnlyParameter {
        index: usize,
    },
    /// The note route bus is not a selected native event input bus.
    EventBus,
}

/// Why a translating node refused one block before the plug-in ran.
///
/// A refusal silences the block and returns [`ProcessStatus::Silence`]: the
/// node is not faulted and an enclosing Graph continues. Each refusal is
/// counted, recorded in a bounded ring for MAIN, and in queued mode returned
/// with the lane batch as [`Error::Framework`](super::super::Error::Framework).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A framework event has an out-of-range offset, is unordered or carries
    /// an invalid value or target (Foundation detail with the event index).
    Event(EventError),
    /// Parameter value `index` has no declared route, under
    /// [`EventPolicy::Refuse`].
    UnmappedParameter { index: usize },
    /// Event `index` is a kind this translation cannot represent or did not
    /// enable, under [`EventPolicy::Refuse`].
    Unsupported { index: usize },
    /// Parameter value `index` repeats a native ID at an offset it already
    /// used in this block, under [`SameOffsetPolicy::Refuse`].
    SameOffset { index: usize },
    /// The block has `count` framework events and the translated note and
    /// expression events exceeded the prepared `maximum`.
    Events { count: usize, maximum: usize },
    /// The block has `count` framework events and the translated parameter
    /// points exceeded the prepared `maximum`.
    Points { count: usize, maximum: usize },
    /// Parameter value `index` would be one distinct native ID more than the
    /// native queue admits per block ([`MAXIMUM_PARAMETER_IDS`]).
    ParameterIds { index: usize, maximum: usize },
    /// Block-start transport cannot be represented (see [`transport_context`]).
    Transport,
    /// Framework and lane input both carry events or points, under
    /// [`MergePolicy::Refuse`].
    Mixed,
    /// Both the transport translation and the lane batch supplied a context.
    TwoContexts,
    /// Merged events, payload or points exceed the prepared merged storage.
    MergedCapacity,
    /// The merged points would exceed [`MAXIMUM_PARAMETER_IDS`] distinct IDs.
    MergedParameterIds,
    /// The same native ID at the same offset appears in both sources.
    MergeConflict,
}

#[derive(Clone, Copy, Debug)]
struct Route {
    graph: u32,
    native: u32,
    scale: ParameterScale,
}
#[derive(Clone, Copy, Debug)]
struct Notes {
    bus: u32,
    channels: u8,
    ids: NoteIdPolicy,
    expressions: bool,
    poly_pressure: bool,
}

/// Where a queued-mode block takes its native events and points from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    Lane,
    Framework,
    Merged,
}

/// Prepared AUDIO-side translator. Owned by the node bundle; dropped on MAIN.
pub(super) struct Translator {
    routes: Vec<Route>,
    unmapped: EventPolicy,
    same_offset: SameOffsetPolicy,
    notes: Option<Notes>,
    unsupported: EventPolicy,
    transport: bool,
    merge: MergePolicy,
    sample_rate: f64,
    event_capacity: usize,
    point_capacity: usize,
    pub(super) events: InputEvents,
    pub(super) points: InputPoints,
    pub(super) context: Option<Context>,
    /// Merged storage; zero capacity under [`MergePolicy::Refuse`] or
    /// without lanes, so no AUDIO path needs an `Option`.
    merged: (InputEvents, InputPoints),
    point_capacity_merged: usize,
    refusals: rtrb::Producer<Refusal>,
}
impl Translator {
    /// MAIN preparation against the refreshed catalogue and selected buses.
    /// Returns the translator and the MAIN end of its refusal ring.
    pub(super) fn prepare(
        translation: FrameworkTranslation,
        catalogue: &[parameters::Parameter],
        event_inputs: &[u8; 16],
        lanes: Option<LaneOptions>,
        sample_rate: f64,
    ) -> Result<(Self, rtrb::Consumer<Refusal>), TranslationError> {
        translation.check(lanes)?;
        let mut routes = Vec::new();
        routes
            .try_reserve_exact(translation.parameters.len())
            .map_err(|_| TranslationError::Allocation)?;
        for (index, route) in translation.parameters.iter().enumerate() {
            let parameter = catalogue
                .iter()
                .find(|p| p.id == route.native_id)
                .ok_or(TranslationError::UnknownParameter { index })?;
            if !parameter.writable() {
                return Err(TranslationError::ReadOnlyParameter { index });
            }
            routes.push(Route {
                graph: route.graph_id.0,
                native: route.native_id,
                scale: route.scale,
            });
        }
        routes.sort_unstable_by_key(|r| r.graph);
        let notes = match translation.notes {
            None => None,
            Some(n) => {
                let channels = event_inputs[n.bus as usize];
                if channels == 0 {
                    return Err(TranslationError::EventBus);
                }
                Some(Notes {
                    bus: n.bus,
                    channels,
                    ids: n.ids,
                    expressions: n.expressions,
                    poly_pressure: n.poly_pressure,
                })
            }
        };
        let storage = |error: processing::Error| match error {
            processing::Error::Allocation => TranslationError::Allocation,
            _ => TranslationError::Capacity,
        };
        let (events, payload, points) = translation.merged(lanes).map_or((0, 0, 0), |l| {
            (
                translation.events + l.input_events,
                l.input_payload,
                translation.points + l.input_points,
            )
        });
        let merged = (
            InputEvents::new(events, payload).map_err(storage)?,
            InputPoints::new(points).map_err(storage)?,
        );
        let (refusals, records) = rtrb::RingBuffer::new(REFUSAL_RECORDS);
        let translator = Self {
            routes,
            unmapped: translation.unmapped_parameters,
            same_offset: translation.same_offset,
            notes,
            unsupported: translation.unsupported,
            transport: translation.transport,
            merge: translation.merge,
            sample_rate,
            event_capacity: translation.events,
            point_capacity: translation.points,
            events: InputEvents::new(translation.events, 0).map_err(storage)?,
            points: InputPoints::new(translation.points).map_err(storage)?,
            context: None,
            merged,
            point_capacity_merged: points,
            refusals,
        };
        Ok((translator, records))
    }

    /// Record a refusal for MAIN. A full ring drops the details; the refusal
    /// is still counted by the node. Never allocates or blocks.
    fn record(&mut self, refusal: Refusal) -> Refusal {
        let _ = self.refusals.push(refusal);
        refusal
    }

    /// Translate one block into prepared storage. Returns the ignored count.
    /// No allocation; storage is cleared first, so a refusal leaves nothing
    /// that a later block could deliver.
    pub(super) fn translate(
        &mut self,
        events: &[ravet_foundation::Event],
        transport: Transport,
        frames: usize,
    ) -> Result<u64, Refusal> {
        self.translate_block(events, transport, frames)
            .map_err(|refusal| self.record(refusal))
    }
    fn translate_block(
        &mut self,
        events: &[ravet_foundation::Event],
        transport: Transport,
        frames: usize,
    ) -> Result<u64, Refusal> {
        self.events.clear();
        self.points.clear();
        self.context = None;
        let mut ignored = 0u64;
        let mut previous = 0;
        for (index, event) in events.iter().enumerate() {
            let offset = event.frame_offset;
            if (frames == 0 && offset != 0) || (frames > 0 && offset >= frames) {
                return Err(Refusal::Event(EventError::OffsetOutOfRange {
                    index,
                    offset,
                    frames,
                }));
            }
            if offset < previous {
                return Err(Refusal::Event(EventError::Unordered { index }));
            }
            previous = offset;
            match self.event(event.data, offset as u32, index, events.len())? {
                Mapped::Yes => {}
                Mapped::Superseded => ignored += 1,
                Mapped::Unmapped(EventPolicy::Refuse) => {
                    return Err(Refusal::UnmappedParameter { index });
                }
                Mapped::Unsupported(EventPolicy::Refuse) => {
                    return Err(Refusal::Unsupported { index });
                }
                Mapped::Unmapped(EventPolicy::Ignore)
                | Mapped::Unsupported(EventPolicy::Ignore) => {
                    ignored += 1;
                }
            }
        }
        if self.transport {
            self.context =
                transport_context(transport, self.sample_rate).map_err(|_| Refusal::Transport)?;
        }
        Ok(ignored)
    }

    fn event(
        &mut self,
        data: EventData,
        offset: u32,
        index: usize,
        count: usize,
    ) -> Result<Mapped, Refusal> {
        let invalid = Refusal::Event(EventError::InvalidValue { index });
        let unsupported = Mapped::Unsupported(self.unsupported);
        match data {
            EventData::ParameterValue { id, value } => {
                let Ok(found) = self.routes.binary_search_by_key(&id.0, |r| r.graph) else {
                    return Ok(Mapped::Unmapped(self.unmapped));
                };
                let route = self.routes[found];
                let value = route.scale.normalize(f64::from(value)).ok_or(invalid)?;
                if let Some(earlier) = self.points.same_offset(route.native, offset) {
                    return match self.same_offset {
                        SameOffsetPolicy::Refuse => Err(Refusal::SameOffset { index }),
                        SameOffsetPolicy::LastWins => self
                            .points
                            .replace_value(earlier, value)
                            .map(|()| Mapped::Superseded)
                            .map_err(|_| invalid),
                    };
                }
                if self.points.points().len() == self.point_capacity {
                    return Err(Refusal::Points {
                        count,
                        maximum: self.point_capacity,
                    });
                }
                match self.points.push(Point {
                    id: route.native,
                    offset,
                    value,
                }) {
                    Ok(()) => Ok(Mapped::Yes),
                    // Point capacity was checked above, so this is the
                    // distinct-ID limit of the native queue.
                    Err(processing::Error::Capacity) => Err(Refusal::ParameterIds {
                        index,
                        maximum: MAXIMUM_PARAMETER_IDS,
                    }),
                    Err(processing::Error::Conflict) => Err(Refusal::SameOffset { index }),
                    Err(_) => Err(invalid),
                }
            }
            EventData::NoteOn {
                channel,
                key,
                velocity,
            } => self.note(
                None,
                NotePhase::On,
                channel,
                key,
                velocity,
                offset,
                index,
                count,
            ),
            EventData::NoteOff {
                channel,
                key,
                velocity,
            } => self.note(
                None,
                NotePhase::Off,
                channel,
                key,
                velocity,
                offset,
                index,
                count,
            ),
            EventData::NoteWithId {
                id,
                phase,
                channel,
                key,
                velocity,
            } => self.note(
                Some(id),
                phase,
                channel,
                key,
                velocity,
                offset,
                index,
                count,
            ),
            EventData::NoteExpression {
                target,
                expression,
                value,
            } => {
                let Some(notes) = self.notes else {
                    return Ok(unsupported);
                };
                if !target.is_valid() || !expression.is_valid_value(value) {
                    return Err(invalid);
                }
                let kind = if expression == NoteExpression::Pressure {
                    if !notes.poly_pressure {
                        return Ok(unsupported);
                    }
                    let (Some(channel), Some(pitch)) = (target.channel, target.key) else {
                        return Err(invalid);
                    };
                    if channel >= notes.channels {
                        return Err(invalid);
                    }
                    Kind::PolyPressure {
                        channel,
                        pitch,
                        note_id: native_id(target.id).ok_or(invalid)?,
                        pressure: value as f32,
                    }
                } else {
                    if !notes.expressions {
                        return Ok(unsupported);
                    }
                    let NoteTarget {
                        channel: None,
                        key: None,
                        id: Some(id),
                    } = target
                    else {
                        return Err(invalid);
                    };
                    let (expression_id, value) =
                        expression_value(expression, value).ok_or(invalid)?;
                    Kind::ExpressionValue {
                        expression_id,
                        note_id: native_id(Some(id)).ok_or(invalid)?,
                        value,
                    }
                };
                self.push(notes.bus, offset, kind, index, count)
            }
            _ => Ok(unsupported),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn note(
        &mut self,
        id: Option<NoteId>,
        phase: NotePhase,
        channel: u8,
        key: u8,
        velocity: f32,
        offset: u32,
        index: usize,
        count: usize,
    ) -> Result<Mapped, Refusal> {
        let invalid = Refusal::Event(EventError::InvalidValue { index });
        let Some(notes) = self.notes else {
            return Ok(Mapped::Unsupported(self.unsupported));
        };
        if id.is_none() && notes.ids == NoteIdPolicy::RequireIdentified
            || channel >= notes.channels
            || key >= 128
            || !velocity.is_finite()
            || !(0. ..=1.).contains(&velocity)
        {
            return Err(invalid);
        }
        let note = Note {
            channel,
            pitch: key,
            note_id: native_id(id).ok_or(invalid)?,
            tuning: 0.,
            velocity,
            length: 0,
        };
        let kind = match phase {
            NotePhase::On => Kind::NoteOn(note),
            NotePhase::Off => Kind::NoteOff(note),
        };
        self.push(notes.bus, offset, kind, index, count)
    }

    fn push(
        &mut self,
        bus: u32,
        offset: u32,
        kind: Kind<'_>,
        index: usize,
        count: usize,
    ) -> Result<Mapped, Refusal> {
        match self.events.push(Event {
            header: Header {
                bus,
                offset,
                ppq_position: 0.,
                flags: 0,
            },
            kind,
        }) {
            Ok(()) => Ok(Mapped::Yes),
            Err(processing::Error::Capacity) => Err(Refusal::Events {
                count,
                maximum: self.event_capacity,
            }),
            Err(_) => Err(Refusal::Event(EventError::InvalidValue { index })),
        }
    }

    /// Queued-mode resolution of one lane batch against the translated block.
    /// Returns the event/point source and the single admitted context.
    pub(super) fn resolve(
        &mut self,
        events: &InputEvents,
        points: &InputPoints,
        context: Option<&Context>,
    ) -> Result<(Source, Option<Context>), Refusal> {
        self.resolve_block(events, points, context)
            .map_err(|refusal| self.record(refusal))
    }
    fn resolve_block(
        &mut self,
        events: &InputEvents,
        points: &InputPoints,
        context: Option<&Context>,
    ) -> Result<(Source, Option<Context>), Refusal> {
        let context = match (self.context, context) {
            (Some(_), Some(_)) => return Err(Refusal::TwoContexts),
            (Some(c), None) => Some(c),
            (None, c) => c.copied(),
        };
        let framework = !self.events.is_empty() || !self.points.points().is_empty();
        let lane = !events.is_empty() || !points.points().is_empty();
        let source = match (framework, lane) {
            (false, _) => Source::Lane,
            (true, false) => Source::Framework,
            (true, true) => {
                if self.merge == MergePolicy::Refuse {
                    return Err(Refusal::Mixed);
                }
                let capacity = self.point_capacity_merged;
                let (merged_events, merged_points) = &mut self.merged;
                let framework_first = self.merge == MergePolicy::FrameworkFirst;
                // Both inputs are offset-ordered by their push invariants, so
                // only capacity can fail here.
                merge_events(&self.events, events, framework_first, merged_events)
                    .map_err(|_| Refusal::MergedCapacity)?;
                merge_points(
                    self.points.points(),
                    points.points(),
                    framework_first,
                    merged_points,
                )
                .map_err(|error| match error {
                    processing::Error::Conflict => Refusal::MergeConflict,
                    processing::Error::Capacity if merged_points.points().len() < capacity => {
                        Refusal::MergedParameterIds
                    }
                    _ => Refusal::MergedCapacity,
                })?;
                Source::Merged
            }
        };
        Ok((source, context))
    }
    /// Merged storage, filled by [`Self::resolve`] when it returns
    /// [`Source::Merged`]; empty otherwise.
    pub(super) fn merged_input(&self) -> (&InputEvents, &[Point]) {
        (&self.merged.0, self.merged.1.points())
    }
}

enum Mapped {
    Yes,
    /// A same-offset value replaced under [`SameOffsetPolicy::LastWins`].
    Superseded,
    /// A parameter value without a route.
    Unmapped(EventPolicy),
    /// A kind this translation cannot represent or did not enable.
    Unsupported(EventPolicy),
}

fn native_id(id: Option<NoteId>) -> Option<i32> {
    match id {
        None => Some(-1),
        Some(NoteId(id)) => i32::try_from(id).ok(),
    }
}

/// SDK note expression type ID and normalized value for one standard
/// expression. Pressure has no SDK note expression and returns `None`; callers
/// map it to poly pressure first, so `None` is never expected.
fn expression_value(expression: NoteExpression, value: f64) -> Option<(u32, f64)> {
    Some(match expression {
        NoteExpression::Volume => (0, value / 4.),
        NoteExpression::Pan => (1, value),
        NoteExpression::Tuning => (2, value / 240. + 0.5),
        NoteExpression::Vibrato => (3, value),
        NoteExpression::Expression => (4, value),
        NoteExpression::Brightness => (5, value),
        NoteExpression::Pressure => return None,
    })
}

/// Stable merge of two offset-ordered event lists into cleared storage.
fn merge_events(
    framework: &InputEvents,
    lane: &InputEvents,
    framework_first: bool,
    out: &mut InputEvents,
) -> Result<(), processing::Error> {
    out.clear();
    let (mut f, mut l) = (0, 0);
    let (fr, lr) = (
        &framework.records[..framework.count],
        &lane.records[..lane.count],
    );
    while f < fr.len() || l < lr.len() {
        let take_framework = match (fr.get(f), lr.get(l)) {
            (Some(a), Some(b)) => a.offset < b.offset || (a.offset == b.offset && framework_first),
            (Some(_), None) => true,
            _ => false,
        };
        if take_framework {
            out.push_copy(framework, f)?;
            f += 1;
        } else {
            out.push_copy(lane, l)?;
            l += 1;
        }
    }
    Ok(())
}

/// Stable merge of two offset-ordered point lists into cleared storage.
/// Same-ID equal offsets across the sources remain an explicit conflict.
fn merge_points(
    framework: &[Point],
    lane: &[Point],
    framework_first: bool,
    out: &mut InputPoints,
) -> Result<(), processing::Error> {
    out.clear();
    let (mut f, mut l) = (0, 0);
    while f < framework.len() || l < lane.len() {
        let take_framework = match (framework.get(f), lane.get(l)) {
            (Some(a), Some(b)) => a.offset < b.offset || (a.offset == b.offset && framework_first),
            (Some(_), None) => true,
            _ => false,
        };
        if take_framework {
            out.push(framework[f])?;
            f += 1;
        } else {
            out.push(lane[l])?;
            l += 1;
        }
    }
    Ok(())
}

/// Map block-start framework transport to an SDK `ProcessContext`.
///
/// All-absent transport (`playing`, positions, tempo and details all `None`)
/// produces `None`: no context is passed. Otherwise `playing` and
/// `sample_position` must both be known, because the SDK has no availability
/// flag for either (`kPlaying` unset means stopped, and `projectTimeSamples`
/// is always read); neither is ever guessed. The sample position is copied
/// exactly as `i64`; `sample_rate` is the prepared rate.
///
/// Flags are set exactly for the fields present: `kPlaying`, `kTempoValid`
/// (finite positive tempo), `kProjectTimeMusicValid` (finite beats), and from
/// details `kRecording`, `kCycleActive`, `kTimeSigValid` (nonzero signature),
/// `kBarPositionValid` (finite bar start, requires beats) and `kCycleValid`
/// (finite ordered beat loop, requires an active loop and beats). Discontinuity,
/// pre-roll, tempo increment (requires tempo), bar number and sample loops
/// (require an active loop and a sample timeline, ordered) are validated but have
/// no SDK field and are not represented. System time, continuous time, SMPTE,
/// clock and chord flags are never set. Invalid values return
/// [`ProcessError::InvalidTransport`] without allocation.
pub fn transport_context(
    transport: Transport,
    sample_rate: f64,
) -> Result<Option<Context>, ProcessError> {
    let invalid = ProcessError::InvalidTransport;
    if !sample_rate.is_finite() || sample_rate <= 0. {
        return Err(invalid);
    }
    if transport.playing.is_none()
        && transport.sample_position.is_none()
        && transport.beat_position.is_none()
        && transport.tempo.is_none()
        && transport.details.is_none()
    {
        return Ok(None);
    }
    let playing = transport.playing.ok_or(invalid)?;
    let samples = transport.sample_position.ok_or(invalid)?;
    let mut context = Context::new(sample_rate, samples);
    if playing {
        context.state |= Context::PLAYING;
    }
    if let Some(tempo) = transport.tempo {
        if !tempo.is_finite() || tempo <= 0. {
            return Err(invalid);
        }
        context.state |= Context::TEMPO;
        context.tempo = tempo;
    }
    if let Some(beats) = transport.beat_position {
        if !beats.is_finite() {
            return Err(invalid);
        }
        context.state |= Context::MUSIC;
        context.project_time_music = beats;
    }
    if let Some(details) = transport.details {
        if details.recording {
            context.state |= Context::RECORDING;
        }
        if details.loop_active {
            context.state |= Context::CYCLE_ACTIVE;
        }
        if let Some((numerator, denominator)) = details.time_signature {
            if numerator == 0 || denominator == 0 {
                return Err(invalid);
            }
            context.state |= Context::TIME_SIGNATURE;
            context.time_signature_numerator = i32::from(numerator);
            context.time_signature_denominator = i32::from(denominator);
        }
        if let Some(increment) = details.tempo_increment
            && (transport.tempo.is_none() || !increment.is_finite())
        {
            return Err(invalid);
        }
        if let Some((start, _number)) = details.bar {
            if transport.beat_position.is_none() || !start.is_finite() {
                return Err(invalid);
            }
            context.state |= Context::BAR;
            context.bar_position_music = start;
        }
        if let Some((start, end)) = details.beat_loop {
            if !details.loop_active
                || transport.beat_position.is_none()
                || !start.is_finite()
                || !end.is_finite()
                || start >= end
            {
                return Err(invalid);
            }
            context.state |= Context::CYCLE;
            context.cycle_start_music = start;
            context.cycle_end_music = end;
        }
        if let Some((start, end)) = details.sample_loop
            && (!details.loop_active || transport.sample_position.is_none() || start >= end)
        {
            return Err(invalid);
        }
    }
    if !context.admitted(sample_rate) {
        return Err(invalid);
    }
    Ok(Some(context))
}

#[cfg(test)]
mod tests;
