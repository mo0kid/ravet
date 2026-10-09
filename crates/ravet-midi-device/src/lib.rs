//! Explicit MIDI 1.0 device connections outside the portable umbrella.
//!
//! Enumerate/connect/send/close on control or worker threads. Prepared packet
//! handoff copies bounded messages without framework allocation or locks; native
//! backend callbacks and output sends do not carry an audio-deadline guarantee.
//! Timestamp origins are connection-local microseconds, not audio sample time.

mod ump;
pub use midir;
use ravet_foundation::MidiMessage;
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
};
pub use ump::{PreparedUmpInput, TimedUmp, UmpDelivery, UmpInputError, UmpProtocol, UmpReceiver};

/// Maximum complete MIDI 1.0 packet, including SysEx start/end delimiters.
pub const MAXIMUM_PACKET_BYTES: usize = 1024;

/// Rejected preparation, packet, device selection or native backend operation.
#[derive(Debug)]
pub enum MidiDeviceError {
    Capacity,
    InvalidPacket,
    Unavailable,
    Backend(String),
}
impl std::fmt::Display for MidiDeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MIDI device error: {self:?}")
    }
}
impl std::error::Error for MidiDeviceError {}
fn backend(error: impl std::fmt::Display) -> MidiDeviceError {
    MidiDeviceError::Backend(error.to_string())
}

/// Validate one explicit-status short message or a complete seven-bit SysEx.
/// No running status, fragmented SysEx, embedded real-time status, or UMP.
pub fn validate_packet(bytes: &[u8]) -> Result<(), MidiDeviceError> {
    if bytes.len() > MAXIMUM_PACKET_BYTES {
        return Err(MidiDeviceError::Capacity);
    }
    if bytes.first() == Some(&0xf0) {
        if bytes.len() >= 2
            && bytes.last() == Some(&0xf7)
            && bytes[1..bytes.len() - 1].iter().all(|byte| *byte < 128)
        {
            return Ok(());
        }
        return Err(MidiDeviceError::InvalidPacket);
    }
    MidiMessage::from_bytes(bytes)
        .map(|_| ())
        .map_err(|_| MidiDeviceError::InvalidPacket)
}

/// Owned stack packet returned from the ring; only `as_bytes()` is initialized data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiPacket {
    pub timestamp_microseconds: u64,
    length: usize,
    bytes: [u8; MAXIMUM_PACKET_BYTES],
}
impl MidiPacket {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}
struct Slot {
    timestamp: AtomicU64,
    length: AtomicUsize,
    bytes: [AtomicU8; MAXIMUM_PACKET_BYTES],
}
#[repr(align(64))]
struct Counter(AtomicUsize);
struct Queue {
    slots: Vec<Slot>,
    head: Counter,
    tail: Counter,
    accepted: AtomicU64,
    dropped: AtomicU64,
    invalid: AtomicU64,
}
/// Relaxed snapshots may contain counts from adjacent packets; counters wrap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MidiInputStatistics {
    pub accepted: u64,
    pub dropped_full: u64,
    pub invalid: u64,
}
impl Queue {
    fn statistics(&self) -> MidiInputStatistics {
        MidiInputStatistics {
            accepted: self.accepted.load(Ordering::Relaxed),
            dropped_full: self.dropped.load(Ordering::Relaxed),
            invalid: self.invalid.load(Ordering::Relaxed),
        }
    }
}
/// Unique callback producer. Prepare and destroy outside callbacks.
pub struct PreparedMidiInput {
    queue: Arc<Queue>,
}
/// Unique consumer. Pop preserves FIFO packets and their unconverted timestamps.
pub struct MidiReceiver {
    queue: Arc<Queue>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureStatus {
    Accepted,
    Full,
    Invalid,
}
impl PreparedMidiInput {
    /// Allocate a power-of-two capacity of 1..=1,024 complete packet slots.
    pub fn new(capacity: usize) -> Result<(Self, MidiReceiver), MidiDeviceError> {
        if !(1..=1024).contains(&capacity) || !capacity.is_power_of_two() {
            return Err(MidiDeviceError::Capacity);
        }
        let queue = Arc::new(Queue {
            slots: (0..capacity)
                .map(|_| Slot {
                    timestamp: AtomicU64::new(0),
                    length: AtomicUsize::new(0),
                    bytes: std::array::from_fn(|_| AtomicU8::new(0)),
                })
                .collect(),
            head: Counter(AtomicUsize::new(0)),
            tail: Counter(AtomicUsize::new(0)),
            accepted: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            invalid: AtomicU64::new(0),
        });
        Ok((
            Self {
                queue: queue.clone(),
            },
            MidiReceiver { queue },
        ))
    }
    /// Reject malformed packets; when full drop the newest complete packet.
    /// No allocation, deallocation, locking, I/O, or timestamp scheduling.
    pub fn capture(&mut self, timestamp_microseconds: u64, bytes: &[u8]) -> CaptureStatus {
        if validate_packet(bytes).is_err() {
            self.queue.invalid.fetch_add(1, Ordering::Relaxed);
            return CaptureStatus::Invalid;
        }
        let head = self.queue.head.0.load(Ordering::Relaxed);
        let tail = self.queue.tail.0.load(Ordering::Acquire);
        if head.wrapping_sub(tail) == self.queue.slots.len() {
            self.queue.dropped.fetch_add(1, Ordering::Relaxed);
            return CaptureStatus::Full;
        }
        let slot = &self.queue.slots[head & (self.queue.slots.len() - 1)];
        for (target, source) in slot.bytes.iter().zip(bytes) {
            target.store(*source, Ordering::Relaxed);
        }
        slot.length.store(bytes.len(), Ordering::Relaxed);
        slot.timestamp
            .store(timestamp_microseconds, Ordering::Relaxed);
        self.queue
            .head
            .0
            .store(head.wrapping_add(1), Ordering::Release);
        self.queue.accepted.fetch_add(1, Ordering::Relaxed);
        CaptureStatus::Accepted
    }
    pub fn statistics(&self) -> MidiInputStatistics {
        self.queue.statistics()
    }
}
impl MidiReceiver {
    /// Remove the oldest packet, or return None without blocking when empty.
    pub fn pop(&mut self) -> Option<MidiPacket> {
        let tail = self.queue.tail.0.load(Ordering::Relaxed);
        if tail == self.queue.head.0.load(Ordering::Acquire) {
            return None;
        }
        let slot = &self.queue.slots[tail & (self.queue.slots.len() - 1)];
        let length = slot.length.load(Ordering::Relaxed);
        let mut packet = MidiPacket {
            timestamp_microseconds: slot.timestamp.load(Ordering::Relaxed),
            length,
            bytes: [0; MAXIMUM_PACKET_BYTES],
        };
        for (target, source) in packet.bytes[..length].iter_mut().zip(&slot.bytes) {
            *target = source.load(Ordering::Relaxed);
        }
        self.queue
            .tail
            .0
            .store(tail.wrapping_add(1), Ordering::Release);
        Some(packet)
    }
    pub fn statistics(&self) -> MidiInputStatistics {
        self.queue.statistics()
    }
}

/// Backend-local identifiers; names need not be unique and IDs can change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiPortInfo {
    pub id: String,
    pub name: String,
}
pub fn input_ports() -> Result<Vec<MidiPortInfo>, MidiDeviceError> {
    let input = midir::MidiInput::new("RAVET enumerate inputs").map_err(backend)?;
    input
        .ports()
        .iter()
        .map(|port| {
            Ok(MidiPortInfo {
                id: port.id(),
                name: input.port_name(port).map_err(backend)?,
            })
        })
        .collect()
}
pub fn output_ports() -> Result<Vec<MidiPortInfo>, MidiDeviceError> {
    let output = midir::MidiOutput::new("RAVET enumerate outputs").map_err(backend)?;
    output
        .ports()
        .iter()
        .map(|port| {
            Ok(MidiPortInfo {
                id: port.id(),
                name: output.port_name(port).map_err(backend)?,
            })
        })
        .collect()
}
/// Owned native connection. Close/drop outside callbacks; no implicit reconnect.
pub struct MidiInputStream {
    connection: midir::MidiInputConnection<PreparedMidiInput>,
    queue: Arc<Queue>,
}
impl MidiInputStream {
    pub fn statistics(&self) -> MidiInputStatistics {
        self.queue.statistics()
    }
    /// Synchronize native teardown before returning producer ownership.
    pub fn close(self) -> PreparedMidiInput {
        self.connection.close().1
    }
}
/// Connect the exact current input ID. All message classes reach validation.
pub fn open_input(
    id: &str,
    capacity: usize,
) -> Result<(MidiInputStream, MidiReceiver), MidiDeviceError> {
    let mut input = midir::MidiInput::new("RAVET input").map_err(backend)?;
    input.ignore(midir::Ignore::None);
    let port = input
        .find_port_by_id(id)
        .ok_or(MidiDeviceError::Unavailable)?;
    let (prepared, receiver) = PreparedMidiInput::new(capacity)?;
    let queue = prepared.queue.clone();
    let connection = input
        .connect(
            &port,
            "RAVET input",
            |time, bytes, prepared| {
                prepared.capture(time, bytes);
            },
            prepared,
        )
        .map_err(backend)?;
    Ok((MidiInputStream { connection, queue }, receiver))
}
/// Explicit virtual endpoint for Unix backend validation or consumer routing.
#[cfg(unix)]
pub fn open_virtual_input(
    name: &str,
    capacity: usize,
) -> Result<(MidiInputStream, MidiReceiver), MidiDeviceError> {
    use midir::os::unix::VirtualInput;
    let mut input = midir::MidiInput::new(name).map_err(backend)?;
    input.ignore(midir::Ignore::None);
    let (prepared, receiver) = PreparedMidiInput::new(capacity)?;
    let queue = prepared.queue.clone();
    let connection = input
        .create_virtual(
            name,
            |time, bytes, prepared| {
                prepared.capture(time, bytes);
            },
            prepared,
        )
        .map_err(backend)?;
    Ok((MidiInputStream { connection, queue }, receiver))
}
/// Output sends are native backend calls on control/worker threads.
pub struct MidiOutputStream {
    connection: midir::MidiOutputConnection,
}
pub fn open_output(id: &str) -> Result<MidiOutputStream, MidiDeviceError> {
    let output = midir::MidiOutput::new("RAVET output").map_err(backend)?;
    let port = output
        .find_port_by_id(id)
        .ok_or(MidiDeviceError::Unavailable)?;
    let connection = output.connect(&port, "RAVET output").map_err(backend)?;
    Ok(MidiOutputStream { connection })
}
impl MidiOutputStream {
    /// Validate and immediately send a complete packet. Does not schedule time.
    pub fn send(&mut self, bytes: &[u8]) -> Result<(), MidiDeviceError> {
        validate_packet(bytes)?;
        self.connection.send(bytes).map_err(backend)
    }
    pub fn close(self) {
        self.connection.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_counters_wrap_without_losing_slots() {
        let (mut input, mut receiver) = PreparedMidiInput::new(2).unwrap();
        input.queue.head.0.store(usize::MAX - 1, Ordering::Relaxed);
        input.queue.tail.0.store(usize::MAX - 1, Ordering::Relaxed);
        for time in 0..8 {
            assert_eq!(input.capture(time, &[0xf8]), CaptureStatus::Accepted);
            assert_eq!(receiver.pop().unwrap().timestamp_microseconds, time);
            assert!(receiver.pop().is_none());
        }
    }
}
