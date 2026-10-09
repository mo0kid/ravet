//! Explicit native note conversion. No voice allocation or metadata is inferred.
use crate::BridgeError;
use clack_plugin::events::{
    Event as ClapEvent, Match, Pckn, UnknownEvent,
    event_types::{
        NoteChokeEvent, NoteExpressionEvent, NoteExpressionType, NoteOffEvent, NoteOnEvent,
    },
};
use ravet_foundation::{Event, EventData, NoteExpression, NoteId, NotePhase, NoteTarget};

pub(crate) fn validate_port(port: Option<u16>) -> Result<(), BridgeError> {
    if port.is_some_and(|port| port >= 256) {
        return Err(BridgeError::InvalidEvent);
    }
    Ok(())
}
pub(crate) fn is_note(data: EventData) -> bool {
    matches!(
        data,
        EventData::NoteOffScoped { .. }
            | EventData::NoteChoke { .. }
            | EventData::NoteOn { .. }
            | EventData::NoteOff { .. }
            | EventData::NoteWithId { .. }
            | EventData::NoteExpression { .. }
    )
}
pub(crate) fn target(channel: i16, key: i16, id: i32) -> Result<NoteTarget, BridgeError> {
    if !(-1..16).contains(&channel) || !(-1..128).contains(&key) || id < -1 {
        return Err(BridgeError::InvalidEvent);
    }
    Ok(NoteTarget {
        channel: (channel >= 0).then_some(channel as u8),
        key: (key >= 0).then_some(key as u8),
        id: (id >= 0).then_some(NoteId(id as u32)),
    })
}
fn kind(raw: i32) -> Option<NoteExpression> {
    // Pinned Clack 0.2.0 from_raw omits Expression. Preserve all standard kinds.
    Some(match raw {
        x if x == NoteExpressionType::Volume.into_raw() => NoteExpression::Volume,
        x if x == NoteExpressionType::Pan.into_raw() => NoteExpression::Pan,
        x if x == NoteExpressionType::Tuning.into_raw() => NoteExpression::Tuning,
        x if x == NoteExpressionType::Vibrato.into_raw() => NoteExpression::Vibrato,
        x if x == NoteExpressionType::Expression.into_raw() => NoteExpression::Expression,
        x if x == NoteExpressionType::Brightness.into_raw() => NoteExpression::Brightness,
        x if x == NoteExpressionType::Pressure.into_raw() => NoteExpression::Pressure,
        _ => return None,
    })
}
pub(crate) fn decode(port: u16, event: &UnknownEvent) -> Result<Option<EventData>, BridgeError> {
    if event.header().as_raw().space_id != 0 {
        return Ok(None);
    }
    let ty = event.header().type_id();
    if ty == NoteExpressionEvent::TYPE_ID {
        let raw = event
            .as_event::<NoteExpressionEvent>()
            .ok_or(BridgeError::InvalidEvent)?
            .as_raw();
        if raw.port_index != port as i16 {
            return Ok(None);
        }
        let Some(expression) = kind(raw.expression_id) else {
            return Ok(None);
        };
        let target = target(raw.channel, raw.key, raw.note_id)?;
        if !expression.is_valid_value(raw.value) {
            return Err(BridgeError::InvalidEvent);
        }
        return Ok(Some(EventData::NoteExpression {
            target,
            expression,
            value: raw.value,
        }));
    }
    if ty == NoteChokeEvent::TYPE_ID {
        let raw = event
            .as_event::<NoteChokeEvent>()
            .ok_or(BridgeError::InvalidEvent)?
            .as_raw();
        if raw.port_index != port as i16 {
            return Ok(None);
        }
        return Ok(Some(EventData::NoteChoke {
            target: target(raw.channel, raw.key, raw.note_id)?,
        }));
    }
    let (raw, phase) = if ty == NoteOnEvent::TYPE_ID {
        (
            event
                .as_event::<NoteOnEvent>()
                .ok_or(BridgeError::InvalidEvent)?
                .as_raw(),
            NotePhase::On,
        )
    } else if ty == NoteOffEvent::TYPE_ID {
        (
            event
                .as_event::<NoteOffEvent>()
                .ok_or(BridgeError::InvalidEvent)?
                .as_raw(),
            NotePhase::Off,
        )
    } else {
        return Ok(None);
    };
    if raw.port_index != port as i16 {
        return Ok(None);
    }
    let target = target(raw.channel, raw.key, raw.note_id)?;
    let velocity = raw.velocity as f32;
    if !raw.velocity.is_finite() || !(0. ..=1.).contains(&raw.velocity) || !velocity.is_finite() {
        return Err(BridgeError::InvalidEvent);
    }
    let (Some(channel), Some(key)) = (target.channel, target.key) else {
        return if phase == NotePhase::Off {
            Ok(Some(EventData::NoteOffScoped { target, velocity }))
        } else {
            Err(BridgeError::InvalidEvent)
        };
    };
    Ok(Some(if let Some(id) = target.id {
        EventData::NoteWithId {
            id,
            phase,
            channel,
            key,
            velocity,
        }
    } else {
        match phase {
            NotePhase::On => EventData::NoteOn {
                channel,
                key,
                velocity,
            },
            NotePhase::Off => EventData::NoteOff {
                channel,
                key,
                velocity,
            },
        }
    }))
}
pub(crate) fn pckn(port: u16, target: NoteTarget) -> Option<Pckn> {
    if port >= 256 || !target.is_valid() || target.id.is_some_and(|id| id.0 > i32::MAX as u32) {
        return None;
    }
    Some(Pckn::new(
        port,
        target
            .channel
            .map(|x| Match::Specific(u16::from(x)))
            .unwrap_or(Match::All),
        target
            .key
            .map(|x| Match::Specific(u16::from(x)))
            .unwrap_or(Match::All),
        target
            .id
            .map(|x| Match::Specific(x.0))
            .unwrap_or(Match::All),
    ))
}
pub(crate) enum NativeNote {
    Choke(NoteChokeEvent),
    On(NoteOnEvent),
    Off(NoteOffEvent),
    Expression(NoteExpressionEvent),
}
pub(crate) fn encode(port: u16, event: Event) -> Option<NativeNote> {
    let time = event.frame_offset as u32;
    match event.data {
        EventData::NoteChoke { target } => {
            return Some(NativeNote::Choke(NoteChokeEvent::new(
                time,
                pckn(port, target)?,
            )));
        }
        EventData::NoteOffScoped { target, velocity } => {
            if !velocity.is_finite() || !(0. ..=1.).contains(&velocity) {
                return None;
            }
            return Some(NativeNote::Off(NoteOffEvent::new(
                time,
                pckn(port, target)?,
                f64::from(velocity),
            )));
        }
        _ => {}
    }
    if let EventData::NoteExpression {
        target,
        expression,
        value,
    } = event.data
    {
        if !expression.is_valid_value(value) {
            return None;
        }
        let kind = match expression {
            NoteExpression::Volume => NoteExpressionType::Volume,
            NoteExpression::Pan => NoteExpressionType::Pan,
            NoteExpression::Tuning => NoteExpressionType::Tuning,
            NoteExpression::Vibrato => NoteExpressionType::Vibrato,
            NoteExpression::Expression => NoteExpressionType::Expression,
            NoteExpression::Brightness => NoteExpressionType::Brightness,
            NoteExpression::Pressure => NoteExpressionType::Pressure,
        };
        return Some(NativeNote::Expression(NoteExpressionEvent::new(
            time,
            pckn(port, target)?,
            kind,
            value,
        )));
    }
    let (id, phase, channel, key, velocity) = match event.data {
        EventData::NoteWithId {
            id,
            phase,
            channel,
            key,
            velocity,
        } => (Some(id), phase, channel, key, velocity),
        EventData::NoteOn {
            channel,
            key,
            velocity,
        } => (None, NotePhase::On, channel, key, velocity),
        EventData::NoteOff {
            channel,
            key,
            velocity,
        } => (None, NotePhase::Off, channel, key, velocity),
        _ => return None,
    };
    if !velocity.is_finite() || !(0. ..=1.).contains(&velocity) {
        return None;
    }
    let pckn = pckn(
        port,
        NoteTarget {
            channel: Some(channel),
            key: Some(key),
            id,
        },
    )?;
    Some(match phase {
        NotePhase::On => NativeNote::On(NoteOnEvent::new(time, pckn, f64::from(velocity))),
        NotePhase::Off => NativeNote::Off(NoteOffEvent::new(time, pckn, f64::from(velocity))),
    })
}
