//! Fixed-storage SysEx7/SysEx8 UMP chunks and explicit bounded reassembly.
use crate::UmpPacket;

/// Position of a payload chunk in a SysEx message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SysExForm {
    /// The whole message is in one chunk.
    Complete = 0,
    /// First chunk of a multi-packet message.
    Start = 1,
    /// Intermediate chunk, even if its payload is empty.
    Continue = 2,
    /// Last chunk.
    End = 3,
}

/// Invalid chunk fields, stream ordering or destination capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UmpSysExError {
    /// Invalid type, form, count, group or seven-bit payload.
    Invalid,
    /// Chunk does not belong to the current assembly or starts over an active one.
    Sequence,
    /// Complete accepted payload would exceed the caller's storage.
    Capacity,
}

/// One SysEx7 or SysEx8 payload chunk; payload excludes F0/F7 and stream ID.
/// `None` stream means SysEx7; `Some(id)` means SysEx8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UmpSysExChunk {
    group: u8,
    stream: Option<u8>,
    form: SysExForm,
    payload: [u8; 13],
    length: u8,
    abort: bool,
}

impl UmpSysExChunk {
    /// Copies a validated payload: up to six seven-bit bytes for SysEx7 or
    /// thirteen unrestricted bytes for SysEx8. Empty chunks are valid.
    pub fn new(
        group: u8,
        stream: Option<u8>,
        form: SysExForm,
        payload: &[u8],
    ) -> Result<Self, UmpSysExError> {
        if group > 15
            || payload.len() > if stream.is_some() { 13 } else { 6 }
            || (stream.is_none() && payload.iter().any(|byte| *byte > 127))
        {
            return Err(UmpSysExError::Invalid);
        }
        let mut storage = [0; 13];
        storage[..payload.len()].copy_from_slice(payload);
        Ok(Self {
            group,
            stream,
            form,
            payload: storage,
            length: payload.len() as u8,
            abort: false,
        })
    }

    /// Constructs the explicit SysEx8 End/count=15 abort marker.
    pub fn abort(group: u8, stream: u8) -> Result<Self, UmpSysExError> {
        let mut chunk = Self::new(group, Some(stream), SysExForm::End, &[])?;
        chunk.abort = true;
        Ok(chunk)
    }

    /// Decodes a SysEx UMP, rejecting malformed counts/statuses and seven-bit
    /// payloads. Unused padding is ignored; abort data is ignored.
    pub fn from_packet(packet: UmpPacket) -> Result<Self, UmpSysExError> {
        let mut bytes = [0; 16];
        packet
            .write_be_bytes(&mut bytes)
            .map_err(|_| UmpSysExError::Invalid)?;
        let form = match bytes[1] >> 4 {
            0 => SysExForm::Complete,
            1 => SysExForm::Start,
            2 => SysExForm::Continue,
            3 => SysExForm::End,
            _ => return Err(UmpSysExError::Invalid),
        };
        let count = usize::from(bytes[1] & 15);
        let group = bytes[0] & 15;
        match packet.message_type() {
            3 if count <= 6 => Self::new(group, None, form, &bytes[2..2 + count]),
            5 if (1..=14).contains(&count) => {
                Self::new(group, Some(bytes[2]), form, &bytes[3..2 + count])
            }
            5 if count == 15 && form == SysExForm::End => Self::abort(group, bytes[2]),
            _ => Err(UmpSysExError::Invalid),
        }
    }

    /// Encodes a canonical packet with zero unused padding.
    pub fn packet(self) -> UmpPacket {
        let mut bytes = [0; 16];
        let (kind, offset, length, count) = if let Some(stream) = self.stream {
            bytes[2] = stream;
            (5, 3, 16, if self.abort { 15 } else { self.length + 1 })
        } else {
            (3, 2, 8, self.length)
        };
        bytes[0] = kind << 4 | self.group;
        bytes[1] = (self.form as u8) << 4 | count;
        bytes[offset..offset + usize::from(self.length)].copy_from_slice(self.payload());
        UmpPacket::from_be_bytes(&bytes[..length]).expect("validated SysEx chunk")
    }
    /// Zero-based group.
    pub const fn group(self) -> u8 {
        self.group
    }
    /// None for SysEx7, or the SysEx8 stream ID.
    pub const fn stream(self) -> Option<u8> {
        self.stream
    }
    /// Chunk position.
    pub const fn form(self) -> SysExForm {
        self.form
    }
    /// True only for the SysEx8 abort marker; no payload is exposed.
    pub const fn is_abort(self) -> bool {
        self.abort
    }
    /// Valid payload bytes, excluding stream ID and F0/F7.
    pub fn payload(&self) -> &[u8] {
        &self.payload[..usize::from(self.length)]
    }
}

/// Assembly result, distinguishing explicit abort from incomplete input.
#[derive(Debug, PartialEq, Eq)]
pub enum SysExAssembly<'a> {
    /// More chunks are required.
    Pending,
    /// Complete payload borrows the assembler until the next operation.
    Complete(&'a [u8]),
    /// Matching SysEx8 abort discarded the current message.
    Aborted,
}

/// Single selected stream assembler backed entirely by caller storage.
///
/// Consumers route chunks to separate assemblers for concurrent streams and
/// enforce SysEx7 interspersing rules on the whole UMP stream. Errors preserve
/// current accepted state and storage; reset explicitly discards that state.
/// No maximum message length is invented beyond the supplied slice capacity.
#[derive(Debug)]
pub struct SysExAssembler<'a> {
    storage: &'a mut [u8],
    active: Option<(u8, Option<u8>)>,
    used: usize,
}
impl<'a> SysExAssembler<'a> {
    /// Borrows storage without allocation or clearing it.
    pub fn new(storage: &'a mut [u8]) -> Self {
        Self {
            storage,
            active: None,
            used: 0,
        }
    }
    /// Discards partial state without changing caller storage.
    pub fn reset(&mut self) {
        self.active = None;
        self.used = 0;
    }
    /// Whether a Start has been accepted without a matching End/abort.
    pub fn is_pending(&self) -> bool {
        self.active.is_some()
    }
    /// Appends only after sequence and complete capacity checks.
    pub fn push(&mut self, chunk: UmpSysExChunk) -> Result<SysExAssembly<'_>, UmpSysExError> {
        let address = (chunk.group, chunk.stream);
        match chunk.form {
            SysExForm::Complete | SysExForm::Start if self.active.is_some() => {
                return Err(UmpSysExError::Sequence);
            }
            SysExForm::Continue | SysExForm::End if self.active != Some(address) => {
                return Err(UmpSysExError::Sequence);
            }
            _ => {}
        }
        if chunk.abort {
            self.reset();
            return Ok(SysExAssembly::Aborted);
        }
        let base = if self.active.is_some() { self.used } else { 0 };
        let end = base
            .checked_add(chunk.payload().len())
            .ok_or(UmpSysExError::Capacity)?;
        if end > self.storage.len() {
            return Err(UmpSysExError::Capacity);
        }
        self.storage[base..end].copy_from_slice(chunk.payload());
        match chunk.form {
            SysExForm::Complete | SysExForm::End => {
                self.active = None;
                self.used = 0;
                Ok(SysExAssembly::Complete(&self.storage[..end]))
            }
            SysExForm::Start | SysExForm::Continue => {
                self.active = Some(address);
                self.used = end;
                Ok(SysExAssembly::Pending)
            }
        }
    }
}
