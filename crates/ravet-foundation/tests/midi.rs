use ravet_foundation::{
    Event, EventData, MidiDecoder, MidiError, MidiKind, MidiMessage, OutputEvents,
};

#[test]
fn every_channel_message_round_trips_and_decodes_full_resolution() {
    for channel in 0..16 {
        for high in 0x8..=0xe {
            let bytes = [high << 4 | channel, 127, 127];
            let len = if high == 0xc || high == 0xd { 2 } else { 3 };
            let message = MidiMessage::from_bytes(&bytes[..len]).unwrap();
            assert_eq!(message.as_bytes(), &bytes[..len]);
        }
        assert_eq!(
            MidiMessage::from_bytes(&[0xe0 | channel, 0, 64])
                .unwrap()
                .kind(),
            MidiKind::PitchBend {
                channel,
                value: 8192
            }
        );
        assert_eq!(
            MidiMessage::from_bytes(&[0xe0 | channel, 127, 127])
                .unwrap()
                .kind(),
            MidiKind::PitchBend {
                channel,
                value: 16383
            }
        );
        let off = MidiMessage::from_bytes(&[0x90 | channel, 60, 0]).unwrap();
        assert_eq!(
            off.kind(),
            MidiKind::NoteOff {
                channel,
                key: 60,
                velocity: 0
            }
        );
        assert_eq!(off.as_bytes(), &[0x90 | channel, 60, 0]);
    }
    assert_eq!(
        MidiMessage::from_bytes(&[0xf2, 127, 127]).unwrap().kind(),
        MidiKind::SongPosition(16383)
    );
}
#[test]
fn running_status_survives_interleaved_realtime_and_common_messages_cancel_it() {
    let mut decoder = MidiDecoder::default();
    assert_eq!(decoder.push(0x92), Ok(None));
    assert_eq!(decoder.push(60), Ok(None));
    assert_eq!(
        decoder.push(0xf8).unwrap().unwrap().kind(),
        MidiKind::TimingClock
    );
    assert_eq!(
        decoder.push(100).unwrap().unwrap().as_bytes(),
        &[0x92, 60, 100]
    );
    assert_eq!(decoder.push(61), Ok(None));
    assert_eq!(decoder.push(0xf9), Err(MidiError::InvalidStatus));
    assert_eq!(decoder.push(0xfa).unwrap().unwrap().kind(), MidiKind::Start);
    assert_eq!(
        decoder.push(0).unwrap().unwrap().kind(),
        MidiKind::NoteOff {
            channel: 2,
            key: 61,
            velocity: 0
        }
    );
    assert_eq!(decoder.push(0xf1), Ok(None));
    assert_eq!(
        decoder.push(0x70).unwrap().unwrap().kind(),
        MidiKind::QuarterFrame(0x70)
    );
    assert_eq!(decoder.push(62), Err(MidiError::MissingStatus));
    assert_eq!(decoder.push(0xc3), Ok(None));
    assert_eq!(decoder.push(5).unwrap().unwrap().as_bytes(), &[0xc3, 5]);
    assert_eq!(decoder.push(6).unwrap().unwrap().as_bytes(), &[0xc3, 6]);
    decoder.reset();
    assert_eq!(decoder.push(7), Err(MidiError::MissingStatus));
}
#[test]
fn sysex_skip_and_interrupted_status_recover_without_corrupting_messages() {
    let mut decoder = MidiDecoder::default();
    decoder.push(0x90).unwrap();
    decoder.push(60).unwrap();
    decoder.push(0xd2).unwrap();
    assert_eq!(
        decoder.push(100).unwrap().unwrap().kind(),
        MidiKind::ChannelPressure {
            channel: 2,
            pressure: 100
        }
    );
    assert_eq!(decoder.push(0xf0), Err(MidiError::UnsupportedSysEx));
    assert_eq!(decoder.push(1), Ok(None));
    assert_eq!(
        decoder.push(0xfe).unwrap().unwrap().kind(),
        MidiKind::ActiveSensing
    );
    assert_eq!(decoder.push(2), Ok(None));
    assert_eq!(
        decoder.push(0xf7).unwrap().unwrap().kind(),
        MidiKind::EndOfExclusive
    );
    assert_eq!(decoder.push(60), Err(MidiError::MissingStatus));
    decoder.push(0xf0).unwrap_err();
    decoder.push(1).unwrap();
    decoder.push(0xb0).unwrap();
    decoder.push(64).unwrap();
    assert_eq!(
        decoder.push(127).unwrap().unwrap().kind(),
        MidiKind::ControlChange {
            channel: 0,
            controller: 64,
            value: 127
        }
    );
}
#[test]
fn invalid_wire_messages_reject_and_timed_output_preserves_bytes() {
    for bytes in [
        &[][..],
        &[0x90, 60],
        &[0x90, 60, 1, 2],
        &[60, 1, 2],
        &[0x90, 128, 1],
        &[0xf4],
        &[0xfd],
        &[0xf0, 1, 0xf7],
    ] {
        assert!(MidiMessage::from_bytes(bytes).is_err(), "{bytes:?}");
    }
    let message = MidiMessage::from_bytes(&[0xb4, 74, 90]).unwrap();
    let event = Event {
        frame_offset: 3,
        data: EventData::Midi(message),
    };
    let mut storage = [event; 1];
    let mut output = OutputEvents::new(&mut storage, 4);
    output.push(event).unwrap();
    assert_eq!(output.as_slice(), &[event]);
}
