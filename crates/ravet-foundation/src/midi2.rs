//! Lossless-resolution MIDI 2.0 channel-voice interpretation of UMP words.
use crate::UmpPacket;

/// Invalid group/channel, seven-bit field or unsupported message type/status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Midi2Error;

/// MIDI 2.0 channel voice data. No resolution scaling or protocol conversion.
///
/// Controller meanings, attributes, per-note ownership and negotiated profiles
/// belong to the consumer. Receivers ignore reserved bits; encoding sets them to
/// zero. Unknown note attribute types are retained for future interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Midi2Kind {
    /// Registered (true) or assignable (false) per-note controller; index is eight bits.
    PerNoteController {
        registered: bool,
        key: u8,
        index: u8,
        value: u32,
    },
    /// Registered (true) or assignable (false) absolute controller.
    Controller {
        registered: bool,
        bank: u8,
        index: u8,
        value: u32,
    },
    /// Signed two's-complement relative controller change.
    RelativeController {
        registered: bool,
        bank: u8,
        index: u8,
        delta: i32,
    },
    /// Unsigned bipolar per-note bend, centered at 0x80000000.
    PerNotePitchBend { key: u8, value: u32 },
    /// Release velocity and uninterpreted attribute type/data.
    NoteOff {
        key: u8,
        velocity: u16,
        attribute_type: u8,
        attribute: u16,
    },
    /// Zero velocity remains Note On, unlike MIDI 1.0.
    NoteOn {
        key: u8,
        velocity: u16,
        attribute_type: u8,
        attribute: u16,
    },
    /// Per-key pressure at full 32-bit resolution.
    PolyPressure { key: u8, value: u32 },
    /// Seven-bit channel controller index and full 32-bit value.
    ControlChange { controller: u8, value: u32 },
    /// Program and optional (MSB, LSB) bank selection, all seven-bit.
    ProgramChange { program: u8, bank: Option<(u8, u8)> },
    /// Channel pressure at full 32-bit resolution.
    ChannelPressure { value: u32 },
    /// Unsigned bipolar channel bend, centered at 0x80000000.
    PitchBend { value: u32 },
    /// Detach previous notes' controllers and/or reset controllers to defaults.
    PerNoteManagement { key: u8, detach: bool, reset: bool },
}

/// Validated group/channel and channel-voice fields, using zero-based addressing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Midi2Message {
    group: u8,
    channel: u8,
    kind: Midi2Kind,
}

impl Midi2Message {
    /// Validates sender field widths without imposing controller/profile policy.
    /// Attribute data is canonicalized to zero when attribute type is zero.
    pub fn new(group: u8, channel: u8, mut kind: Midi2Kind) -> Result<Self, Midi2Error> {
        if group > 15 || channel > 15 {
            return Err(Midi2Error);
        }
        let valid = match &mut kind {
            Midi2Kind::PerNoteController { key, .. }
            | Midi2Kind::PerNotePitchBend { key, .. }
            | Midi2Kind::PolyPressure { key, .. }
            | Midi2Kind::PerNoteManagement { key, .. } => *key < 128,
            Midi2Kind::NoteOn {
                key,
                attribute_type,
                attribute,
                ..
            }
            | Midi2Kind::NoteOff {
                key,
                attribute_type,
                attribute,
                ..
            } => {
                if *attribute_type == 0 {
                    *attribute = 0;
                }
                *key < 128
            }
            Midi2Kind::Controller { bank, index, .. }
            | Midi2Kind::RelativeController { bank, index, .. } => *bank < 128 && *index < 128,
            Midi2Kind::ControlChange { controller, .. } => *controller < 128,
            Midi2Kind::ProgramChange { program, bank } => {
                *program < 128 && bank.is_none_or(|(msb, lsb)| msb < 128 && lsb < 128)
            }
            Midi2Kind::ChannelPressure { .. } | Midi2Kind::PitchBend { .. } => true,
        };
        if !valid {
            return Err(Midi2Error);
        }
        Ok(Self {
            group,
            channel,
            kind,
        })
    }

    /// Interprets defined MT=4 statuses, ignoring reserved bits as specified.
    /// Unknown status 7 rejects; raw `UmpPacket` remains available unchanged.
    pub fn from_packet(packet: UmpPacket) -> Result<Self, Midi2Error> {
        if packet.message_type() != 4 {
            return Err(Midi2Error);
        }
        let words = packet.as_words();
        let first = words[0];
        let data = words[1];
        let high = ((first >> 8) & 127) as u8;
        let low = first as u8;
        let status = (first >> 20) & 15;
        let kind = match status {
            0 | 1 => Midi2Kind::PerNoteController {
                registered: status == 0,
                key: high,
                index: low,
                value: data,
            },
            2 | 3 => Midi2Kind::Controller {
                registered: status == 2,
                bank: high,
                index: low & 127,
                value: data,
            },
            4 | 5 => Midi2Kind::RelativeController {
                registered: status == 4,
                bank: high,
                index: low & 127,
                delta: data as i32,
            },
            6 => Midi2Kind::PerNotePitchBend {
                key: high,
                value: data,
            },
            8 => Midi2Kind::NoteOff {
                key: high,
                velocity: (data >> 16) as u16,
                attribute_type: low,
                attribute: data as u16,
            },
            9 => Midi2Kind::NoteOn {
                key: high,
                velocity: (data >> 16) as u16,
                attribute_type: low,
                attribute: data as u16,
            },
            10 => Midi2Kind::PolyPressure {
                key: high,
                value: data,
            },
            11 => Midi2Kind::ControlChange {
                controller: high,
                value: data,
            },
            12 => Midi2Kind::ProgramChange {
                program: (data >> 24) as u8 & 127,
                bank: if low & 1 != 0 {
                    Some(((data >> 8) as u8 & 127, data as u8 & 127))
                } else {
                    None
                },
            },
            13 => Midi2Kind::ChannelPressure { value: data },
            14 => Midi2Kind::PitchBend { value: data },
            15 => Midi2Kind::PerNoteManagement {
                key: high,
                detach: low & 2 != 0,
                reset: low & 1 != 0,
            },
            _ => return Err(Midi2Error),
        };
        Self::new(((first >> 24) & 15) as u8, ((first >> 16) & 15) as u8, kind)
    }

    /// Encodes canonical MT=4 words, preserving full data resolution.
    pub fn packet(self) -> UmpPacket {
        let (status, high, low, data): (u32, u8, u8, u32) = match self.kind {
            Midi2Kind::PerNoteController {
                registered,
                key,
                index,
                value,
            } => (if registered { 0 } else { 1 }, key, index, value),
            Midi2Kind::Controller {
                registered,
                bank,
                index,
                value,
            } => (if registered { 2 } else { 3 }, bank, index, value),
            Midi2Kind::RelativeController {
                registered,
                bank,
                index,
                delta,
            } => (if registered { 4 } else { 5 }, bank, index, delta as u32),
            Midi2Kind::PerNotePitchBend { key, value } => (6, key, 0, value),
            Midi2Kind::NoteOff {
                key,
                velocity,
                attribute_type,
                attribute,
            } => (
                8,
                key,
                attribute_type,
                u32::from(velocity) << 16 | u32::from(attribute),
            ),
            Midi2Kind::NoteOn {
                key,
                velocity,
                attribute_type,
                attribute,
            } => (
                9,
                key,
                attribute_type,
                u32::from(velocity) << 16 | u32::from(attribute),
            ),
            Midi2Kind::PolyPressure { key, value } => (10, key, 0, value),
            Midi2Kind::ControlChange { controller, value } => (11, controller, 0, value),
            Midi2Kind::ProgramChange { program, bank } => (
                12,
                0,
                u8::from(bank.is_some()),
                u32::from(program) << 24
                    | bank.map_or(0, |(msb, lsb)| u32::from(msb) << 8 | u32::from(lsb)),
            ),
            Midi2Kind::ChannelPressure { value } => (13, 0, 0, value),
            Midi2Kind::PitchBend { value } => (14, 0, 0, value),
            Midi2Kind::PerNoteManagement { key, detach, reset } => {
                (15, key, u8::from(detach) << 1 | u8::from(reset), 0)
            }
        };
        UmpPacket::from_words(&[
            0x40000000
                | u32::from(self.group) << 24
                | status << 20
                | u32::from(self.channel) << 16
                | u32::from(high) << 8
                | u32::from(low),
            data,
        ])
        .expect("validated two-word MIDI 2 message")
    }

    /// Zero-based group.
    pub const fn group(self) -> u8 {
        self.group
    }
    /// Zero-based channel within the group.
    pub const fn channel(self) -> u8 {
        self.channel
    }
    /// Full-resolution interpreted fields.
    pub const fn kind(self) -> Midi2Kind {
        self.kind
    }
}
