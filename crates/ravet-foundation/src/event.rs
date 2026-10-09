/// A permanent product-defined identity, independent of names and field order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParameterId(pub u32);

/// Raw parameter edit boundaries. Pairing, ownership, cancellation and recording
/// are receiver policy; these events carry no scalar value or implicit state edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterGesturePhase {
    Begin,
    End,
}

/// Product-local note identity. The complete u32 range is portable; adapters
/// validate their narrower native limits. No automatic uniqueness/lifetime policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NoteId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotePhase {
    On,
    Off,
}

/// Matching scope within a routed note endpoint. None is a wildcard, not an
/// unknown field. IDs retain the full portable u32 range; backends check limits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoteTarget {
    pub channel: Option<u8>,
    pub key: Option<u8>,
    pub id: Option<NoteId>,
}
impl NoteTarget {
    pub fn is_valid(self) -> bool {
        self.channel.is_none_or(|channel| channel < 16) && self.key.is_none_or(|key| key < 128)
    }
}
/// Standard voice-expression units. Values replace the preceding expression;
/// receiver DSP owns application relative to its voice defaults and smoothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteExpression {
    /// Linear volume multiplier, strictly positive and <=4.
    Volume,
    /// 0 left, 0.5 center, 1 right.
    Pan,
    /// Relative equal-tempered semitones, -120..=120.
    Tuning,
    Vibrato,
    Expression,
    Brightness,
    Pressure,
}
impl NoteExpression {
    /// Standard finite range; vibrato/expression/brightness/pressure use 0..=1.
    pub fn is_valid_value(self, value: f64) -> bool {
        value.is_finite()
            && match self {
                Self::Volume => value > 0. && value <= 4.,
                Self::Tuning => (-120. ..=120.).contains(&value),
                _ => (0. ..=1.).contains(&value),
            }
    }
}

/// Borrowed events never own heap data. Parameter values use physical units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventData {
    /// Physical value for a matching scope within a routed note input endpoint.
    /// Full f64 precision; receiver owns voice/global precedence, ranges, state
    /// and smoothing. Even an all-wildcard target remains endpoint-scoped.
    ParameterValueScoped {
        id: ParameterId,
        target: NoteTarget,
        value: f64,
    },
    /// Signed replacement modulation for a matching endpoint scope, separate
    /// from saved base state. Full finite f64 range; receiver owns application.
    ParameterModulationScoped {
        id: ParameterId,
        target: NoteTarget,
        amount: f64,
    },
    /// Key release for a matching scope within a routed endpoint. Velocity is
    /// finite 0..=1; receiver owns matching, release envelope and voice lifetime.
    NoteOffScoped {
        target: NoteTarget,
        velocity: f32,
    },
    /// Immediate choke of matching voices; no velocity or automatic allocation.
    /// Receiver DSP defines its actual termination/fade policy.
    NoteChoke {
        target: NoteTarget,
    },
    /// Processor-to-controller completion notification for matching voices on
    /// the originating note INPUT endpoint. This is distinct from key release
    /// and choke. The carrier offset only orders delivery, not voice termination
    /// time. Receivers own identity reuse and lifecycle bookkeeping.
    NoteEnd {
        target: NoteTarget,
    },
    /// Sample-timed standard expression with full f64 value and wildcard target.
    /// No voice matching, base-state edit, unit conversion or reset is inferred.
    NoteExpression {
        target: NoteTarget,
        expression: NoteExpression,
        value: f64,
    },
    /// Concrete channel/key note with explicit identity. On velocity zero remains
    /// an On; receivers own voice matching/lifetime, duplication and release policy.
    NoteWithId {
        id: NoteId,
        phase: NotePhase,
        channel: u8,
        key: u8,
        velocity: f32,
    },
    /// An explicit global parameter edit boundary; receivers declare support.
    ParameterGesture {
        id: ParameterId,
        phase: ParameterGesturePhase,
    },
    /// Global physical-unit modulation amount, independent of saved base value.
    /// Amount replaces the prior modulation amount; consumer DSP owns combining,
    /// clipping, smoothing, persistence and reset policy. No per-note scope.
    ParameterModulation {
        id: ParameterId,
        amount: f32,
    },
    ParameterValue {
        id: ParameterId,
        value: f32,
    },
    NoteOn {
        channel: u8,
        key: u8,
        velocity: f32,
    },
    NoteOff {
        channel: u8,
        key: u8,
        velocity: f32,
    },
    /// Validated MIDI 1.0 short message; adapters preserve sample timing.
    Midi(crate::MidiMessage),
    /// One framed UMP, preserving numeric words and sample timing. Message
    /// semantics and negotiated protocol are processor/adapter responsibilities.
    Ump(crate::UmpPacket),
}

impl EventData {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::ParameterValueScoped { target, value, .. } => {
                target.is_valid() && value.is_finite()
            }
            Self::ParameterModulationScoped { target, amount, .. } => {
                target.is_valid() && amount.is_finite()
            }
            Self::NoteOffScoped { target, velocity } => {
                target.is_valid() && velocity.is_finite() && (0. ..=1.).contains(&velocity)
            }
            Self::NoteChoke { target } | Self::NoteEnd { target } => target.is_valid(),
            Self::NoteExpression {
                target,
                expression,
                value,
            } => target.is_valid() && expression.is_valid_value(value),
            Self::ParameterGesture { .. } => true,
            Self::ParameterValue { value, .. } => value.is_finite(),
            Self::ParameterModulation { amount, .. } => amount.is_finite(),
            Self::Midi(_) | Self::Ump(_) => true,
            Self::NoteWithId {
                channel,
                key,
                velocity,
                ..
            }
            | Self::NoteOn {
                channel,
                key,
                velocity,
            }
            | Self::NoteOff {
                channel,
                key,
                velocity,
            } => {
                channel < 16 && key < 128 && velocity.is_finite() && (0.0..=1.0).contains(&velocity)
            }
        }
    }
}

/// Applied before the sample at `frame_offset`, preserving same-frame order.
/// [`EventData::NoteEnd`] is a completion notification: its offset only orders
/// delivery and carries no exact termination time.
///
/// For nonempty blocks, the offset must be less than the frame count. Empty
/// blocks allow only offset zero and apply events without advancing DSP time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub frame_offset: usize,
    pub data: EventData,
}

/// Malformed event streams are rejected as a whole before processing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventError {
    TooMany {
        count: usize,
        maximum: usize,
    },
    OffsetOutOfRange {
        index: usize,
        offset: usize,
        frames: usize,
    },
    Unordered {
        index: usize,
    },
    InvalidValue {
        index: usize,
    },
}

/// Optional host context; absence does not imply a stopped transport.
///
/// Adapters supply finite positions and a finite positive tempo when known.
/// `discontinuity` indicates a seek/loop jump; each processor documents whether
/// it resets history. The reference gain effect has no transport dependence.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transport {
    pub playing: Option<bool>,
    pub sample_position: Option<i64>,
    pub beat_position: Option<f64>,
    pub tempo: Option<f64>,
    pub discontinuity: bool,
    /// Additional block-start metadata. None means unavailable, not all false.
    pub details: Option<TransportDetails>,
}

/// Explicit recording/loop/pre-roll state with independently available metadata.
/// No field changes DSP state or advances a timeline automatically. Adapters
/// validate the public raw values before crossing their backend boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TransportDetails {
    pub recording: bool,
    pub loop_active: bool,
    pub within_pre_roll: bool,
    /// Positive numerator and denominator; no power-of-two restriction.
    pub time_signature: Option<(u16, u16)>,
    /// BPM per sample; finite, requires a known tempo. Not an end-of-block tempo.
    pub tempo_increment: Option<f64>,
    /// Active loop [start, end) in beats, finite and strictly increasing.
    /// Requires a known beat timeline; inactive loops are not represented.
    pub beat_loop: Option<(f64, f64)>,
    /// Active loop [start, end) in samples, strictly increasing.
    /// Requires a known sample timeline; conversion may quantize endpoints.
    pub sample_loop: Option<(i64, i64)>,
    /// Current bar start in beats and zero-based bar number; requires beats.
    /// Negative starts and bar numbers are allowed for pre-roll.
    pub bar: Option<(f64, i32)>,
}

impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany { count, maximum } => {
                write!(f, "block has {count} events; capacity is {maximum}")
            }
            Self::OffsetOutOfRange {
                index,
                offset,
                frames,
            } => write!(
                f,
                "event {index} offset {offset} is outside a {frames}-frame block"
            ),
            Self::Unordered { index } => write!(f, "event {index} precedes the previous event"),
            Self::InvalidValue { index } => write!(f, "event {index} contains an invalid value"),
        }
    }
}
impl std::error::Error for EventError {}

/// Caller-owned, bounded output events. Construct a fresh writer per callback.
///
/// Push rejects invalid values, unordered/out-of-block offsets and full storage
/// without changing the accepted prefix. A zero-frame callback accepts offset 0.
/// No allocation, event replacement or hidden delivery retry is performed.
#[derive(Debug)]
pub struct OutputEvents<'a> {
    storage: &'a mut [Event],
    len: usize,
    frames: usize,
}
impl<'a> OutputEvents<'a> {
    pub fn new(storage: &'a mut [Event], frames: usize) -> Self {
        Self {
            storage,
            len: 0,
            frames,
        }
    }
    pub fn capacity(&self) -> usize {
        self.storage.len()
    }
    pub fn frames(&self) -> usize {
        self.frames
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn as_slice(&self) -> &[Event] {
        &self.storage[..self.len]
    }
    pub fn clear(&mut self) {
        self.len = 0;
    }
    pub fn push(&mut self, event: Event) -> Result<(), EventError> {
        let index = self.len;
        if index == self.capacity() {
            return Err(EventError::TooMany {
                count: index.saturating_add(1),
                maximum: self.capacity(),
            });
        }
        if (self.frames == 0 && event.frame_offset != 0)
            || (self.frames > 0 && event.frame_offset >= self.frames)
        {
            return Err(EventError::OffsetOutOfRange {
                index,
                offset: event.frame_offset,
                frames: self.frames,
            });
        }
        if index > 0 && event.frame_offset < self.storage[index - 1].frame_offset {
            return Err(EventError::Unordered { index });
        }
        if !event.data.valid() {
            return Err(EventError::InvalidValue { index });
        }
        self.storage[index] = event;
        self.len += 1;
        Ok(())
    }
}
