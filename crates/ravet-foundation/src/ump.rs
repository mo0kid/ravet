//! Fixed-storage UMP framing. Semantic interpretation is independent of framing.

/// Malformed framing, incomplete input, capacity or short-message semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UmpError {
    /// The number of words does not match the message-type size.
    Length,
    /// End of input occurred partway through a packet.
    Truncated,
    /// The destination cannot hold the complete packet.
    Capacity,
    /// Message is outside the selected semantic codec.
    Semantic,
}

/// One complete Universal MIDI Packet, retaining every bit including reserved data.
///
/// This validates framing only, not message semantics or negotiated protocol.
/// Words are numeric values; transport byte order must be selected explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UmpPacket {
    words: [u32; 4],
    length: u8,
}

impl UmpPacket {
    /// Required word count for a first word, including reserved message types.
    pub const fn word_count(first: u32) -> usize {
        [1, 1, 1, 2, 2, 4, 1, 1, 2, 2, 2, 3, 3, 4, 4, 4][(first >> 28) as usize]
    }

    /// Copies exactly one complete packet. No allocation or partial acceptance.
    pub fn from_words(words: &[u32]) -> Result<Self, UmpError> {
        let first = *words.first().ok_or(UmpError::Length)?;
        if words.len() != Self::word_count(first) {
            return Err(UmpError::Length);
        }
        let mut storage = [0; 4];
        storage[..words.len()].copy_from_slice(words);
        Ok(Self {
            words: storage,
            length: words.len() as u8,
        })
    }

    /// Reads an explicitly big-endian byte representation of one packet.
    /// This is an application serialization choice, not a universal UMP transport.
    pub fn from_be_bytes(bytes: &[u8]) -> Result<Self, UmpError> {
        if bytes.is_empty() || bytes.len() > 16 || !bytes.len().is_multiple_of(4) {
            return Err(UmpError::Length);
        }
        let mut words = [0; 4];
        for (word, chunk) in words.iter_mut().zip(bytes.as_chunks::<4>().0) {
            *word = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        Self::from_words(&words[..bytes.len() / 4])
    }

    /// Writes complete big-endian words, preserving destination tails.
    /// An undersized destination is unchanged.
    pub fn write_be_bytes(self, output: &mut [u8]) -> Result<usize, UmpError> {
        let bytes = self.as_words().len() * 4;
        if output.len() < bytes {
            return Err(UmpError::Capacity);
        }
        for (word, chunk) in self
            .as_words()
            .iter()
            .zip(output[..bytes].as_chunks_mut::<4>().0)
        {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        Ok(bytes)
    }

    /// Complete numeric words in specification order.
    pub fn as_words(&self) -> &[u32] {
        &self.words[..usize::from(self.length)]
    }

    /// Four-bit message type.
    pub const fn message_type(self) -> u8 {
        (self.words[0] >> 28) as u8
    }

    /// Group for currently defined grouped types; utility, stream and reserved
    /// message types return `None` rather than inventing addressing semantics.
    pub const fn group(self) -> Option<u8> {
        match self.message_type() {
            1..=5 | 13 => Some(((self.words[0] >> 24) & 15) as u8),
            _ => None,
        }
    }
}

/// Incremental nonallocating framing of numeric UMP words.
///
/// Start at a known packet boundary. Every message type has a defined size, so
/// reserved packets can be retained without interpreting them. There is no
/// resynchronization heuristic, timing, SysEx assembly or protocol negotiation.
#[derive(Default, Debug)]
pub struct UmpDecoder {
    words: [u32; 4],
    used: usize,
}

impl UmpDecoder {
    /// Empty stream at a packet boundary.
    pub const fn new() -> Self {
        Self {
            words: [0; 4],
            used: 0,
        }
    }

    /// Consumes one word and yields only complete packets.
    pub fn push(&mut self, word: u32) -> Option<UmpPacket> {
        self.words[self.used] = word;
        self.used += 1;
        if self.used == UmpPacket::word_count(self.words[0]) {
            let packet = UmpPacket {
                words: self.words,
                length: self.used as u8,
            };
            self.used = 0;
            // Canonical unused storage makes equality independent of stream history.
            let mut packet = packet;
            packet.words[usize::from(packet.length)..].fill(0);
            Some(packet)
        } else {
            None
        }
    }

    /// Verifies end of input without discarding an incomplete packet.
    pub fn finish(&self) -> Result<(), UmpError> {
        if self.used == 0 {
            Ok(())
        } else {
            Err(UmpError::Truncated)
        }
    }

    /// Discards an incomplete packet; the next word must begin a new packet.
    pub fn reset(&mut self) {
        self.used = 0;
    }
}

/// MIDI 1.0 short messages carried by UMP MT=1 (system) or MT=2 (channel).
/// This changes packet representation only; it does not convert MIDI protocols.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UmpMidi1 {
    group: u8,
    message: crate::MidiMessage,
}

impl UmpMidi1 {
    /// Selects a zero-based group. F7 End-of-Exclusive is reserved in UMP
    /// system messages and rejects even though it is a valid byte-stream status.
    pub fn new(group: u8, message: crate::MidiMessage) -> Result<Self, UmpError> {
        if group > 15 || message.as_bytes()[0] == 0xf7 {
            return Err(UmpError::Semantic);
        }
        Ok(Self { group, message })
    }
    /// Decodes system or channel short messages, ignoring reserved padding and
    /// high reserved bits in data fields. The original packet retains those bits.
    pub fn from_packet(packet: UmpPacket) -> Result<Self, UmpError> {
        let word = packet.as_words()[0];
        let status = (word >> 16) as u8;
        let length = match (packet.message_type(), status) {
            (2, 0x80..=0xbf | 0xe0..=0xef) | (1, 0xf2) => 3,
            (2, 0xc0..=0xdf) | (1, 0xf1 | 0xf3) => 2,
            (1, 0xf6 | 0xf8 | 0xfa..=0xfc | 0xfe..=0xff) => 1,
            _ => return Err(UmpError::Semantic),
        };
        let bytes = [status, (word >> 8) as u8 & 127, word as u8 & 127];
        let message =
            crate::MidiMessage::from_bytes(&bytes[..length]).map_err(|_| UmpError::Semantic)?;
        Self::new(((word >> 24) & 15) as u8, message)
    }
    /// Canonical numeric UMP word, with unused bytes zeroed.
    pub fn packet(self) -> UmpPacket {
        let bytes = self.message.as_bytes();
        let kind = if bytes[0] < 0xf0 { 2 } else { 1 };
        let word = kind << 28
            | u32::from(self.group) << 24
            | u32::from(bytes[0]) << 16
            | u32::from(*bytes.get(1).unwrap_or(&0)) << 8
            | u32::from(*bytes.get(2).unwrap_or(&0));
        UmpPacket::from_words(&[word]).expect("validated single-word MIDI 1 message")
    }
    /// Zero-based UMP group.
    pub const fn group(self) -> u8 {
        self.group
    }
    /// Original MIDI 1.0 message, preserving defined wire data.
    pub const fn message(self) -> crate::MidiMessage {
        self.message
    }
}
