//! Explicit inactive note-port selection and fixed native input encoding.
use clack_extensions::note_ports::{NoteDialect, NotePortInfoBuffer, PluginNotePorts};
use clack_host::{
    events::{
        Match, Pckn,
        event_types::{Midi2Event, MidiEvent, NoteOffEvent, NoteOnEvent},
    },
    host::HostHandlers,
    plugin::PluginInstance,
};
use ravet_foundation::{EventData, EventError, ProcessError};
/// One selected native input port index and dialect, not the metadata port ID.
/// Metadata is validated while inactive; no protocol/group translation is inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostedGraphNoteInput {
    pub port_index: u16,
    pub dialect: HostedGraphNoteDialect,
}
/// One selected native output port index/dialect, fixed before activation.
/// Default CLAP output admits concrete channel/key with wildcard note ID; the
/// builder identity option also preserves explicit IDs. No choke/end/expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostedGraphNoteOutput {
    pub port_index: u16,
    pub dialect: HostedGraphNoteDialect,
}
/// Native CLAP note input dialect. MIDI/MPE admit short MIDI and quantized
/// portable NoteOn/Off; CLAP admits wildcard-ID notes and opt-in explicit IDs; MIDI2
/// admits framed UMP only. No automatic MIDI1/MIDI2 translation or SysEx assembly.
/// MIDI velocity rounds to seven bits; quantized zero NoteOn uses MIDI's note-off
/// semantics. Performance input is rejected for zero-frame blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedGraphNoteDialect {
    Clap,
    Midi,
    MidiMpe,
    Midi2,
}
impl HostedGraphNoteDialect {
    fn native(self) -> NoteDialect {
        match self {
            Self::Clap => NoteDialect::Clap,
            Self::Midi => NoteDialect::Midi,
            Self::MidiMpe => NoteDialect::MidiMpe,
            Self::Midi2 => NoteDialect::Midi2,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedGraphNoteError {
    Unavailable,
    Capacity,
    Unknown,
    Metadata,
    Dialect,
}
pub(crate) fn prepare<H: HostHandlers>(
    instance: &mut PluginInstance<H>,
    route: HostedGraphNoteInput,
) -> Result<HostedGraphNoteInput, HostedGraphNoteError> {
    prepare_direction(instance, route.port_index, route.dialect, true)?;
    Ok(route)
}
pub(crate) fn prepare_output<H: HostHandlers>(
    instance: &mut PluginInstance<H>,
    route: HostedGraphNoteOutput,
) -> Result<HostedGraphNoteOutput, HostedGraphNoteError> {
    prepare_direction(instance, route.port_index, route.dialect, false)?;
    Ok(route)
}
fn prepare_direction<H: HostHandlers>(
    instance: &mut PluginInstance<H>,
    port_index: u16,
    dialect: HostedGraphNoteDialect,
    input: bool,
) -> Result<(), HostedGraphNoteError> {
    if port_index >= 256 {
        return Err(HostedGraphNoteError::Capacity);
    }
    let handle = instance.plugin_handle();
    let extension = handle
        .get_extension::<PluginNotePorts>()
        .ok_or(HostedGraphNoteError::Unavailable)?;
    let count = extension.count(&handle, input);
    if count > 256 {
        return Err(HostedGraphNoteError::Capacity);
    }
    if u32::from(port_index) >= count {
        return Err(HostedGraphNoteError::Unknown);
    }
    let mut buffer = NotePortInfoBuffer::new();
    let info = extension
        .get(&handle, u32::from(port_index), input, &mut buffer)
        .ok_or(HostedGraphNoteError::Metadata)?;
    if info.name.len() > 256
        || info
            .preferred_dialect
            .is_some_and(|preferred| !info.supported_dialects.supports(preferred))
    {
        return Err(HostedGraphNoteError::Metadata);
    }
    if !info.supported_dialects.supports(dialect.native()) {
        return Err(HostedGraphNoteError::Dialect);
    }
    Ok(())
}
#[derive(Clone, Copy)]
pub(crate) struct InputProfile {
    pub route: HostedGraphNoteInput,
    pub identities: bool,
    pub expressions: bool,
    pub scoped: bool,
}
pub(crate) enum NativeNote {
    Expression(clack_host::events::event_types::NoteExpressionEvent),
    Choke(clack_host::events::event_types::NoteChokeEvent),
    On(NoteOnEvent),
    Off(NoteOffEvent),
    Midi(MidiEvent),
    Ump(Midi2Event),
}
#[cfg(test)]
pub(crate) fn encode(
    route: HostedGraphNoteInput,
    data: EventData,
    time: u32,
    index: usize,
) -> Result<NativeNote, ProcessError> {
    encode_with_identity(route, data, time, index, false)
}
#[cfg(test)]
pub(crate) fn encode_with_identity(
    route: HostedGraphNoteInput,
    data: EventData,
    time: u32,
    index: usize,
    identities: bool,
) -> Result<NativeNote, ProcessError> {
    encode_with_profile(route, data, time, index, identities, false, false)
}
pub(crate) fn encode_with_profile(
    route: HostedGraphNoteInput,
    data: EventData,
    time: u32,
    index: usize,
    identities: bool,
    expressions: bool,
    scoped: bool,
) -> Result<NativeNote, ProcessError> {
    let invalid = || ProcessError::Events(EventError::InvalidValue { index });
    if matches!(
        data,
        EventData::NoteOffScoped { .. } | EventData::NoteChoke { .. }
    ) {
        if !scoped || route.dialect != HostedGraphNoteDialect::Clap {
            return Err(invalid());
        }
        let target = match data {
            EventData::NoteOffScoped { target, .. } | EventData::NoteChoke { target } => target,
            _ => unreachable!(),
        };
        let pckn = crate::graph_note_expression::encode_target(route.port_index, target, index)?;
        return Ok(match data {
            EventData::NoteOffScoped { velocity, .. }
                if velocity.is_finite() && (0. ..=1.).contains(&velocity) =>
            {
                NativeNote::Off(NoteOffEvent::new(time, pckn, f64::from(velocity)))
            }
            EventData::NoteChoke { .. } => NativeNote::Choke(
                clack_host::events::event_types::NoteChokeEvent::new(time, pckn),
            ),
            _ => return Err(invalid()),
        });
    }
    if let EventData::NoteExpression {
        target,
        expression,
        value,
    } = data
    {
        if !expressions || route.dialect != HostedGraphNoteDialect::Clap {
            return Err(invalid());
        }
        return crate::graph_note_expression::encode(
            route.port_index,
            target,
            expression,
            value,
            time,
            index,
        )
        .map(NativeNote::Expression);
    }
    if let EventData::NoteWithId {
        id,
        phase,
        channel,
        key,
        velocity,
    } = data
    {
        if !identities
            || route.dialect != HostedGraphNoteDialect::Clap
            || id.0 > i32::MAX as u32
            || channel >= 16
            || key >= 128
            || !velocity.is_finite()
            || !(0. ..=1.).contains(&velocity)
        {
            return Err(invalid());
        }
        let target = Pckn::new(route.port_index, u16::from(channel), u16::from(key), id.0);
        return Ok(match phase {
            ravet_foundation::NotePhase::On => {
                NativeNote::On(NoteOnEvent::new(time, target, f64::from(velocity)))
            }
            ravet_foundation::NotePhase::Off => {
                NativeNote::Off(NoteOffEvent::new(time, target, f64::from(velocity)))
            }
        });
    }
    match data {
        EventData::NoteOn {
            channel,
            key,
            velocity,
        }
        | EventData::NoteOff {
            channel,
            key,
            velocity,
        } => {
            if channel >= 16
                || key >= 128
                || !velocity.is_finite()
                || !(0. ..=1.).contains(&velocity)
            {
                return Err(invalid());
            }
            let on = matches!(data, EventData::NoteOn { .. });
            match route.dialect {
                HostedGraphNoteDialect::Clap => {
                    let target = Pckn::new(
                        route.port_index,
                        u16::from(channel),
                        u16::from(key),
                        Match::All,
                    );
                    Ok(if on {
                        NativeNote::On(NoteOnEvent::new(time, target, f64::from(velocity)))
                    } else {
                        NativeNote::Off(NoteOffEvent::new(time, target, f64::from(velocity)))
                    })
                }
                HostedGraphNoteDialect::Midi | HostedGraphNoteDialect::MidiMpe => {
                    Ok(NativeNote::Midi(MidiEvent::new(
                        time,
                        route.port_index,
                        [
                            if on { 0x90 } else { 0x80 } | channel,
                            key,
                            (velocity * 127.).round() as u8,
                        ],
                    )))
                }
                HostedGraphNoteDialect::Midi2 => Err(invalid()),
            }
        }
        EventData::Midi(message)
            if matches!(
                route.dialect,
                HostedGraphNoteDialect::Midi | HostedGraphNoteDialect::MidiMpe
            ) =>
        {
            let mut bytes = [0; 3];
            bytes[..message.as_bytes().len()].copy_from_slice(message.as_bytes());
            Ok(NativeNote::Midi(MidiEvent::new(
                time,
                route.port_index,
                bytes,
            )))
        }
        EventData::Ump(packet) if route.dialect == HostedGraphNoteDialect::Midi2 => {
            let mut words = [0; 4];
            words[..packet.as_words().len()].copy_from_slice(packet.as_words());
            Ok(NativeNote::Ump(Midi2Event::new(
                time,
                route.port_index,
                words,
            )))
        }
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravet_foundation::{MidiMessage, UmpPacket};
    fn route(dialect: HostedGraphNoteDialect) -> HostedGraphNoteInput {
        HostedGraphNoteInput {
            port_index: 3,
            dialect,
        }
    }
    #[test]
    fn native_notes_preserve_targets_and_midi_short_bytes_quantize_and_pad() {
        let data = EventData::NoteOn {
            channel: 2,
            key: 60,
            velocity: 0.5,
        };
        let NativeNote::On(note) =
            encode(route(HostedGraphNoteDialect::Clap), data, 14, 6).unwrap()
        else {
            panic!("native CLAP note")
        };
        assert_eq!(note.pckn(), Pckn::new(3u16, 2u16, 60u16, Match::All));
        assert_eq!(note.velocity(), 0.5);
        for dialect in [
            HostedGraphNoteDialect::Midi,
            HostedGraphNoteDialect::MidiMpe,
        ] {
            let NativeNote::Midi(event) = encode(route(dialect), data, 14, 6).unwrap() else {
                panic!("native MIDI")
            };
            assert_eq!(event.data(), [0x92, 60, 64]);
            assert_eq!(event.port_index(), 3);
            let NativeNote::Midi(zero) = encode(
                route(dialect),
                EventData::NoteOn {
                    channel: 2,
                    key: 60,
                    velocity: 0.001,
                },
                14,
                6,
            )
            .unwrap() else {
                panic!("quantized MIDI")
            };
            assert_eq!(zero.data(), [0x92, 60, 0]);
            for bytes in [&[0xC2, 7][..], &[0xF8][..]] {
                let NativeNote::Midi(event) = encode(
                    route(dialect),
                    EventData::Midi(MidiMessage::from_bytes(bytes).unwrap()),
                    14,
                    6,
                )
                .unwrap() else {
                    panic!("short MIDI")
                };
                let mut expected = [0; 3];
                expected[..bytes.len()].copy_from_slice(bytes);
                assert_eq!(event.data(), expected);
            }
        }
    }
    #[test]
    fn framed_ump_words_are_preserved_and_dialect_mismatch_rejects() {
        for words in [
            &[0x22903C7F][..],
            &[0x42903C00, 0x80000000][..],
            &[0x50000000, 1, 2, 3][..],
        ] {
            let packet = UmpPacket::from_words(words).unwrap();
            let NativeNote::Ump(event) = encode(
                route(HostedGraphNoteDialect::Midi2),
                EventData::Ump(packet),
                14,
                6,
            )
            .unwrap() else {
                panic!("native UMP")
            };
            let mut expected = [0; 4];
            expected[..words.len()].copy_from_slice(words);
            assert_eq!(event.data(), expected);
            assert_eq!(event.port_index(), 3);
            assert!(matches!(
                encode(
                    route(HostedGraphNoteDialect::Midi),
                    EventData::Ump(packet),
                    14,
                    6
                ),
                Err(ProcessError::Events(EventError::InvalidValue { index: 6 }))
            ));
        }
        assert!(
            encode(
                route(HostedGraphNoteDialect::Midi2),
                EventData::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 1.
                },
                14,
                6
            )
            .is_err()
        );
        assert!(
            encode(
                route(HostedGraphNoteDialect::Clap),
                EventData::Midi(MidiMessage::from_bytes(&[0x90, 60, 127]).unwrap()),
                14,
                6
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use ravet_foundation::{NoteId, NotePhase};
    #[test]
    fn selected_input_ids_preserve_targets_zero_on_and_native_boundary() {
        let route = HostedGraphNoteInput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Clap,
        };
        for id in [0, 123, i32::MAX as u32] {
            let data = EventData::NoteWithId {
                id: NoteId(id),
                phase: NotePhase::On,
                channel: 2,
                key: 60,
                velocity: 0.,
            };
            assert!(encode_with_identity(route, data, 2, 0, false).is_err());
            let NativeNote::On(native) = encode_with_identity(route, data, 2, 0, true).unwrap()
            else {
                panic!("explicit on");
            };
            assert_eq!(native.pckn(), Pckn::new(1u16, 2u16, 60u16, id));
            assert_eq!(native.velocity(), 0.);
        }
        for id in [i32::MAX as u32 + 1, u32::MAX] {
            let data = EventData::NoteWithId {
                id: NoteId(id),
                phase: NotePhase::Off,
                channel: 2,
                key: 60,
                velocity: 0.5,
            };
            assert!(encode_with_identity(route, data, 2, 0, true).is_err());
        }
        let data = EventData::NoteWithId {
            id: NoteId(123),
            phase: NotePhase::On,
            channel: 2,
            key: 60,
            velocity: 0.5,
        };
        for dialect in [
            HostedGraphNoteDialect::Midi,
            HostedGraphNoteDialect::MidiMpe,
            HostedGraphNoteDialect::Midi2,
        ] {
            assert!(
                encode_with_identity(HostedGraphNoteInput { dialect, ..route }, data, 2, 0, true)
                    .is_err()
            );
        }
    }
}

#[cfg(test)]
mod scoped_tests {
    use super::*;
    use ravet_foundation::{NoteId, NoteTarget};
    #[test]
    fn scoped_input_requires_clap_opt_in_and_valid_native_id_before_encoding() {
        let route = HostedGraphNoteInput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Clap,
        };
        for data in [
            EventData::NoteOffScoped {
                target: NoteTarget::default(),
                velocity: 0.5,
            },
            EventData::NoteChoke {
                target: NoteTarget::default(),
            },
        ] {
            assert!(encode_with_profile(route, data, 2, 0, false, false, false).is_err());
            assert!(encode_with_profile(route, data, 2, 0, false, false, true).is_ok());
            assert!(
                encode_with_profile(
                    HostedGraphNoteInput {
                        dialect: HostedGraphNoteDialect::Midi,
                        ..route
                    },
                    data,
                    2,
                    0,
                    false,
                    false,
                    true
                )
                .is_err()
            );
        }
        for data in [
            EventData::NoteOffScoped {
                target: NoteTarget {
                    id: Some(NoteId(u32::MAX)),
                    ..NoteTarget::default()
                },
                velocity: 0.5,
            },
            EventData::NoteChoke {
                target: NoteTarget {
                    id: Some(NoteId(u32::MAX)),
                    ..NoteTarget::default()
                },
            },
            EventData::NoteOffScoped {
                target: NoteTarget::default(),
                velocity: f32::NAN,
            },
        ] {
            assert!(encode_with_profile(route, data, 2, 0, false, false, true).is_err());
        }
    }
}
