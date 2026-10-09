use ravet_foundation::{UmpDecoder, UmpError, UmpPacket};

#[test]
fn every_message_type_preserves_framing_and_opaque_payload() {
    let lengths = [1, 1, 1, 2, 2, 4, 1, 1, 2, 2, 2, 3, 3, 4, 4, 4];
    let mut decoder = UmpDecoder::new();
    for (kind, count) in lengths.into_iter().enumerate() {
        let words = [
            (kind as u32) << 28 | 0x0fabcdef,
            0x12345678,
            0x87654321,
            u32::MAX,
        ];
        let packet = UmpPacket::from_words(&words[..count]).unwrap();
        assert_eq!(packet.message_type(), kind as u8);
        assert_eq!(
            packet.group(),
            if matches!(kind, 1..=5 | 13) {
                Some(15)
            } else {
                None
            }
        );
        for (index, word) in words[..count].iter().enumerate() {
            let result = decoder.push(*word);
            assert_eq!(
                result,
                if index + 1 == count {
                    Some(packet)
                } else {
                    None
                }
            );
            if index + 1 < count {
                assert_eq!(decoder.finish(), Err(UmpError::Truncated));
            }
        }
        decoder.finish().unwrap();
        for wrong in 0..=4 {
            if wrong != count {
                assert_eq!(
                    UmpPacket::from_words(&words[..wrong]),
                    Err(UmpError::Length)
                );
            }
        }
    }
    assert_eq!(decoder.push(0x50000000), None);
    decoder.reset();
    decoder.finish().unwrap();
    assert_eq!(decoder.push(0), Some(UmpPacket::from_words(&[0]).unwrap()));
}

#[test]
fn explicit_byte_order_and_atomic_destination_validation() {
    // MIDI 2 note on, group 1, channel 2, key 60, velocity 0xabcd.
    let expected = [0x41, 0x92, 0x3c, 0, 0xab, 0xcd, 0, 0];
    let packet = UmpPacket::from_words(&[0x41923c00, 0xabcd0000]).unwrap();
    assert_eq!(UmpPacket::from_be_bytes(&expected).unwrap(), packet);
    let mut bytes = [0xee; 20];
    assert_eq!(packet.write_be_bytes(&mut bytes).unwrap(), 8);
    assert_eq!(&bytes[..8], &expected);
    assert_eq!(&bytes[8..], &[0xee; 12]);
    for length in 0..8 {
        let mut output = [0xee; 8];
        assert_eq!(
            packet.write_be_bytes(&mut output[..length]),
            Err(UmpError::Capacity)
        );
        assert_eq!(output, [0xee; 8]);
        assert_eq!(
            UmpPacket::from_be_bytes(&expected[..length]),
            Err(UmpError::Length)
        );
    }
    assert_eq!(UmpPacket::from_be_bytes(&[0; 20]), Err(UmpError::Length));
    for _ in 0..10_000 {
        let mut decoder = UmpDecoder::new();
        assert_eq!(decoder.push(0x41923c00), None);
        assert_eq!(decoder.push(0xabcd0000), Some(packet));
    }
}

#[test]
fn midi1_short_messages_keep_status_data_and_reject_reserved_system_codes() {
    use ravet_foundation::{MidiMessage, UmpMidi1};
    for group in 0..16 {
        for status in 0x80..=0xffu8 {
            let length = match status {
                0x80..=0xbf | 0xe0..=0xef | 0xf2 => 3,
                0xc0..=0xdf | 0xf1 | 0xf3 => 2,
                0xf6 | 0xf8 | 0xfa..=0xfc | 0xfe..=0xff => 1,
                _ => continue,
            };
            let bytes = [status, 127, 42];
            let message = MidiMessage::from_bytes(&bytes[..length]).unwrap();
            let ump = UmpMidi1::new(group, message).unwrap();
            assert_eq!(UmpMidi1::from_packet(ump.packet()).unwrap(), ump);
            assert_eq!(ump.group(), group);
            assert_eq!(ump.message().as_bytes(), &bytes[..length]);
        }
    }
    let canonical = UmpMidi1::from_packet(UmpPacket::from_words(&[0x1ff8ffff]).unwrap()).unwrap();
    assert_eq!(canonical.packet().as_words(), [0x1ff80000]);
    let note = UmpMidi1::from_packet(UmpPacket::from_words(&[0x21903cff]).unwrap()).unwrap();
    assert_eq!(note.message().as_bytes(), &[0x90, 60, 127]);
    assert!(UmpMidi1::new(16, note.message()).is_err());
    assert!(UmpMidi1::new(0, MidiMessage::from_bytes(&[0xf7]).unwrap()).is_err());
    for first in [
        0x10f00000, 0x10f70000, 0x10f90000, 0x10900000, 0x20f80000, 0,
    ] {
        assert!(UmpMidi1::from_packet(UmpPacket::from_words(&[first]).unwrap()).is_err());
    }
}
