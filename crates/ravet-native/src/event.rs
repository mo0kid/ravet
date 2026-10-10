//! Version-one in-process native event records. This is not a wire/file format.
use ravet_foundation::{
    Event, EventData, MidiMessage, NoteExpression, NoteId, NotePhase, NoteTarget, OutputEvents,
    ParameterGesturePhase, ParameterId, UmpPacket,
};

/// Stable v1 tags. Note identities and scoped targets retain the full u32 range.
pub mod native_event_kind {
    pub const PARAMETER_VALUE: u32 = 1;
    pub const PARAMETER_MODULATION: u32 = 2;
    pub const PARAMETER_GESTURE: u32 = 3;
    pub const NOTE_ON: u32 = 4;
    pub const NOTE_OFF: u32 = 5;
    pub const NOTE_ON_WITH_ID: u32 = 6;
    pub const NOTE_OFF_WITH_ID: u32 = 7;
    pub const NOTE_OFF_SCOPED: u32 = 8;
    pub const NOTE_CHOKE: u32 = 9;
    pub const NOTE_END: u32 = 10;
    pub const NOTE_EXPRESSION: u32 = 11;
    pub const PARAMETER_VALUE_SCOPED: u32 = 12;
    pub const PARAMETER_MODULATION_SCOPED: u32 = 13;
    pub const MIDI: u32 = 14;
    pub const UMP: u32 = 15;
}
/// Fixed C layout matching include/RavetNativeEvent.h. Zero unused fields; scope
/// bits 1/2/4 select channel/key/id (unset means wildcard). detail selects gesture
/// (0 begin/1 end), expression (0 volume,1 pan,2 tuning,3 vibrato,4 expression,
/// 5 brightness,6 pressure), MIDI byte count or UMP word count. MIDI packs its
/// bytes in increasing shifts of `words[0]`; UMP retains numeric words in order.
///
/// value stores exact widened f32 for global controls/velocities, and full f64
/// for scoped controls/expressions. Decoding rejects lossy f32 conversion. This
/// record defines neither endpoint routing nor host-specific scalar conversion.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeEventV1 {
    pub offset: u32,
    pub kind: u32,
    pub parameter_id: u32,
    pub note_id: u32,
    pub scope: u32,
    pub channel: u32,
    pub key: u32,
    pub detail: u32,
    pub value: f64,
    pub words: [u32; 4],
}
/// Existing physical global parameter record, also matching the Gain reference
/// C event layout. Kept separate from the tagged v1 performance record.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeParameterEvent {
    pub offset: u32,
    pub id: u32,
    pub value: f32,
}
/// Entire destination prefixes remain unchanged on any conversion failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeEventError {
    Capacity,
    InvalidRecord { index: usize },
    OffsetOutOfRange { index: usize },
    Unordered { index: usize },
}
impl std::fmt::Display for NativeEventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native event conversion: {self:?}")
    }
}
impl std::error::Error for NativeEventError {}
const EMPTY: Event = Event {
    frame_offset: 0,
    data: EventData::ParameterValue {
        id: ParameterId(0),
        value: 0.,
    },
};
fn valid(data: EventData) -> bool {
    let mut storage = [EMPTY];
    OutputEvents::new(&mut storage, 0)
        .push(Event {
            frame_offset: 0,
            data,
        })
        .is_ok()
}
fn scalar(value: f64) -> Option<f32> {
    let small = value as f32;
    (value.is_finite() && f64::from(small).to_bits() == value.to_bits()).then_some(small)
}
impl NativeEventV1 {
    /// Encode one valid portable event, independent of a block's timing/order.
    /// All unused fields are canonical zero; unrepresentable offsets reject.
    pub fn from_event(event: Event) -> Result<Self, NativeEventError> {
        use native_event_kind as k;
        if !valid(event.data) {
            return Err(NativeEventError::InvalidRecord { index: 0 });
        }
        let mut out = Self {
            offset: u32::try_from(event.frame_offset)
                .map_err(|_| NativeEventError::OffsetOutOfRange { index: 0 })?,
            ..Self::default()
        };
        let mut target = None;
        match event.data {
            EventData::ParameterValue { id, value } => {
                out.kind = k::PARAMETER_VALUE;
                out.parameter_id = id.0;
                out.value = f64::from(value);
            }
            EventData::ParameterModulation { id, amount } => {
                out.kind = k::PARAMETER_MODULATION;
                out.parameter_id = id.0;
                out.value = f64::from(amount);
            }
            EventData::ParameterGesture { id, phase } => {
                out.kind = k::PARAMETER_GESTURE;
                out.parameter_id = id.0;
                out.detail = u32::from(phase == ParameterGesturePhase::End);
            }
            EventData::NoteOn {
                channel,
                key,
                velocity,
            } => {
                out.kind = k::NOTE_ON;
                out.channel = u32::from(channel);
                out.key = u32::from(key);
                out.value = f64::from(velocity);
            }
            EventData::NoteOff {
                channel,
                key,
                velocity,
            } => {
                out.kind = k::NOTE_OFF;
                out.channel = u32::from(channel);
                out.key = u32::from(key);
                out.value = f64::from(velocity);
            }
            EventData::NoteWithId {
                id,
                phase,
                channel,
                key,
                velocity,
            } => {
                out.kind = if phase == NotePhase::On {
                    k::NOTE_ON_WITH_ID
                } else {
                    k::NOTE_OFF_WITH_ID
                };
                out.note_id = id.0;
                out.channel = u32::from(channel);
                out.key = u32::from(key);
                out.value = f64::from(velocity);
            }
            EventData::NoteOffScoped {
                target: t,
                velocity,
            } => {
                out.kind = k::NOTE_OFF_SCOPED;
                target = Some(t);
                out.value = f64::from(velocity);
            }
            EventData::NoteChoke { target: t } => {
                out.kind = k::NOTE_CHOKE;
                target = Some(t);
            }
            EventData::NoteEnd { target: t } => {
                out.kind = k::NOTE_END;
                target = Some(t);
            }
            EventData::NoteExpression {
                target: t,
                expression,
                value,
            } => {
                out.kind = k::NOTE_EXPRESSION;
                target = Some(t);
                out.value = value;
                out.detail = match expression {
                    NoteExpression::Volume => 0,
                    NoteExpression::Pan => 1,
                    NoteExpression::Tuning => 2,
                    NoteExpression::Vibrato => 3,
                    NoteExpression::Expression => 4,
                    NoteExpression::Brightness => 5,
                    NoteExpression::Pressure => 6,
                };
            }
            EventData::ParameterValueScoped {
                id,
                target: t,
                value,
            } => {
                out.kind = k::PARAMETER_VALUE_SCOPED;
                target = Some(t);
                out.parameter_id = id.0;
                out.value = value;
            }
            EventData::ParameterModulationScoped {
                id,
                target: t,
                amount,
            } => {
                out.kind = k::PARAMETER_MODULATION_SCOPED;
                target = Some(t);
                out.parameter_id = id.0;
                out.value = amount;
            }
            EventData::Midi(message) => {
                out.kind = k::MIDI;
                out.detail = message.as_bytes().len() as u32;
                for (index, byte) in message.as_bytes().iter().enumerate() {
                    out.words[0] |= u32::from(*byte) << (index * 8);
                }
            }
            EventData::Ump(packet) => {
                out.kind = k::UMP;
                out.detail = packet.as_words().len() as u32;
                out.words[..packet.as_words().len()].copy_from_slice(packet.as_words());
            }
        }
        if let Some(t) = target {
            if let Some(channel) = t.channel {
                out.scope |= 1;
                out.channel = u32::from(channel);
            }
            if let Some(key) = t.key {
                out.scope |= 2;
                out.key = u32::from(key);
            }
            if let Some(id) = t.id {
                out.scope |= 4;
                out.note_id = id.0;
            }
        }
        Ok(out)
    }
    /// Decode strict v1 fields; unknown tags, scope bits, unused nonzero fields,
    /// invalid framing/scalars and lossy f32 values reject. No allocation.
    pub fn to_event(self) -> Result<Event, NativeEventError> {
        use native_event_kind as k;
        let invalid = NativeEventError::InvalidRecord { index: 0 };
        let target = NoteTarget {
            channel: if self.scope & 1 != 0 {
                Some(u8::try_from(self.channel).map_err(|_| invalid)?)
            } else {
                None
            },
            key: if self.scope & 2 != 0 {
                Some(u8::try_from(self.key).map_err(|_| invalid)?)
            } else {
                None
            },
            id: (self.scope & 4 != 0).then_some(NoteId(self.note_id)),
        };
        let channel = u8::try_from(self.channel).map_err(|_| invalid)?;
        let key = u8::try_from(self.key).map_err(|_| invalid)?;
        let data = match self.kind {
            k::PARAMETER_VALUE => EventData::ParameterValue {
                id: ParameterId(self.parameter_id),
                value: scalar(self.value).ok_or(invalid)?,
            },
            k::PARAMETER_MODULATION => EventData::ParameterModulation {
                id: ParameterId(self.parameter_id),
                amount: scalar(self.value).ok_or(invalid)?,
            },
            k::PARAMETER_GESTURE => EventData::ParameterGesture {
                id: ParameterId(self.parameter_id),
                phase: match self.detail {
                    0 => ParameterGesturePhase::Begin,
                    1 => ParameterGesturePhase::End,
                    _ => return Err(invalid),
                },
            },
            k::NOTE_ON => EventData::NoteOn {
                channel,
                key,
                velocity: scalar(self.value).ok_or(invalid)?,
            },
            k::NOTE_OFF => EventData::NoteOff {
                channel,
                key,
                velocity: scalar(self.value).ok_or(invalid)?,
            },
            k::NOTE_ON_WITH_ID | k::NOTE_OFF_WITH_ID => EventData::NoteWithId {
                id: NoteId(self.note_id),
                phase: if self.kind == k::NOTE_ON_WITH_ID {
                    NotePhase::On
                } else {
                    NotePhase::Off
                },
                channel,
                key,
                velocity: scalar(self.value).ok_or(invalid)?,
            },
            k::NOTE_OFF_SCOPED => EventData::NoteOffScoped {
                target,
                velocity: scalar(self.value).ok_or(invalid)?,
            },
            k::NOTE_CHOKE => EventData::NoteChoke { target },
            k::NOTE_END => EventData::NoteEnd { target },
            k::NOTE_EXPRESSION => EventData::NoteExpression {
                target,
                expression: match self.detail {
                    0 => NoteExpression::Volume,
                    1 => NoteExpression::Pan,
                    2 => NoteExpression::Tuning,
                    3 => NoteExpression::Vibrato,
                    4 => NoteExpression::Expression,
                    5 => NoteExpression::Brightness,
                    6 => NoteExpression::Pressure,
                    _ => return Err(invalid),
                },
                value: self.value,
            },
            k::PARAMETER_VALUE_SCOPED => EventData::ParameterValueScoped {
                id: ParameterId(self.parameter_id),
                target,
                value: self.value,
            },
            k::PARAMETER_MODULATION_SCOPED => EventData::ParameterModulationScoped {
                id: ParameterId(self.parameter_id),
                target,
                amount: self.value,
            },
            k::MIDI => {
                if !(1..=3).contains(&self.detail) {
                    return Err(invalid);
                }
                let bytes = self.words[0].to_le_bytes();
                EventData::Midi(
                    MidiMessage::from_bytes(&bytes[..self.detail as usize]).map_err(|_| invalid)?,
                )
            }
            k::UMP => {
                if !(1..=4).contains(&self.detail) {
                    return Err(invalid);
                }
                EventData::Ump(
                    UmpPacket::from_words(&self.words[..self.detail as usize])
                        .map_err(|_| invalid)?,
                )
            }
            _ => return Err(invalid),
        };
        let event = Event {
            frame_offset: self.offset as usize,
            data,
        };
        let canonical = Self::from_event(event)?;
        // Compare value bits so unused negative zero rejects and used -0 survives.
        if canonical != self || canonical.value.to_bits() != self.value.to_bits() {
            return Err(invalid);
        }
        Ok(event)
    }
}
fn timing(
    offset: usize,
    previous: usize,
    index: usize,
    frames: usize,
) -> Result<(), NativeEventError> {
    if (frames == 0 && offset != 0) || (frames > 0 && offset >= frames) {
        return Err(NativeEventError::OffsetOutOfRange { index });
    }
    if index > 0 && offset < previous {
        return Err(NativeEventError::Unordered { index });
    }
    Ok(())
}
fn indexed(error: NativeEventError, index: usize) -> NativeEventError {
    match error {
        NativeEventError::OffsetOutOfRange { .. } => NativeEventError::OffsetOutOfRange { index },
        _ => NativeEventError::InvalidRecord { index },
    }
}
/// Decode at most 1024 records into caller-prepared storage. Validate every
/// record and timing before writing any destination; preserve same-frame order
/// and destination tails. Zero-frame blocks accept only offset zero.
pub fn decode_native_events<'a>(
    records: &[NativeEventV1],
    frames: usize,
    output: &'a mut [Event],
) -> Result<&'a [Event], NativeEventError> {
    if records.len() > 1024 || records.len() > output.len() {
        return Err(NativeEventError::Capacity);
    }
    let mut previous = 0;
    for (index, record) in records.iter().enumerate() {
        let event = record.to_event().map_err(|e| indexed(e, index))?;
        timing(event.frame_offset, previous, index, frames)?;
        previous = event.frame_offset;
    }
    for (out, record) in output.iter_mut().zip(records) {
        *out = record.to_event().expect("preflight immutable records");
    }
    Ok(&output[..records.len()])
}
/// Encode at most 1024 valid timed events atomically into caller-owned records.
/// No allocation or native delivery; host refusal remains consumer policy.
pub fn encode_native_events<'a>(
    events: &[Event],
    frames: usize,
    output: &'a mut [NativeEventV1],
) -> Result<&'a [NativeEventV1], NativeEventError> {
    if events.len() > 1024 || events.len() > output.len() {
        return Err(NativeEventError::Capacity);
    }
    let mut previous = 0;
    for (index, event) in events.iter().enumerate() {
        NativeEventV1::from_event(*event).map_err(|e| indexed(e, index))?;
        timing(event.frame_offset, previous, index, frames)?;
        previous = event.frame_offset;
    }
    for (out, event) in output.iter_mut().zip(events) {
        *out = NativeEventV1::from_event(*event).expect("preflight immutable events");
    }
    Ok(&output[..events.len()])
}
/// Decode the legacy global parameter layout with the same whole-stream timing,
/// finite-value and 1024-event limits. Used by the native Gain reference.
pub fn decode_parameter_events<'a>(
    records: &[NativeParameterEvent],
    frames: usize,
    output: &'a mut [Event],
) -> Result<&'a [Event], NativeEventError> {
    if records.len() > 1024 || records.len() > output.len() {
        return Err(NativeEventError::Capacity);
    }
    let mut previous = 0;
    for (index, record) in records.iter().enumerate() {
        if !record.value.is_finite() {
            return Err(NativeEventError::InvalidRecord { index });
        }
        timing(record.offset as usize, previous, index, frames)?;
        previous = record.offset as usize;
    }
    for (out, record) in output.iter_mut().zip(records) {
        *out = Event {
            frame_offset: record.offset as usize,
            data: EventData::ParameterValue {
                id: ParameterId(record.id),
                value: record.value,
            },
        };
    }
    Ok(&output[..records.len()])
}
