use ravet_foundation::UmpPacket;
use ravet_midi_device::{PreparedUmpInput, UmpInputError, UmpProtocol, UmpReceiver};
use std::{
    ffi::CString,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
#[allow(unsafe_code)]
mod ffi;
/// Native endpoint selection/metadata or queue preparation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreMidiError {
    Name,
    Capacity,
    Queue(UmpInputError),
    Native(i32),
}
impl std::fmt::Display for CoreMidiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CoreMIDI error: {self:?}")
    }
}
impl std::error::Error for CoreMidiError {}
/// Control-thread snapshot; exact signed CoreMIDI unique ID, not a list index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreMidiEndpoint {
    pub id: i32,
    pub name: String,
}
fn name(value: &str) -> Result<CString, CoreMidiError> {
    if value.is_empty() || value.len() > 256 {
        return Err(CoreMidiError::Name);
    }
    CString::new(value).map_err(|_| CoreMidiError::Name)
}
fn protocol(value: UmpProtocol) -> i32 {
    match value {
        UmpProtocol::Midi1 => 1,
        UmpProtocol::Midi2 => 2,
    }
}
/// Enumerate at most 4096 sources with complete <=1023-byte UTF-8 display names.
/// Hotplug can invalidate the whole snapshot or later exact-ID connection.
pub fn sources() -> Result<Vec<CoreMidiEndpoint>, CoreMidiError> {
    ffi::endpoints(true)
}
/// Enumerate bounded destinations without opening or sending to any endpoint.
pub fn destinations() -> Result<Vec<CoreMidiEndpoint>, CoreMidiError> {
    ffi::endpoints(false)
}
#[derive(Default)]
struct Diagnostics {
    invalid_metadata: AtomicU64,
}
/// Protocol-selected native input with one bounded UMP producer.
/// The native shim rejects overlapping callback entry instead of sharing mutable
/// producer state. Queues retain an accepted prefix under capacity pressure.
/// Native metadata errors and overlapping whole callbacks have separate counters.
/// Construction and destruction allocate/block on control threads; destruction
/// gates Rust entry, disposes native ports and waits for an entered callback before
/// freeing its context on the control thread. No native callback deadline claim.
pub struct CoreUmpInput {
    port: ffi::Port,
    diagnostics: Arc<Diagnostics>,
    virtual_id: Option<i32>,
}
impl CoreUmpInput {
    /// Connect only the selected source unique ID, with explicit receive protocol.
    pub fn connect(
        client_name: &str,
        port_name: &str,
        source_id: i32,
        receive_protocol: UmpProtocol,
        capacity: usize,
    ) -> Result<(Self, UmpReceiver), CoreMidiError> {
        Self::open(
            client_name,
            port_name,
            source_id,
            false,
            receive_protocol,
            capacity,
        )
    }
    /// Create an owned named virtual destination. It does not broadcast to any
    /// hardware/source subscribers; outputs must select its returned unique ID.
    pub fn virtual_destination(
        client_name: &str,
        endpoint_name: &str,
        receive_protocol: UmpProtocol,
        capacity: usize,
    ) -> Result<(Self, UmpReceiver), CoreMidiError> {
        Self::open(
            client_name,
            endpoint_name,
            0,
            true,
            receive_protocol,
            capacity,
        )
    }
    fn open(
        client_name: &str,
        port_name: &str,
        source_id: i32,
        is_virtual: bool,
        receive_protocol: UmpProtocol,
        capacity: usize,
    ) -> Result<(Self, UmpReceiver), CoreMidiError> {
        let client_name = name(client_name)?;
        let port_name = name(port_name)?;
        let (producer, receiver) = PreparedUmpInput::new(capacity).map_err(CoreMidiError::Queue)?;
        let diagnostics = Arc::new(Diagnostics::default());
        let port = ffi::Port::input(
            &client_name,
            &port_name,
            source_id,
            is_virtual,
            receive_protocol,
            producer,
            Arc::clone(&diagnostics),
        )?;
        let virtual_id = if is_virtual {
            Some(port.virtual_id()?)
        } else {
            None
        };
        Ok((
            Self {
                port,
                diagnostics,
                virtual_id,
            },
            receiver,
        ))
    }
    pub fn virtual_destination_id(&self) -> Option<i32> {
        self.virtual_id
    }
    /// Relaxed wrapping count; native protocol/null/word-limit failures, separate
    /// from receiver framing errors/full-packet statistics.
    pub fn invalid_native_metadata(&self) -> u64 {
        self.diagnostics.invalid_metadata.load(Ordering::Relaxed)
    }
    pub fn overlapping_callbacks(&self) -> u64 {
        self.port.overlaps()
    }
}
/// Exact-ID native output. Selected protocol labels each complete packet; CoreMIDI
/// may translate to the destination's protocol. There is no hidden device fallback,
/// MIDI-CI negotiation, protocol semantic conversion or application retry here.
pub struct CoreUmpOutput {
    port: ffi::Port,
    protocol: UmpProtocol,
}
impl CoreUmpOutput {
    pub fn connect(
        client_name: &str,
        destination_id: i32,
        send_protocol: UmpProtocol,
    ) -> Result<Self, CoreMidiError> {
        Ok(Self {
            port: ffi::Port::output(&name(client_name)?, destination_id)?,
            protocol: send_protocol,
        })
    }
    /// Send one complete framed packet from a control/worker thread. Host ticks
    /// schedule its first word, zero means now. Native failure is returned directly;
    /// framing does not validate MIDI protocol/message semantics.
    pub fn send(
        &mut self,
        timestamp_host_ticks: u64,
        packet: UmpPacket,
    ) -> Result<(), CoreMidiError> {
        self.port.send(self.protocol, timestamp_host_ticks, packet)
    }
}

/// Owned virtual source distributing exact packets to its connected input ports.
/// Creation does not connect hardware or other sources. Applications may subscribe
/// to its visible endpoint; sends distribute to every current subscriber. IDs are
/// assigned by CoreMIDI per lifetime, without persistent-ID or reconnect policy.
/// Native ownership is thread-local; construct/send/drop on its selected worker
/// or control thread. Drop removes the source, without closing subscribers' ports.
pub struct CoreUmpSource {
    port: ffi::Port,
    protocol: UmpProtocol,
    id: i32,
}
impl CoreUmpSource {
    /// Create an explicitly named MIDI 1/2 source; no implicit subscribers.
    pub fn new(
        client_name: &str,
        source_name: &str,
        send_protocol: UmpProtocol,
    ) -> Result<Self, CoreMidiError> {
        let port = ffi::Port::source(&name(client_name)?, &name(source_name)?, send_protocol)?;
        let id = port.virtual_id()?;
        Ok(Self {
            port,
            protocol: send_protocol,
            id,
        })
    }
    /// Exact signed unique ID for explicit source connections during this lifetime.
    pub fn id(&self) -> i32 {
        self.id
    }
    /// Distribute one complete framed packet. Timestamp units are host ticks,
    /// zero means now. Native success does not acknowledge subscriber delivery;
    /// CoreMIDI protocol conversion and consumer routing remain explicit policies.
    pub fn send(
        &mut self,
        timestamp_host_ticks: u64,
        packet: UmpPacket,
    ) -> Result<(), CoreMidiError> {
        self.port.send(self.protocol, timestamp_host_ticks, packet)
    }
}
