use ravet_foundation::{Midi2Kind as K, Midi2Message as M, UmpPacket};

#[test]
fn independently_specified_words_cover_every_defined_status() {
    let fixtures = [
        (
            0x4f023cff,
            0x12345678,
            K::PerNoteController {
                registered: true,
                key: 60,
                index: 255,
                value: 0x12345678,
            },
        ),
        (
            0x4f123c80,
            u32::MAX,
            K::PerNoteController {
                registered: false,
                key: 60,
                index: 128,
                value: u32::MAX,
            },
        ),
        (
            0x4f22017f,
            0x12345678,
            K::Controller {
                registered: true,
                bank: 1,
                index: 127,
                value: 0x12345678,
            },
        ),
        (
            0x4f32017f,
            0,
            K::Controller {
                registered: false,
                bank: 1,
                index: 127,
                value: 0,
            },
        ),
        (
            0x4f42017f,
            0x80000000,
            K::RelativeController {
                registered: true,
                bank: 1,
                index: 127,
                delta: i32::MIN,
            },
        ),
        (
            0x4f52017f,
            u32::MAX,
            K::RelativeController {
                registered: false,
                bank: 1,
                index: 127,
                delta: -1,
            },
        ),
        (
            0x4f623c00,
            0x80000000,
            K::PerNotePitchBend {
                key: 60,
                value: 0x80000000,
            },
        ),
        (
            0x4f823c03,
            0xabcd1234,
            K::NoteOff {
                key: 60,
                velocity: 0xabcd,
                attribute_type: 3,
                attribute: 0x1234,
            },
        ),
        (
            0x4f923c01,
            0x00001234,
            K::NoteOn {
                key: 60,
                velocity: 0,
                attribute_type: 1,
                attribute: 0x1234,
            },
        ),
        (
            0x4fa23c00,
            u32::MAX,
            K::PolyPressure {
                key: 60,
                value: u32::MAX,
            },
        ),
        (
            0x4fb27f00,
            0x12345678,
            K::ControlChange {
                controller: 127,
                value: 0x12345678,
            },
        ),
        (
            0x4fc20001,
            0x7f00017f,
            K::ProgramChange {
                program: 127,
                bank: Some((1, 127)),
            },
        ),
        (
            0x4fc20000,
            0x01000000,
            K::ProgramChange {
                program: 1,
                bank: None,
            },
        ),
        (
            0x4fd20000,
            0x12345678,
            K::ChannelPressure { value: 0x12345678 },
        ),
        (0x4fe20000, 0x80000000, K::PitchBend { value: 0x80000000 }),
        (
            0x4ff23c03,
            0,
            K::PerNoteManagement {
                key: 60,
                detach: true,
                reset: true,
            },
        ),
    ];
    for (first, second, kind) in fixtures {
        let packet = UmpPacket::from_words(&[first, second]).unwrap();
        let message = M::from_packet(packet).unwrap();
        assert_eq!(message.group(), 15);
        assert_eq!(message.channel(), 2);
        assert_eq!(message.kind(), kind);
        assert_eq!(message.packet(), packet);
        assert_eq!(M::new(15, 2, kind).unwrap(), message);
        for group in 0..16 {
            for channel in 0..16 {
                let message = M::new(group, channel, kind).unwrap();
                assert_eq!(M::from_packet(message.packet()).unwrap(), message);
            }
        }
    }
}

#[test]
fn reserved_bits_are_ignored_without_truncating_defined_fields() {
    let fixtures = [
        ([0x40a0bcff, u32::MAX], [0x40a03c00, u32::MAX]),
        ([0x40c0ffff, 0xffffffff], [0x40c00001, 0x7f007f7f]),
        ([0x40f0bcff, u32::MAX], [0x40f03c03, 0]),
        ([0x40d0ffff, u32::MAX], [0x40d00000, u32::MAX]),
        ([0x40903c00, 0x0000ffff], [0x40903c00, 0]),
        // Future attribute types are retained rather than interpreted.
        ([0x40903cff, 0xabcdef01], [0x40903cff, 0xabcdef01]),
        // Bank fields have no effect when Bank Valid is clear.
        ([0x40c00000, 0x0100ffff], [0x40c00000, 0x01000000]),
    ];
    for (input, expected) in fixtures {
        let message = M::from_packet(UmpPacket::from_words(&input).unwrap()).unwrap();
        assert_eq!(message.packet().as_words(), expected);
    }
    assert!(M::from_packet(UmpPacket::from_words(&[0x40700000, 0]).unwrap()).is_err());
    assert!(M::from_packet(UmpPacket::from_words(&[0x20903c7f]).unwrap()).is_err());
    let kind = K::PitchBend { value: 0x80000000 };
    assert!(M::new(16, 0, kind).is_err());
    assert!(M::new(0, 16, kind).is_err());
    for kind in [
        K::NoteOn {
            key: 128,
            velocity: 0,
            attribute_type: 0,
            attribute: 0,
        },
        K::Controller {
            registered: true,
            bank: 128,
            index: 0,
            value: 0,
        },
        K::RelativeController {
            registered: false,
            bank: 0,
            index: 128,
            delta: 0,
        },
        K::ProgramChange {
            program: 128,
            bank: None,
        },
        K::ProgramChange {
            program: 0,
            bank: Some((0, 128)),
        },
        K::ControlChange {
            controller: 128,
            value: 0,
        },
    ] {
        assert!(M::new(0, 0, kind).is_err());
    }
}
