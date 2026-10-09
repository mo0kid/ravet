//! Bounded OSC packet codecs and explicit worker-thread UDP transport.
//! These APIs allocate/block and never belong in audio callbacks. No socket,
//! destination, message dispatch or background network activity is implicit.
use std::{
    io,
    net::{SocketAddr, UdpSocket},
    time::Duration,
};
/// Largest supported packet, divisible by four and within UDP payload bounds.
pub const MAXIMUM_BYTES: usize = 65504;
#[derive(Clone, Debug, PartialEq)]
pub enum Argument<'a> {
    Int(i32),
    Float(f32),
    String(&'a str),
    Blob(&'a [u8]),
    Long(i64),
    Double(f64),
    TimeTag(u64),
    Bool(bool),
    Nil,
    Infinitum,
}
/// Bundle tags preserve raw NTP bits; 1 represents immediate execution. This
/// codec does not schedule packets, synchronize clocks or dispatch patterns.
#[derive(Clone, Debug, PartialEq)]
pub enum Packet<'a> {
    Message {
        address: &'a str,
        arguments: Vec<Argument<'a>>,
    },
    Bundle {
        time_tag: u64,
        elements: Vec<Packet<'a>>,
    },
}
#[derive(Debug)]
pub enum OscError {
    InvalidPacket,
    UnsupportedTag(u8),
    Capacity,
    Io(io::Error),
}
impl From<io::Error> for OscError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl std::fmt::Display for OscError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OSC error: {self:?}")
    }
}
impl std::error::Error for OscError {}
/// Decode at most 65504 bytes, eight nesting levels, 256 packets and 256
/// arguments per message. Strings/blobs borrow input; metadata allocates.
/// ASCII strings and all padding are validated; unknown tags/trailing data reject.
/// Nonfinite IEEE values are preserved; applications validate their own values.
pub fn decode(bytes: &[u8]) -> Result<Packet<'_>, OscError> {
    if bytes.len() > MAXIMUM_BYTES {
        return Err(OscError::Capacity);
    }
    parse(bytes, 1, &mut 0)
}
struct Reader<'a> {
    bytes: &'a [u8],
}
impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], OscError> {
        if len > self.bytes.len() {
            return Err(OscError::InvalidPacket);
        }
        let (data, rest) = self.bytes.split_at(len);
        self.bytes = rest;
        Ok(data)
    }
    fn string(&mut self) -> Result<&'a str, OscError> {
        let end = self
            .bytes
            .iter()
            .position(|v| *v == 0)
            .ok_or(OscError::InvalidPacket)?;
        let length = (end + 4) & !3;
        let bytes = self.take(length)?;
        if !bytes[..end].is_ascii() || bytes[end..].iter().any(|v| *v != 0) {
            return Err(OscError::InvalidPacket);
        }
        std::str::from_utf8(&bytes[..end]).map_err(|_| OscError::InvalidPacket)
    }
    fn word(&mut self) -> Result<u32, OscError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn wide(&mut self) -> Result<u64, OscError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
}
fn parse<'a>(bytes: &'a [u8], depth: usize, count: &mut usize) -> Result<Packet<'a>, OscError> {
    if depth > 8 || *count >= 256 {
        return Err(OscError::Capacity);
    }
    *count += 1;
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return Err(OscError::InvalidPacket);
    }
    let mut reader = Reader { bytes };
    let address = reader.string()?;
    let packet = if address == "#bundle" {
        let time_tag = reader.wide()?;
        let mut elements = Vec::new();
        while !reader.bytes.is_empty() {
            let length = reader.word()? as i32;
            if length <= 0 {
                return Err(OscError::InvalidPacket);
            }
            elements.push(parse(reader.take(length as usize)?, depth + 1, count)?);
        }
        Packet::Bundle { time_tag, elements }
    } else {
        if !address.starts_with('/') || address.len() > 1024 {
            return Err(OscError::InvalidPacket);
        }
        let tags = reader.string()?;
        if !tags.starts_with(',') {
            return Err(OscError::InvalidPacket);
        }
        if tags.len() > 257 {
            return Err(OscError::Capacity);
        }
        let mut arguments = Vec::with_capacity(tags.len() - 1);
        for tag in tags.bytes().skip(1) {
            arguments.push(match tag {
                b'i' => Argument::Int(reader.word()? as i32),
                b'f' => Argument::Float(f32::from_bits(reader.word()?)),
                b's' => Argument::String(reader.string()?),
                b'b' => {
                    let length = reader.word()? as i32;
                    if length < 0 {
                        return Err(OscError::InvalidPacket);
                    }
                    let data = reader.take(length as usize)?;
                    let padding = (4 - data.len() % 4) % 4;
                    if reader.take(padding)?.iter().any(|v| *v != 0) {
                        return Err(OscError::InvalidPacket);
                    }
                    Argument::Blob(data)
                }
                b'h' => Argument::Long(reader.wide()? as i64),
                b'd' => Argument::Double(f64::from_bits(reader.wide()?)),
                b't' => Argument::TimeTag(reader.wide()?),
                b'T' => Argument::Bool(true),
                b'F' => Argument::Bool(false),
                b'N' => Argument::Nil,
                b'I' => Argument::Infinitum,
                _ => return Err(OscError::UnsupportedTag(tag)),
            });
        }
        Packet::Message { address, arguments }
    };
    if !reader.bytes.is_empty() {
        return Err(OscError::InvalidPacket);
    }
    Ok(packet)
}
fn append(out: &mut Vec<u8>, data: &[u8]) -> Result<(), OscError> {
    if data.len() > MAXIMUM_BYTES - out.len() {
        return Err(OscError::Capacity);
    }
    out.extend_from_slice(data);
    Ok(())
}
fn string(out: &mut Vec<u8>, text: &str) -> Result<(), OscError> {
    if !text.is_ascii() || text.contains('\0') {
        return Err(OscError::InvalidPacket);
    }
    let length = text.len().checked_add(4).ok_or(OscError::Capacity)? & !3;
    if length > MAXIMUM_BYTES - out.len() {
        return Err(OscError::Capacity);
    }
    out.extend_from_slice(text.as_bytes());
    out.resize(out.len() + length - text.len(), 0);
    Ok(())
}
/// Encode deterministic network bytes with the same structural limits as decode.
pub fn encode(packet: &Packet<'_>) -> Result<Vec<u8>, OscError> {
    let mut bytes = Vec::new();
    write_packet(packet, &mut bytes, 1, &mut 0)?;
    Ok(bytes)
}
fn write_packet(
    packet: &Packet<'_>,
    out: &mut Vec<u8>,
    depth: usize,
    count: &mut usize,
) -> Result<(), OscError> {
    if depth > 8 || *count >= 256 {
        return Err(OscError::Capacity);
    }
    *count += 1;
    match packet {
        Packet::Bundle { time_tag, elements } => {
            string(out, "#bundle")?;
            append(out, &time_tag.to_be_bytes())?;
            for element in elements {
                let at = out.len();
                append(out, &[0; 4])?;
                write_packet(element, out, depth + 1, count)?;
                let size = (out.len() - at - 4) as u32;
                out[at..at + 4].copy_from_slice(&size.to_be_bytes());
            }
        }
        Packet::Message { address, arguments } => {
            if !address.starts_with('/') || address.len() > 1024 {
                return Err(OscError::InvalidPacket);
            }
            if arguments.len() > 256 {
                return Err(OscError::Capacity);
            }
            string(out, address)?;
            let mut tags = String::with_capacity(arguments.len() + 1);
            tags.push(',');
            for argument in arguments {
                tags.push(match argument {
                    Argument::Int(_) => 'i',
                    Argument::Float(_) => 'f',
                    Argument::String(_) => 's',
                    Argument::Blob(_) => 'b',
                    Argument::Long(_) => 'h',
                    Argument::Double(_) => 'd',
                    Argument::TimeTag(_) => 't',
                    Argument::Bool(true) => 'T',
                    Argument::Bool(false) => 'F',
                    Argument::Nil => 'N',
                    Argument::Infinitum => 'I',
                });
            }
            string(out, &tags)?;
            for argument in arguments {
                match argument {
                    Argument::Int(v) => append(out, &v.to_be_bytes())?,
                    Argument::Float(v) => append(out, &v.to_bits().to_be_bytes())?,
                    Argument::String(v) => string(out, v)?,
                    Argument::Blob(v) => {
                        if v.len() > MAXIMUM_BYTES {
                            return Err(OscError::Capacity);
                        }
                        append(out, &(v.len() as u32).to_be_bytes())?;
                        append(out, v)?;
                        append(out, &[0; 3][..(4 - v.len() % 4) % 4])?;
                    }
                    Argument::Long(v) => append(out, &v.to_be_bytes())?,
                    Argument::Double(v) => append(out, &v.to_bits().to_be_bytes())?,
                    Argument::TimeTag(v) => append(out, &v.to_be_bytes())?,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
/// Explicit allocating/blocking UDP transport on a worker or control thread.
/// No receiver thread, audio bridge, authentication or dispatch policy is supplied.
pub struct OscSocket {
    socket: UdpSocket,
    scratch: Vec<u8>,
    maximum: usize,
}
pub struct ReceivedPacket<'a> {
    pub sender: SocketAddr,
    pub packet: Packet<'a>,
}
impl OscSocket {
    /// Bind one explicit interface/address. Max bytes is divisible by four,
    /// 4..65504; timeout is positive and at most 300 seconds.
    pub fn bind(address: SocketAddr, maximum: usize, timeout: Duration) -> Result<Self, OscError> {
        if !(4..=MAXIMUM_BYTES).contains(&maximum)
            || !maximum.is_multiple_of(4)
            || timeout.is_zero()
            || timeout > Duration::from_secs(300)
        {
            return Err(OscError::Capacity);
        }
        let socket = UdpSocket::bind(address)?;
        socket.set_read_timeout(Some(timeout))?;
        Ok(Self {
            socket,
            scratch: vec![0; maximum + 4],
            maximum,
        })
    }
    pub fn local_address(&self) -> Result<SocketAddr, OscError> {
        Ok(self.socket.local_addr()?)
    }
    /// Send only to caller-supplied numeric destination; no implicit DNS/peer.
    pub fn send(&self, destination: SocketAddr, packet: &Packet<'_>) -> Result<usize, OscError> {
        let bytes = encode(packet)?;
        if bytes.len() > self.maximum {
            return Err(OscError::Capacity);
        }
        Ok(self.socket.send_to(&bytes, destination)?)
    }
    /// Decode one datagram, rejecting oversized packets even if socket truncates
    /// them. Borrowed packet data stays valid until the next mutable operation.
    /// Timeout and other I/O errors retain their io::Error kind.
    pub fn receive(&mut self) -> Result<ReceivedPacket<'_>, OscError> {
        let (length, sender) = self.socket.recv_from(&mut self.scratch)?;
        if length > self.maximum {
            return Err(OscError::Capacity);
        }
        Ok(ReceivedPacket {
            sender,
            packet: decode(&self.scratch[..length])?,
        })
    }
}
