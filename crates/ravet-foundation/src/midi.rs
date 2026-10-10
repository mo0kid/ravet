//! Nonallocating MIDI 1.0 short messages and byte-stream running status.

/// Validated MIDI 1.0 message with at most two data bytes, preserving wire bytes.
/// SysEx and MIDI 2.0 UMP require separate storage/protocols and are not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiMessage {
    bytes: [u8; 3],
    len: u8,
}

/// Decoded channel voice data uses zero-based channels and unscaled MIDI units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiKind {
    NoteOff {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOn {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    PolyPressure {
        channel: u8,
        key: u8,
        pressure: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
    ChannelPressure {
        channel: u8,
        pressure: u8,
    },
    /// Unsigned 14-bit value; center is 8192. Semitone range belongs to the receiver.
    PitchBend {
        channel: u8,
        value: u16,
    },
    QuarterFrame(u8),
    SongPosition(u16),
    SongSelect(u8),
    TuneRequest,
    EndOfExclusive,
    TimingClock,
    Start,
    Continue,
    Stop,
    ActiveSensing,
    Reset,
}

/// Small errors for invalid messages or byte-stream state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiError {
    InvalidStatus,
    InvalidLength,
    InvalidData,
    MissingStatus,
    UnsupportedSysEx,
}

fn message_length(status: u8) -> Result<u8, MidiError> {
    match status {
        0x80..=0xbf | 0xe0..=0xef | 0xf2 => Ok(3),
        0xc0..=0xdf | 0xf1 | 0xf3 => Ok(2),
        0xf6..=0xf8 | 0xfa..=0xfc | 0xfe..=0xff => Ok(1),
        0xf0 => Err(MidiError::UnsupportedSysEx),
        _ => Err(MidiError::InvalidStatus),
    }
}
impl MidiMessage {
    /// Validate one complete message, with explicit status and exact wire length.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MidiError> {
        let status = *bytes.first().ok_or(MidiError::InvalidLength)?;
        let len = message_length(status)?;
        if bytes.len() != usize::from(len) {
            return Err(MidiError::InvalidLength);
        }
        if bytes[1..].iter().any(|&byte| byte >= 128) {
            return Err(MidiError::InvalidData);
        }
        let mut storage = [0; 3];
        storage[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            bytes: storage,
            len,
        })
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
    /// Decode channel voice and system messages. Note-on with zero velocity is
    /// interpreted as note-off with release velocity zero, preserving raw bytes.
    pub fn kind(self) -> MidiKind {
        let [status, a, b] = self.bytes;
        let channel = status & 15;
        match status {
            0x80..=0x8f => MidiKind::NoteOff {
                channel,
                key: a,
                velocity: b,
            },
            0x90..=0x9f if b == 0 => MidiKind::NoteOff {
                channel,
                key: a,
                velocity: 0,
            },
            0x90..=0x9f => MidiKind::NoteOn {
                channel,
                key: a,
                velocity: b,
            },
            0xa0..=0xaf => MidiKind::PolyPressure {
                channel,
                key: a,
                pressure: b,
            },
            0xb0..=0xbf => MidiKind::ControlChange {
                channel,
                controller: a,
                value: b,
            },
            0xc0..=0xcf => MidiKind::ProgramChange {
                channel,
                program: a,
            },
            0xd0..=0xdf => MidiKind::ChannelPressure {
                channel,
                pressure: a,
            },
            0xe0..=0xef => MidiKind::PitchBend {
                channel,
                value: u16::from(a) | (u16::from(b) << 7),
            },
            0xf1 => MidiKind::QuarterFrame(a),
            0xf2 => MidiKind::SongPosition(u16::from(a) | (u16::from(b) << 7)),
            0xf3 => MidiKind::SongSelect(a),
            0xf6 => MidiKind::TuneRequest,
            0xf7 => MidiKind::EndOfExclusive,
            0xf8 => MidiKind::TimingClock,
            0xfa => MidiKind::Start,
            0xfb => MidiKind::Continue,
            0xfc => MidiKind::Stop,
            0xfe => MidiKind::ActiveSensing,
            0xff => MidiKind::Reset,
            _ => unreachable!("message constructor validates status"),
        }
    }
}

/// Incremental MIDI 1.0 byte decoder; no allocation, clock or event scheduling.
/// Channel messages retain running status. System common cancels it; real-time
/// bytes are emitted immediately without disturbing a partial message. A new
/// non-real-time status discards any incomplete message. Undefined real-time
/// statuses reject without changing state; other invalid statuses clear state.
/// SysEx start reports UnsupportedSysEx once, then skips its data until EOX or
/// another non-real-time status. Real-time messages within SysEx still emit.
#[derive(Clone, Debug, Default)]
pub struct MidiDecoder {
    bytes: [u8; 3],
    used: u8,
    running: Option<u8>,
    sysex: bool,
}
impl MidiDecoder {
    /// Discard partial data and running status when disconnecting/resetting a port.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn push(&mut self, byte: u8) -> Result<Option<MidiMessage>, MidiError> {
        if byte >= 0xf8 {
            return MidiMessage::from_bytes(&[byte]).map(Some);
        }
        if byte >= 0x80 {
            self.used = 0;
            self.running = None;
            self.sysex = byte == 0xf0;
            let len = message_length(byte)?;
            if byte == 0xf7 {
                return Ok(Some(MidiMessage::from_bytes(&[byte])?));
            }
            if len == 1 {
                return Ok(Some(MidiMessage::from_bytes(&[byte])?));
            }
            self.bytes[0] = byte;
            self.used = 1;
            self.running = (byte < 0xf0).then_some(byte);
            return Ok(None);
        }
        if self.sysex {
            return Ok(None);
        }
        if self.used == 0 {
            self.bytes[0] = self.running.ok_or(MidiError::MissingStatus)?;
            self.used = 1;
        }
        self.bytes[usize::from(self.used)] = byte;
        self.used += 1;
        let len = message_length(self.bytes[0])?;
        if self.used == len {
            self.used = 0;
            Ok(Some(MidiMessage::from_bytes(
                &self.bytes[..usize::from(len)],
            )?))
        } else {
            Ok(None)
        }
    }
}
impl core::fmt::Display for MidiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidStatus => "invalid or undefined MIDI status",
            Self::InvalidLength => "incorrect MIDI message length",
            Self::InvalidData => "MIDI data bytes must be seven-bit values",
            Self::MissingStatus => "MIDI data arrived without a status",
            Self::UnsupportedSysEx => "SysEx requires separate bounded storage",
        })
    }
}
impl core::error::Error for MidiError {}
