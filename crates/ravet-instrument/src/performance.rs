//! Explicit native note ownership within one already routed endpoint.

use ravet_foundation::{
    Event, EventData, EventError, NoteExpression, NoteId, NotePhase, NoteTarget,
};

/// Exact standard f64 replacement values, independent of DSP application.
/// Defaults: volume/expression/brightness 1, pan .5, tuning/vibrato/pressure 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformanceExpression([f64; 7]);

impl Default for PerformanceExpression {
    fn default() -> Self {
        Self([1., 0.5, 0., 0., 1., 1., 0.])
    }
}

impl PerformanceExpression {
    /// Preserves the accepted value's bits, including signed zero and subnormals.
    pub fn get(self, expression: NoteExpression) -> f64 {
        self.0[expression_index(expression)]
    }
}

fn expression_index(expression: NoteExpression) -> usize {
    match expression {
        NoteExpression::Volume => 0,
        NoteExpression::Pan => 1,
        NoteExpression::Tuning => 2,
        NoteExpression::Vibrato => 3,
        NoteExpression::Expression => 4,
        NoteExpression::Brightness => 5,
        NoteExpression::Pressure => 6,
    }
}

/// Key release keeps ownership until the DSP explicitly finishes the voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformancePhase {
    Idle,
    Held,
    Released,
}

/// One fixed slot. Owned targets always have concrete channel/key; ID is optional.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformanceVoice {
    pub target: Option<NoteTarget>,
    pub phase: PerformancePhase,
    /// Unquantized normalized f32; a zero-velocity On remains Held.
    pub velocity: f32,
    pub release_velocity: f32,
    pub expression: PerformanceExpression,
    age: u8,
}

impl Default for PerformanceVoice {
    fn default() -> Self {
        Self {
            target: None,
            phase: PerformancePhase::Idle,
            velocity: 0.,
            release_velocity: 0.,
            expression: PerformanceExpression::default(),
            age: 0,
        }
    }
}

/// The caller retriggers its DSP slot; a same-ID retrigger emits no old completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerformanceStart {
    pub index: usize,
    pub retriggered: bool,
}

/// Fixed results for one event. Completed targets retain the original address/ID.
/// Completion is distinct from release; the consumer owns output delivery policy.
#[derive(Clone, Debug, PartialEq)]
pub struct PerformanceChanges<const N: usize> {
    pub changed: u128,
    pub released: u128,
    pub started: Option<PerformanceStart>,
    completed: [Option<NoteTarget>; N],
}

impl<const N: usize> Default for PerformanceChanges<N> {
    fn default() -> Self {
        Self {
            changed: 0,
            released: 0,
            started: None,
            completed: [None; N],
        }
    }
}

impl<const N: usize> PerformanceChanges<N> {
    /// Completed physical slot and original target, in ascending slot order.
    pub fn completions(&self) -> impl Iterator<Item = (usize, NoteTarget)> + '_ {
        self.completed
            .iter()
            .enumerate()
            .filter_map(|(index, target)| target.map(|target| (index, target)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceError {
    InvalidConfiguration,
    InvalidIndex,
    InvalidValue,
    /// An owned ID cannot move to another address before completion/reset.
    IdentityAddressConflict {
        id: NoteId,
    },
    /// Only normalized note On/Off, scoped Off/choke and standard expressions.
    /// MIDI/UMP, NoteEnd and parameter semantics need an explicit separate route.
    UnsupportedEvent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceBatchError {
    FramesOutOfRange {
        frames: usize,
        maximum: usize,
    },
    Event(EventError),
    Semantic {
        index: usize,
        error: PerformanceError,
    },
}

/// Immediate completions caused by input steals/chokes; does not include later
/// DSP envelope completion. No output is published or storage reserved implicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerformanceBatchInfo {
    pub immediate_completions: usize,
}

/// Allocation-free 1..128-slot normalized/native performance receiver.
///
/// All target fields are conjunctive; absent fields are wildcards within this
/// routed endpoint. Unidentified duplicate On events stack. Off releases every
/// held match and retains velocity/expressions through the tail; repeated Off
/// does not restart release or replace its velocity. Expressions update held
/// and released matches, but are not retained for future notes. Volume zero
/// mutes without completing. Concrete duplicate IDs retrigger their existing
/// address, resetting expressions; another address rejects before mutation.
/// Stealing prefers idle, oldest released, then oldest held and reports the
/// old target as completed. Choke removes matching voices immediately; a DSP
/// consumer must implement that policy or choose another receiver.
///
/// There are no pedals, MIDI conversion, smoothing, oscillator, endpoint router,
/// thread synchronization or format export. Rank ages stay bounded 0..N-1.
#[derive(Clone, Debug, PartialEq)]
pub struct PerformanceVoices<const N: usize> {
    voices: [PerformanceVoice; N],
}

impl<const N: usize> PerformanceVoices<N> {
    pub fn new() -> Result<Self, PerformanceError> {
        if !(1..=128).contains(&N) {
            return Err(PerformanceError::InvalidConfiguration);
        }
        Ok(Self {
            voices: [PerformanceVoice::default(); N],
        })
    }

    pub fn voices(&self) -> &[PerformanceVoice; N] {
        &self.voices
    }

    /// Explicit lifecycle reset discards ownership without generating events.
    /// The consumer coordinates host/controller reset and its own DSP history.
    pub fn reset(&mut self) {
        self.voices.fill(PerformanceVoice::default());
    }

    /// DSP-owned completion, including a consumer-terminated held note. An idle
    /// slot returns None; an invalid index leaves the receiver unchanged.
    pub fn finish(&mut self, index: usize) -> Result<Option<NoteTarget>, PerformanceError> {
        if index >= N {
            return Err(PerformanceError::InvalidIndex);
        }
        let target = self.voices[index].target;
        self.remove(index);
        Ok(target)
    }

    fn remove(&mut self, index: usize) {
        if self.voices[index].target.is_some() {
            let age = self.voices[index].age;
            self.voices[index] = PerformanceVoice::default();
            for voice in &mut self.voices {
                if voice.target.is_some() && voice.age > age {
                    voice.age -= 1;
                }
            }
        }
    }

    fn start(
        &mut self,
        target: NoteTarget,
        velocity: f32,
    ) -> Result<PerformanceChanges<N>, PerformanceError> {
        let duplicate = target.id.and_then(|id| {
            self.voices
                .iter()
                .position(|voice| voice.target.is_some_and(|owned| owned.id == Some(id)))
        });
        if let Some(index) = duplicate
            && self.voices[index].target != Some(target)
        {
            return Err(PerformanceError::IdentityAddressConflict {
                id: target.id.expect("duplicate requires identity"),
            });
        }
        let index = duplicate.unwrap_or_else(|| {
            (0..N)
                .min_by_key(|&index| {
                    let voice = self.voices[index];
                    let priority = match voice.phase {
                        PerformancePhase::Idle => 0,
                        PerformancePhase::Released => 1,
                        PerformancePhase::Held => 2,
                    };
                    (priority, u8::MAX - voice.age, index)
                })
                .expect("nonempty validated bank")
        });
        let mut changes = PerformanceChanges::default();
        if duplicate.is_none() {
            changes.completed[index] = self.voices[index].target;
        }
        self.remove(index);
        for voice in &mut self.voices {
            if voice.target.is_some() {
                voice.age += 1;
            }
        }
        self.voices[index] = PerformanceVoice {
            target: Some(target),
            phase: PerformancePhase::Held,
            velocity,
            ..PerformanceVoice::default()
        };
        changes.changed = 1u128 << index;
        changes.started = Some(PerformanceStart {
            index,
            retriggered: duplicate.is_some(),
        });
        Ok(changes)
    }

    /// Validate one event before changing any receiver state. See [`Self::preflight`]
    /// for a complete timed-stream check before a consumer changes DSP/state.
    pub fn apply(&mut self, data: EventData) -> Result<PerformanceChanges<N>, PerformanceError> {
        let (target, action) = match data {
            EventData::NoteWithId {
                id,
                phase,
                channel,
                key,
                velocity,
            } => (
                NoteTarget {
                    channel: Some(channel),
                    key: Some(key),
                    id: Some(id),
                },
                match phase {
                    NotePhase::On => Action::Start(velocity),
                    NotePhase::Off => Action::Release(velocity),
                },
            ),
            EventData::NoteOn {
                channel,
                key,
                velocity,
            } => (
                NoteTarget {
                    channel: Some(channel),
                    key: Some(key),
                    id: None,
                },
                Action::Start(velocity),
            ),
            EventData::NoteOff {
                channel,
                key,
                velocity,
            } => (
                NoteTarget {
                    channel: Some(channel),
                    key: Some(key),
                    id: None,
                },
                Action::Release(velocity),
            ),
            EventData::NoteOffScoped { target, velocity } => (target, Action::Release(velocity)),
            EventData::NoteChoke { target } => (target, Action::Choke),
            EventData::NoteExpression {
                target,
                expression,
                value,
            } => {
                if !expression.is_valid_value(value) {
                    return Err(PerformanceError::InvalidValue);
                }
                (target, Action::Expression(expression, value))
            }
            _ => return Err(PerformanceError::UnsupportedEvent),
        };
        if !target.is_valid()
            || matches!(action, Action::Start(v) | Action::Release(v) if !v.is_finite() || !(0. ..=1.).contains(&v))
        {
            return Err(PerformanceError::InvalidValue);
        }
        if let Action::Start(velocity) = action {
            return self.start(target, velocity);
        }
        let mut changes = PerformanceChanges::default();
        for index in 0..N {
            if !self.voices[index]
                .target
                .is_some_and(|owned| matches_target(target, owned))
            {
                continue;
            }
            match action {
                Action::Release(velocity) => {
                    if self.voices[index].phase == PerformancePhase::Held {
                        self.voices[index].phase = PerformancePhase::Released;
                        self.voices[index].release_velocity = velocity;
                        changes.changed |= 1u128 << index;
                        changes.released |= 1u128 << index;
                    }
                }
                Action::Choke => {
                    changes.completed[index] = self.voices[index].target;
                    self.remove(index);
                    changes.changed |= 1u128 << index;
                }
                Action::Expression(expression, value) => {
                    let old = &mut self.voices[index].expression.0[expression_index(expression)];
                    if old.to_bits() != value.to_bits() {
                        *old = value;
                        changes.changed |= 1u128 << index;
                    }
                }
                Action::Start(_) => unreachable!("start handled before matching"),
            }
        }
        Ok(changes)
    }

    /// Simulate the entire ordered stream on fixed copied storage without changing
    /// this bank. Limits are 1024 events and 65536 frames; zero-frame events use 0.
    /// Equal-offset order is retained. Identity conflicts/unsupported/invalid late
    /// events reject atomically. Success counts immediate steal/choke completions.
    ///
    /// The consumer must keep ownership unchanged between preflight and replay,
    /// apply the same events at their sample boundaries, separately budget DSP
    /// completions, and stage publication until its own whole operation succeeds.
    /// This method does not reserve an output buffer or mutate consumer DSP.
    pub fn preflight(
        &self,
        events: &[Event],
        frames: usize,
    ) -> Result<PerformanceBatchInfo, PerformanceBatchError> {
        if frames > 65536 {
            return Err(PerformanceBatchError::FramesOutOfRange {
                frames,
                maximum: 65536,
            });
        }
        if events.len() > 1024 {
            return Err(PerformanceBatchError::Event(EventError::TooMany {
                count: events.len(),
                maximum: 1024,
            }));
        }
        let mut candidate = self.clone();
        let mut immediate_completions = 0;
        for (index, event) in events.iter().enumerate() {
            if (frames == 0 && event.frame_offset != 0)
                || (frames > 0 && event.frame_offset >= frames)
            {
                return Err(PerformanceBatchError::Event(EventError::OffsetOutOfRange {
                    index,
                    offset: event.frame_offset,
                    frames,
                }));
            }
            if index > 0 && event.frame_offset < events[index - 1].frame_offset {
                return Err(PerformanceBatchError::Event(EventError::Unordered {
                    index,
                }));
            }
            let changes = candidate
                .apply(event.data)
                .map_err(|error| PerformanceBatchError::Semantic { index, error })?;
            immediate_completions += changes.completions().count();
        }
        Ok(PerformanceBatchInfo {
            immediate_completions,
        })
    }
}

#[derive(Clone, Copy)]
enum Action {
    Start(f32),
    Release(f32),
    Choke,
    Expression(NoteExpression, f64),
}

fn matches_target(target: NoteTarget, owned: NoteTarget) -> bool {
    target
        .channel
        .is_none_or(|channel| owned.channel == Some(channel))
        && target.key.is_none_or(|key| owned.key == Some(key))
        && target.id.is_none_or(|id| owned.id == Some(id))
}
