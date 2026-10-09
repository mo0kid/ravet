use clack_plugin::events::{
    Pckn,
    event_types::{
        Midi2Event, MidiEvent, NoteChokeEvent, NoteEndEvent, NoteExpressionEvent, NoteOffEvent,
        NoteOnEvent, ParamGestureBeginEvent, ParamGestureEndEvent, ParamModEvent, ParamValueEvent,
    },
    io::OutputEvents,
};
use clack_plugin::utils::ClapId;
use ravet_foundation::{Event, EventData, ParameterGesturePhase};
/// Event delivery is separate from successful DSP progression. A host queue can
/// accept only a prefix; there is no rollback, hidden retry or later-block replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputDelivery {
    pub delivered: usize,
    pub undelivered: usize,
    pub reason: Option<OutputRejection>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputRejection {
    /// Whole generated stream is unrepresentable (reserved ID, absent endpoint,
    /// an identity/scope unsupported by MIDI, or performance/completion in flush).
    Unrepresentable,
    /// The host refused the next event (often capacity); the suffix is discarded.
    HostRejected,
}
enum Native {
    Choke(NoteChokeEvent),
    End(NoteEndEvent),
    NoteOn(NoteOnEvent),
    NoteOff(NoteOffEvent),
    Expression(NoteExpressionEvent),
    GestureBegin(ParamGestureBeginEvent),
    GestureEnd(ParamGestureEndEvent),
    Parameter(ParamValueEvent),
    Modulation(ParamModEvent),
    Midi(MidiEvent),
    Ump(Midi2Event),
}
fn native(
    event: Event,
    port: Option<u16>,
    clap: Option<u16>,
    scoped: Option<u16>,
    completion: Option<u16>,
    flush: bool,
) -> Option<Native> {
    if let EventData::NoteEnd { target } = event.data {
        if flush {
            return None;
        }
        return Some(Native::End(NoteEndEvent::new(
            event.frame_offset as u32,
            crate::notes::pckn(completion?, target)?,
        )));
    }
    if let Some(clap) = clap
        && crate::notes::is_note(event.data)
    {
        if flush {
            return None;
        }
        return match crate::notes::encode(clap, event)? {
            crate::notes::NativeNote::Choke(note) => Some(Native::Choke(note)),
            crate::notes::NativeNote::On(note) => Some(Native::NoteOn(note)),
            crate::notes::NativeNote::Off(note) => Some(Native::NoteOff(note)),
            crate::notes::NativeNote::Expression(note) => Some(Native::Expression(note)),
        };
    }
    let time = event.frame_offset as u32; // Prepared block bounds are <=65536.
    let data = match event.data {
        EventData::ParameterValueScoped { id, target, value } => {
            if !value.is_finite() {
                return None;
            }
            return Some(Native::Parameter(ParamValueEvent::new(
                time,
                ClapId::from_raw(id.0)?,
                crate::notes::pckn(scoped?, target)?,
                value,
            )));
        }
        EventData::ParameterModulationScoped { id, target, amount } => {
            if !amount.is_finite() {
                return None;
            }
            return Some(Native::Modulation(ParamModEvent::new(
                time,
                ClapId::from_raw(id.0)?,
                crate::notes::pckn(scoped?, target)?,
                amount,
            )));
        }

        EventData::ParameterGesture { id, phase } => {
            let id = ClapId::from_raw(id.0)?;
            return Some(match phase {
                ParameterGesturePhase::Begin => {
                    Native::GestureBegin(ParamGestureBeginEvent::new(time, id))
                }
                ParameterGesturePhase::End => {
                    Native::GestureEnd(ParamGestureEndEvent::new(time, id))
                }
            });
        }
        EventData::ParameterModulation { id, amount } => {
            return Some(Native::Modulation(ParamModEvent::new(
                time,
                ClapId::from_raw(id.0)?,
                Pckn::match_all(),
                f64::from(amount),
            )));
        }
        EventData::ParameterValue { id, value } => {
            return Some(Native::Parameter(ParamValueEvent::new(
                time,
                ClapId::from_raw(id.0)?,
                Pckn::match_all(),
                f64::from(value),
            )));
        }
        // The current generated-performance profile uses MIDI, which cannot
        // preserve a portable note identity. Reject whole stream without loss.
        EventData::NoteEnd { .. }
        | EventData::NoteWithId { .. }
        | EventData::NoteExpression { .. }
        | EventData::NoteOffScoped { .. }
        | EventData::NoteChoke { .. } => return None,
        EventData::Ump(packet) => {
            let mut words = [0; 4];
            words[..packet.as_words().len()].copy_from_slice(packet.as_words());
            return Some(Native::Ump(Midi2Event::new(time, port?, words)));
        }
        EventData::Midi(message) => {
            let mut bytes = [0; 3];
            bytes[..message.as_bytes().len()].copy_from_slice(message.as_bytes());
            bytes
        }
        EventData::NoteOn {
            channel,
            key,
            velocity,
        } => [
            0x90 | channel,
            key,
            if velocity > 0. {
                (velocity * 127.).round().max(1.) as u8
            } else {
                0
            },
        ],
        EventData::NoteOff {
            channel,
            key,
            velocity,
        } => [0x80 | channel, key, (velocity * 127.).round() as u8],
    };
    Some(Native::Midi(MidiEvent::new(time, port?, data)))
}
pub(crate) fn deliver(
    events: &[Event],
    port: Option<u16>,
    clap: Option<u16>,
    scoped: Option<u16>,
    completion: Option<u16>,
    flush: bool,
    output: &mut OutputEvents<'_>,
) -> OutputDelivery {
    if events
        .iter()
        .any(|event| native(*event, port, clap, scoped, completion, flush).is_none())
    {
        return OutputDelivery {
            delivered: 0,
            undelivered: events.len(),
            reason: Some(OutputRejection::Unrepresentable),
        };
    }
    for (index, event) in events.iter().enumerate() {
        let accepted = match native(*event, port, clap, scoped, completion, flush).unwrap() {
            Native::Choke(event) => output.try_push(event),
            Native::End(event) => output.try_push(event),
            Native::NoteOn(event) => output.try_push(event),
            Native::NoteOff(event) => output.try_push(event),
            Native::Expression(event) => output.try_push(event),
            Native::GestureBegin(event) => output.try_push(event),
            Native::GestureEnd(event) => output.try_push(event),
            Native::Parameter(event) => output.try_push(event),
            Native::Modulation(event) => output.try_push(event),
            Native::Midi(event) => output.try_push(event),
            Native::Ump(event) => output.try_push(event),
        };
        if accepted.is_err() {
            return OutputDelivery {
                delivered: index,
                undelivered: events.len() - index,
                reason: Some(OutputRejection::HostRejected),
            };
        }
    }
    OutputDelivery {
        delivered: events.len(),
        undelivered: 0,
        reason: None,
    }
}
