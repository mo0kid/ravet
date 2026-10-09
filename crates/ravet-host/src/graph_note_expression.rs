//! Fixed selected-port expression conversion; scope and receiver policy are explicit.
use clack_host::events::{
    Match, Pckn,
    event_types::{NoteExpressionEvent, NoteExpressionType},
};
use ravet_foundation::{EventData, EventError, NoteExpression, NoteId, NoteTarget, ProcessError};
pub(crate) fn encode(
    port: u16,
    target: NoteTarget,
    expression: NoteExpression,
    value: f64,
    time: u32,
    index: usize,
) -> Result<NoteExpressionEvent, ProcessError> {
    if !expression.is_valid_value(value) {
        return Err(ProcessError::Events(EventError::InvalidValue { index }));
    }
    let target = encode_target(port, target, index)?;
    let kind = match expression {
        NoteExpression::Volume => NoteExpressionType::Volume,
        NoteExpression::Pan => NoteExpressionType::Pan,
        NoteExpression::Tuning => NoteExpressionType::Tuning,
        NoteExpression::Vibrato => NoteExpressionType::Vibrato,
        NoteExpression::Expression => NoteExpressionType::Expression,
        NoteExpression::Brightness => NoteExpressionType::Brightness,
        NoteExpression::Pressure => NoteExpressionType::Pressure,
    };
    Ok(NoteExpressionEvent::new(time, target, kind, value))
}
pub(crate) fn encode_target(
    port: u16,
    target: NoteTarget,
    index: usize,
) -> Result<Pckn, ProcessError> {
    if port >= 256 || !target.is_valid() || target.id.is_some_and(|id| id.0 > i32::MAX as u32) {
        return Err(ProcessError::Events(EventError::InvalidValue { index }));
    }
    Ok(Pckn::new(
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
            .map(|id| Match::Specific(id.0))
            .unwrap_or(Match::All),
    ))
}
pub(crate) fn decode_target(
    port: u16,
    raw_port: i16,
    channel: i16,
    key: i16,
    id: i32,
) -> Result<Option<NoteTarget>, ()> {
    if raw_port != port as i16 {
        return Ok(None);
    }
    if !(-1..16).contains(&channel) || !(-1..128).contains(&key) || id < -1 {
        return Err(());
    }
    Ok(Some(NoteTarget {
        channel: (channel >= 0).then_some(channel as u8),
        key: (key >= 0).then_some(key as u8),
        id: (id >= 0).then_some(NoteId(id as u32)),
    }))
}
pub(crate) fn decode(port: u16, event: &NoteExpressionEvent) -> Result<Option<EventData>, ()> {
    let raw = event.as_raw();
    if raw.port_index != port as i16 {
        return Ok(None);
    }
    // Clack 0.2.0 from_raw omits the standard Expression discriminator. Match
    // the complete public wire enum directly rather than lose that valid kind.
    let expression = match raw.expression_id {
        kind if kind == NoteExpressionType::Volume.into_raw() => NoteExpression::Volume,
        kind if kind == NoteExpressionType::Pan.into_raw() => NoteExpression::Pan,
        kind if kind == NoteExpressionType::Tuning.into_raw() => NoteExpression::Tuning,
        kind if kind == NoteExpressionType::Vibrato.into_raw() => NoteExpression::Vibrato,
        kind if kind == NoteExpressionType::Expression.into_raw() => NoteExpression::Expression,
        kind if kind == NoteExpressionType::Brightness.into_raw() => NoteExpression::Brightness,
        kind if kind == NoteExpressionType::Pressure.into_raw() => NoteExpression::Pressure,
        _ => return Ok(None),
    };
    if !expression.is_valid_value(raw.value) {
        return Err(());
    }
    let Some(target) = decode_target(port, raw.port_index, raw.channel, raw.key, raw.note_id)?
    else {
        return Ok(None);
    };
    Ok(Some(EventData::NoteExpression {
        target,
        expression,
        value: raw.value,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_standard_types_scopes_ranges_and_f64_values_roundtrip_without_heap() {
        let kinds = [
            (NoteExpression::Volume, 0.12345678912345678),
            (NoteExpression::Pan, 0.12345678912345678),
            (NoteExpression::Tuning, -119.12345678912345),
            (NoteExpression::Vibrato, 0.12345678912345678),
            (NoteExpression::Expression, 0.12345678912345678),
            (NoteExpression::Brightness, 0.12345678912345678),
            (NoteExpression::Pressure, 0.12345678912345678),
        ];
        let scopes = [
            NoteTarget::default(),
            NoteTarget {
                channel: Some(15),
                key: None,
                id: None,
            },
            NoteTarget {
                channel: None,
                key: Some(127),
                id: Some(NoteId(i32::MAX as u32)),
            },
        ];
        let counts = allocation_counter::measure(|| {
            for _ in 0..1000 {
                for (expression, value) in kinds {
                    for target in scopes {
                        let native = encode(1, target, expression, value, 2, 0).unwrap();
                        assert_eq!(
                            decode(1, &native),
                            Ok(Some(EventData::NoteExpression {
                                target,
                                expression,
                                value
                            }))
                        );
                        assert_eq!(native.value().to_bits(), value.to_bits());
                    }
                }
            }
        });
        assert_eq!(counts.count_total, 0);
    }
    #[test]
    fn malformed_selected_metadata_rejects_and_unselected_or_unknown_filters() {
        let good = encode(
            1,
            NoteTarget::default(),
            NoteExpression::Pressure,
            0.5,
            2,
            0,
        )
        .unwrap();
        let mut invalid = [*good.as_raw(); 7];
        invalid[0].note_id = -2;
        invalid[1].channel = -2;
        invalid[2].channel = 16;
        invalid[3].key = -2;
        invalid[4].key = 128;
        invalid[5].value = f64::NAN;
        invalid[6].value = 1.01;
        for raw in invalid {
            let native = NoteExpressionEvent::from_raw(&raw);
            assert_eq!(decode(1, &native), Err(()));
        }
        let mut raw = invalid[5];
        raw.port_index = 0;
        let other = NoteExpressionEvent::from_raw(&raw);
        assert_eq!(decode(1, &other), Ok(None));
        raw.port_index = -1;
        let all_ports = NoteExpressionEvent::from_raw(&raw);
        assert_eq!(decode(1, &all_ports), Ok(None));
        raw.port_index = 1;
        raw.expression_id = 99;
        let unknown = NoteExpressionEvent::from_raw(&raw);
        assert_eq!(decode(1, &unknown), Ok(None));
        for target in [
            NoteTarget {
                channel: Some(16),
                ..NoteTarget::default()
            },
            NoteTarget {
                id: Some(NoteId(u32::MAX)),
                ..NoteTarget::default()
            },
        ] {
            assert!(encode(1, target, NoteExpression::Pressure, 0.5, 2, 0).is_err());
        }
        assert!(
            encode(
                256,
                NoteTarget::default(),
                NoteExpression::Pressure,
                0.5,
                2,
                0
            )
            .is_err()
        );
        assert!(encode(1, NoteTarget::default(), NoteExpression::Volume, 0., 2, 0).is_err());
    }
}
