//! Selected performance output decoding; unsupported targets filter explicitly.
use crate::{HostedGraphNoteDialect, HostedGraphNoteOutput};
use clack_host::events::{
    Match, UnknownEvent,
    event_types::{Midi2Event, MidiEvent, NoteOffEvent, NoteOnEvent},
};
use ravet_foundation::{EventData, MidiMessage, UmpPacket};

#[cfg(test)]
pub(crate) fn decode(
    route: HostedGraphNoteOutput,
    event: &UnknownEvent,
) -> Result<Option<EventData>, ()> {
    decode_with_identity(route, event, false)
}
#[derive(Clone, Copy)]
pub(crate) struct OutputProfile {
    pub route: HostedGraphNoteOutput,
    pub identities: bool,
    pub expressions: bool,
    pub scoped: bool,
}
#[cfg(test)]
pub(crate) fn decode_with_identity(
    route: HostedGraphNoteOutput,
    event: &UnknownEvent,
    identities: bool,
) -> Result<Option<EventData>, ()> {
    decode_with_profile(route, event, identities, false, false)
}
pub(crate) fn decode_with_profile(
    route: HostedGraphNoteOutput,
    event: &UnknownEvent,
    identities: bool,
    expressions: bool,
    scoped: bool,
) -> Result<Option<EventData>, ()> {
    if route.dialect == HostedGraphNoteDialect::Clap {
        if expressions
            && let Some(expression) =
                event.as_event::<clack_host::events::event_types::NoteExpressionEvent>()
        {
            return crate::graph_note_expression::decode(route.port_index, expression);
        }
        if scoped {
            if let Some(choke) = event.as_event::<clack_host::events::event_types::NoteChokeEvent>()
            {
                return crate::graph_note_expression::decode_target(
                    route.port_index,
                    choke.as_raw().port_index,
                    choke.as_raw().channel,
                    choke.as_raw().key,
                    choke.as_raw().note_id,
                )
                .map(|target| target.map(|target| EventData::NoteChoke { target }));
            }
            if let Some(off) = event.as_event::<NoteOffEvent>() {
                let raw = off.as_raw();
                let Some(target) = crate::graph_note_expression::decode_target(
                    route.port_index,
                    raw.port_index,
                    raw.channel,
                    raw.key,
                    raw.note_id,
                )?
                else {
                    return Ok(None);
                };
                if raw.channel < 0 || raw.key < 0 {
                    if !raw.velocity.is_finite() || !(0. ..=1.).contains(&raw.velocity) {
                        return Err(());
                    }
                    return Ok(Some(EventData::NoteOffScoped {
                        target,
                        velocity: raw.velocity as f32,
                    }));
                }
            }
        }
        let (target, value, on, raw_id) = if let Some(note) = event.as_event::<NoteOnEvent>() {
            (note.pckn(), note.velocity(), true, note.as_raw().note_id)
        } else if let Some(note) = event.as_event::<NoteOffEvent>() {
            (note.pckn(), note.velocity(), false, note.as_raw().note_id)
        } else {
            return Ok(None);
        };
        if target.port_index != Match::Specific(route.port_index)
            || (!identities && target.note_id != Match::All)
        {
            return Ok(None);
        }
        // Clack normalizes every negative ID to All; only -1 is the native
        // wildcard. Explicit identity profiles reject malformed negative IDs.
        if identities && raw_id < -1 {
            return Err(());
        }
        let (Match::Specific(channel), Match::Specific(key)) = (target.channel, target.key) else {
            return Ok(None);
        };
        if channel >= 16 || key >= 128 || !value.is_finite() || !(0. ..=1.).contains(&value) {
            return Err(());
        }
        let velocity = value as f32;
        if let Match::Specific(id) = target.note_id {
            if id > i32::MAX as u32 {
                return Err(());
            }
            return Ok(Some(EventData::NoteWithId {
                id: ravet_foundation::NoteId(id),
                phase: if on {
                    ravet_foundation::NotePhase::On
                } else {
                    ravet_foundation::NotePhase::Off
                },
                channel: channel as u8,
                key: key as u8,
                velocity,
            }));
        }
        Ok(Some(if on {
            EventData::NoteOn {
                channel: channel as u8,
                key: key as u8,
                velocity,
            }
        } else {
            EventData::NoteOff {
                channel: channel as u8,
                key: key as u8,
                velocity,
            }
        }))
    } else if matches!(
        route.dialect,
        HostedGraphNoteDialect::Midi | HostedGraphNoteDialect::MidiMpe
    ) {
        let Some(message) = event.as_event::<MidiEvent>() else {
            return Ok(None);
        };
        if message.port_index() != route.port_index {
            return Ok(None);
        }
        let bytes = message.data();
        let length = match bytes[0] {
            0x80..=0xbf | 0xe0..=0xef | 0xf2 => 3,
            0xc0..=0xdf | 0xf1 | 0xf3 => 2,
            0xf6..=0xf8 | 0xfa..=0xfc | 0xfe..=0xff => 1,
            _ => return Err(()),
        };
        MidiMessage::from_bytes(&bytes[..length])
            .map(|message| Some(EventData::Midi(message)))
            .map_err(|_| ())
    } else {
        let Some(message) = event.as_event::<Midi2Event>() else {
            return Ok(None);
        };
        if message.port_index() != route.port_index {
            return Ok(None);
        }
        let words = message.data();
        UmpPacket::from_words(&words[..UmpPacket::word_count(words[0])])
            .map(|packet| Some(EventData::Ump(packet)))
            .map_err(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clack_host::events::Pckn;
    fn route(dialect: HostedGraphNoteDialect) -> HostedGraphNoteOutput {
        HostedGraphNoteOutput {
            port_index: 1,
            dialect,
        }
    }
    #[test]
    fn concrete_wildcard_id_notes_narrow_while_unrepresentable_targets_filter() {
        let target = Pckn::new(1u16, 2u16, 60u16, Match::All);
        let selected = route(HostedGraphNoteDialect::Clap);
        let event = NoteOnEvent::new(12, target, 0.1);
        assert_eq!(
            decode(selected, event.as_ref()).unwrap(),
            Some(EventData::NoteOn {
                channel: 2,
                key: 60,
                velocity: 0.1,
            })
        );
        let off = NoteOffEvent::new(13, target, 0.);
        assert_eq!(
            decode(selected, off.as_ref()).unwrap(),
            Some(EventData::NoteOff {
                channel: 2,
                key: 60,
                velocity: 0.,
            })
        );
        for target in [
            Pckn::new(0u16, 2u16, 60u16, Match::All),
            Pckn::new(1u16, Match::All, 60u16, Match::All),
            Pckn::new(1u16, 2u16, 60u16, 7u32),
        ] {
            assert_eq!(
                decode(selected, NoteOnEvent::new(12, target, 0.5).as_ref()),
                Ok(None)
            );
        }
        for value in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
            assert_eq!(
                decode(selected, NoteOnEvent::new(12, target, value).as_ref()),
                Err(())
            );
        }
        assert_eq!(
            decode(
                selected,
                NoteOnEvent::new(12, Pckn::new(1u16, 16u16, 60u16, Match::All), 0.5).as_ref()
            ),
            Err(())
        );
    }
    #[test]
    fn midi_preserves_wire_length_and_ump_preserves_framed_words() {
        for dialect in [
            HostedGraphNoteDialect::Midi,
            HostedGraphNoteDialect::MidiMpe,
        ] {
            for (bytes, expected) in [
                ([0x92, 60, 0], &[0x92, 60, 0][..]),
                ([0xC2, 7, 200], &[0xC2, 7][..]),
                ([0xF8, 200, 200], &[0xF8][..]),
            ] {
                let Some(EventData::Midi(message)) =
                    decode(route(dialect), MidiEvent::new(12, 1, bytes).as_ref()).unwrap()
                else {
                    panic!("short MIDI")
                };
                assert_eq!(message.as_bytes(), expected);
            }
            for bytes in [[0x92, 200, 64], [0xF0, 0, 0], [0xF9, 0, 0], [0, 0, 0]] {
                assert_eq!(
                    decode(route(dialect), MidiEvent::new(12, 1, bytes).as_ref()),
                    Err(())
                );
            }
            assert_eq!(
                decode(
                    route(dialect),
                    MidiEvent::new(12, 0, [0x92, 200, 64]).as_ref()
                ),
                Ok(None)
            );
        }
        for words in [
            [0x22903C7F, 9, 9, 9],
            [0x42903C00, 0x80000000, 9, 9],
            [0xB1234567, 1, 2, 9],
            [0x51234567, 1, 2, 3],
        ] {
            let Some(EventData::Ump(packet)) = decode(
                route(HostedGraphNoteDialect::Midi2),
                Midi2Event::new(12, 1, words).as_ref(),
            )
            .unwrap() else {
                panic!("UMP")
            };
            assert_eq!(packet.as_words(), &words[..UmpPacket::word_count(words[0])]);
        }
        assert_eq!(
            decode(
                route(HostedGraphNoteDialect::Midi2),
                MidiEvent::new(12, 1, [0x92, 60, 64]).as_ref()
            ),
            Ok(None)
        );
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use clack_host::events::Pckn;
    use ravet_foundation::{NoteId, NotePhase};
    #[test]
    fn selected_ids_preserve_full_native_range_and_reject_malformed_negative_values() {
        let route = HostedGraphNoteOutput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Clap,
        };
        for id in [0, 123, i32::MAX as u32] {
            let native = NoteOnEvent::new(2, Pckn::new(1u16, 2u16, 60u16, id), 0.);
            assert_eq!(
                decode_with_identity(route, native.as_ref(), false),
                Ok(None)
            );
            assert_eq!(
                decode_with_identity(route, native.as_ref(), true),
                Ok(Some(EventData::NoteWithId {
                    id: NoteId(id),
                    phase: NotePhase::On,
                    channel: 2,
                    key: 60,
                    velocity: 0.,
                }))
            );
        }
        let native = NoteOnEvent::new(2, Pckn::new(1u16, 2u16, 60u16, Match::All), 0.5);
        let mut raw = *native.as_raw();
        raw.note_id = -2;
        let bad = NoteOnEvent::from_raw(&raw);
        assert_eq!(decode_with_identity(route, bad.as_ref(), true), Err(()));
        raw.port_index = 0;
        let other_port = NoteOnEvent::from_raw(&raw);
        assert_eq!(
            decode_with_identity(route, other_port.as_ref(), true),
            Ok(None)
        );
    }
}

#[cfg(test)]
mod scoped_tests {
    use super::*;
    use clack_host::events::{Pckn, event_types::NoteChokeEvent};
    use ravet_foundation::{NoteId, NoteTarget};
    #[test]
    fn wildcard_off_choke_native_scope_and_default_filtering_are_explicit() {
        let route = HostedGraphNoteOutput {
            port_index: 1,
            dialect: HostedGraphNoteDialect::Clap,
        };
        let off = NoteOffEvent::new(
            2,
            Pckn::new(1u16, Match::All, 60u16, i32::MAX as u32),
            0.12345678912345678,
        );
        assert_eq!(
            decode_with_profile(route, off.as_ref(), true, false, false),
            Ok(None)
        );
        assert_eq!(
            decode_with_profile(route, off.as_ref(), false, false, true),
            Ok(Some(EventData::NoteOffScoped {
                target: NoteTarget {
                    channel: None,
                    key: Some(60),
                    id: Some(NoteId(i32::MAX as u32))
                },
                velocity: 0.12345678912345678_f64 as f32
            }))
        );
        let good = NoteChokeEvent::new(2, Pckn::new(1u16, Match::All, Match::All, Match::All));
        let mut raw = *good.as_raw();
        raw.velocity = f64::NAN;
        let choke = NoteChokeEvent::from_raw(&raw);
        assert_eq!(
            decode_with_profile(route, choke.as_ref(), true, false, false),
            Ok(None)
        );
        assert_eq!(
            decode_with_profile(route, choke.as_ref(), false, false, true),
            Ok(Some(EventData::NoteChoke {
                target: NoteTarget::default()
            }))
        );
        let mut invalid = [raw; 5];
        invalid[0].channel = -2;
        invalid[1].key = -2;
        invalid[2].channel = 16;
        invalid[3].key = 128;
        invalid[4].note_id = -2;
        for mut raw in invalid {
            let bad = NoteChokeEvent::from_raw(&raw);
            assert_eq!(
                decode_with_profile(route, bad.as_ref(), false, false, true),
                Err(())
            );
            raw.port_index = 0;
            let other = NoteChokeEvent::from_raw(&raw);
            assert_eq!(
                decode_with_profile(route, other.as_ref(), false, false, true),
                Ok(None)
            );
        }
        let mut raw = *off.as_raw();
        raw.channel = 2;
        raw.note_id = -2;
        let bad = NoteOffEvent::from_raw(&raw);
        assert_eq!(
            decode_with_profile(route, bad.as_ref(), false, false, true),
            Err(())
        );
    }
}
