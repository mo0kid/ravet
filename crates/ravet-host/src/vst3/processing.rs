//! Exact-paired stopped event metadata and prepared main-f32 processing.
//!
//! Input text/binary data is copied into bounded owned storage before processing.
//! Output views borrow prepared storage: no subsequent render can overwrite it
//! while those views remain live. Native SDK queues never borrow these Rust bytes
//! after the call. CONTROL owners remain main-thread confined; the existing
//! scoped AUDIO capability carries copied bus metadata and the paired table.
//! Actual SDK/DAW acceptance and real-time deadlines require separate evidence.
//!
//! The processing CONTROL companion cannot cross threads:
//!
//! ```compile_fail,E0277
//! use ravet_host::vst3::instances::processing::Provider;
//! fn require_send<T: Send>() {}
//! require_send::<Provider<'static>>();
//! ```
//!
//! ```compile_fail,E0277
//! use ravet_host::vst3::instances::processing::Provider;
//! fn require_sync<T: Sync>() {}
//! require_sync::<Provider<'static>>();
//! ```
use super::{Audio, Block, InputChannel, Instance, OutputChannel, Profile, parameters::Point};
use std::{ffi::c_void, marker::PhantomData, rc::Rc};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RawEvent {
    pub struct_size: u32,
    pub kind: u16,
    pub flags: u16,
    pub bus: u32,
    pub offset: u32,
    pub ppq_position: f64,
    pub channel: i32,
    pub pitch: i32,
    pub note_id: i32,
    pub note_length: i32,
    pub tuning: f32,
    pub velocity: f32,
    pub expression_id: u32,
    pub data_type: u32,
    pub value: f64,
    pub integer_value: u64,
    pub root: i32,
    pub bass: i32,
    pub mask: u32,
    pub controller: u32,
    pub midi_value: u32,
    pub midi_value2: u32,
    pub payload_offset: u32,
    pub payload_bytes: u32,
    pub reserved: u32,
}
/// Exact block-start SDK availability and units. Absent fields are ignored and
/// the native provider supplies zero for them. Requirements are copied requests,
/// rather than a demand to invent unavailable data. Validation precedes DSP.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Context {
    pub struct_size: u32,
    pub state: u32,
    pub sample_rate: f64,
    pub project_time_samples: i64,
    pub system_time: i64,
    pub continuous_time_samples: i64,
    pub project_time_music: f64,
    pub bar_position_music: f64,
    pub cycle_start_music: f64,
    pub cycle_end_music: f64,
    pub tempo: f64,
    pub time_signature_numerator: i32,
    pub time_signature_denominator: i32,
    pub chord_key: u32,
    pub chord_root: u32,
    pub chord_mask: u32,
    pub smpte_offset_subframes: i32,
    pub frames_per_second: u32,
    pub frame_rate_flags: u32,
    pub samples_to_next_clock: i32,
    pub reserved: u32,
}
impl Context {
    pub const PLAYING: u32 = 1 << 1;
    pub const CYCLE_ACTIVE: u32 = 1 << 2;
    pub const RECORDING: u32 = 1 << 3;
    pub const SYSTEM_TIME: u32 = 1 << 8;
    pub const MUSIC: u32 = 1 << 9;
    pub const TEMPO: u32 = 1 << 10;
    pub const BAR: u32 = 1 << 11;
    pub const CYCLE: u32 = 1 << 12;
    pub const TIME_SIGNATURE: u32 = 1 << 13;
    pub const SMPTE: u32 = 1 << 14;
    pub const CLOCK: u32 = 1 << 15;
    pub const CONTINUOUS: u32 = 1 << 17;
    pub const CHORD: u32 = 1 << 18;
    pub fn new(sample_rate: f64, project_time_samples: i64) -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
            sample_rate,
            project_time_samples,
            ..Self::default()
        }
    }
    pub(super) fn admitted(&self, rate: f64) -> bool {
        let s = self.state;
        let known = Self::PLAYING
            | Self::CYCLE_ACTIVE
            | Self::RECORDING
            | Self::SYSTEM_TIME
            | Self::MUSIC
            | Self::TEMPO
            | Self::BAR
            | Self::CYCLE
            | Self::TIME_SIGNATURE
            | Self::SMPTE
            | Self::CLOCK
            | Self::CONTINUOUS
            | Self::CHORD;
        self.struct_size as usize == size_of::<Self>()
            && self.reserved == 0
            && s & !known == 0
            && rate.is_finite()
            && rate > 0.
            && self.sample_rate == rate
            && (s & Self::MUSIC == 0 || self.project_time_music.is_finite())
            && (s & Self::BAR == 0 || self.bar_position_music.is_finite())
            && (s & Self::CYCLE == 0
                || (self.cycle_start_music.is_finite()
                    && self.cycle_end_music.is_finite()
                    && self.cycle_end_music >= self.cycle_start_music))
            && (s & Self::TEMPO == 0 || (self.tempo.is_finite() && self.tempo > 0.))
            && (s & Self::TIME_SIGNATURE == 0
                || (self.time_signature_numerator > 0 && self.time_signature_denominator > 0))
            && (s & Self::CHORD == 0
                || (self.chord_key <= 255 && self.chord_root <= 255 && self.chord_mask <= 0xfff))
            && (s & Self::SMPTE == 0
                || (self.frames_per_second > 0 && self.frame_rate_flags & !3 == 0))
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawReport {
    pub struct_size: u32,
    pub stage: u32,
    pub failure: u32,
    pub index: u32,
    pub flags: u32,
    pub input_parameter_failure: u32,
    pub output_event_failure: u32,
    pub reserved: u32,
    pub native_result: i32,
    pub event_count: u32,
    pub payload_used: u32,
    pub parameter_queue_count: u32,
    pub parameter_point_count: u32,
    pub reserved2: u32,
}
impl RawReport {
    pub(super) fn initial() -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
            stage: 0,
            failure: 0,
            index: u32::MAX,
            flags: 0,
            input_parameter_failure: 0,
            output_event_failure: 0,
            reserved: 0,
            native_result: 0,
            event_count: 0,
            payload_used: 0,
            parameter_queue_count: 0,
            parameter_point_count: 0,
            reserved2: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    raw: RawReport,
}
impl Report {
    /// Copied wire facts, used to re-view an exclusively owned output batch.
    #[cfg_attr(not(feature = "vst3-hosted-audio"), allow(dead_code))]
    pub(super) fn raw(&self) -> RawReport {
        self.raw
    }
    fn admit(raw: RawReport) -> Result<Self, Error> {
        let attempted = raw.flags & 1 != 0;
        let returned = raw.flags & 2 != 0;
        let threw = raw.flags & 4 != 0;
        if raw.struct_size as usize != size_of::<RawReport>()
            || raw.reserved != 0
            || raw.reserved2 != 0
            || raw.flags & !63 != 0
            || (returned && threw)
            || ((returned || threw) && !attempted)
            || (attempted && !(returned || threw))
            || (raw.flags & 16 != 0 && !attempted)
            || (raw.flags & 32 != 0 && (!returned || raw.flags & 12 != 0 || raw.failure != 0))
            || (!returned && raw.native_result != 0)
            || !(raw.stage <= 4
                || (16..=22).contains(&raw.stage)
                || (100..=120).contains(&raw.stage))
            || !(raw.failure <= 4 || (16..=18).contains(&raw.failure))
            || raw.event_count > 2048
            || raw.payload_used > 65536
            || raw.parameter_queue_count > 64
            || raw.parameter_point_count > 2048
        {
            return Err(Error::Abi);
        }
        Ok(Self { raw })
    }
    pub fn stage(&self) -> u32 {
        self.raw.stage
    }
    pub fn failure(&self) -> u32 {
        self.raw.failure
    }
    pub fn index(&self) -> Option<u32> {
        (self.raw.index != u32::MAX).then_some(self.raw.index)
    }
    pub fn returned_status(&self) -> Option<i32> {
        (self.raw.flags & 2 != 0).then_some(self.raw.native_result)
    }
    pub fn threw(&self) -> bool {
        self.raw.flags & 4 != 0
    }
    pub fn invalidated(&self) -> bool {
        self.raw.flags & 8 != 0
    }
    pub fn history_may_have_advanced(&self) -> bool {
        self.raw.flags & 16 != 0
    }
    pub fn published(&self) -> bool {
        self.raw.flags & 32 != 0
    }
    pub fn input_parameter_failure(&self) -> u32 {
        self.raw.input_parameter_failure
    }
    pub fn output_event_failure(&self) -> u32 {
        self.raw.output_event_failure
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Abi,
    Shape,
    Capacity,
    Allocation,
    Lifecycle,
    Poisoned,
    Unassigned,
    Conflict,
    Provider { status: i32, report: Report },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VST3 processing error: {self:?}")
    }
}
impl std::error::Error for Error {}
pub(super) fn accepted(status: i32, raw: RawReport) -> Result<Report, Error> {
    let report = Report::admit(raw)?;
    if status != 0 {
        return Err(Error::Provider { status, report });
    }
    if report.invalidated()
        || report.threw()
        || report.failure() != 0
        || report.returned_status().is_some_and(|r| r != 0)
        || report.input_parameter_failure() != 0
        || report.output_event_failure() != 0
    {
        return Err(Error::Abi);
    }
    Ok(report)
}
// CONTROL success has no AUDIO/list publication facts and uses the exact
// operation stage. Nonzero native statuses retain their independent report.
fn accepted_control(status: i32, raw: RawReport, stage: u32) -> Result<Report, Error> {
    let report = accepted(status, raw)?;
    if raw.stage != stage
        || raw.flags != 0
        || raw.index != u32::MAX
        || raw.event_count != 0
        || raw.payload_used != 0
        || raw.parameter_queue_count != 0
        || raw.parameter_point_count != 0
    {
        return Err(Error::Abi);
    }
    Ok(report)
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawSelection {
    pub struct_size: u32,
    pub input_mask: u32,
    pub output_mask: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawInfo {
    pub struct_size: u32,
    pub input_bus_count: u32,
    pub output_bus_count: u32,
    pub input_mask: u32,
    pub output_mask: u32,
    pub flags: u32,
    pub context_requirements: u32,
    pub reserved: u32,
}
#[repr(C)]
pub struct RawBus {
    pub struct_size: u32,
    pub index: u32,
    pub channels: u32,
    pub bus_type: u32,
    pub flags: u32,
    pub reserved: u32,
    pub name: [u8; 512],
}
#[repr(C)]
pub struct RawOutputs {
    pub struct_size: u32,
    pub reserved: u32,
    pub events: *mut RawEvent,
    pub event_capacity: u32,
    pub event_count: u32,
    pub payload: *mut u8,
    pub payload_capacity: u32,
    pub payload_used: u32,
    pub parameter_ids: *mut u32,
    pub parameter_queue_capacity: u32,
    pub parameter_queue_count: u32,
    pub parameter_points: *mut Point,
    pub parameter_point_capacity: u32,
    pub parameter_point_count: u32,
}
type Refresh =
    unsafe extern "C" fn(*mut c_void, *const RawSelection, *mut RawInfo, *mut RawReport) -> i32;
type BusInfo = unsafe extern "C" fn(*const c_void, u32, u32, *mut RawBus) -> i32;
type Assignment = unsafe extern "C" fn(*const c_void, u32, u32, u32, *mut u32) -> i32;
type Prepare = unsafe extern "C" fn(*mut c_void, *const Profile, *mut RawReport) -> i32;
type Process = unsafe extern "C" fn(
    *mut c_void,
    *const Block,
    *const Point,
    u32,
    *const RawEvent,
    u32,
    *const u8,
    u32,
    *const Context,
    *mut RawOutputs,
    *mut RawReport,
) -> i32;
#[repr(C)]
pub struct RawProvider {
    pub struct_size: u32,
    pub version: u32,
    pub provider: *const super::RawProvider,
    pub refresh: Option<Refresh>,
    pub bus_info: Option<BusInfo>,
    pub midi_assignment: Option<Assignment>,
    pub prepare: Option<Prepare>,
    pub process: Option<Process>,
}
#[derive(Clone, Copy)]
pub struct Provider<'a> {
    pub(super) raw: &'a RawProvider,
    no_send_sync: PhantomData<Rc<()>>,
}
impl<'a> Provider<'a> {
    /// # Safety
    /// The readable exact table and paired ProviderV2 are trusted compliant native
    /// code. Callbacks contain exceptions, preflight every whole borrowed range,
    /// bound every count and preserve copied queue/payload/core lifetimes. Code
    /// stays loaded through all ownership and until process exit after a pin.
    pub unsafe fn from_raw(raw: &'a RawProvider) -> Result<Self, Error> {
        if raw.struct_size as usize != size_of::<RawProvider>()
            || raw.version != 1
            || raw.provider.is_null()
            || raw.refresh.is_none()
            || raw.bus_info.is_none()
            || raw.midi_assignment.is_none()
            || raw.prepare.is_none()
            || raw.process.is_none()
        {
            return Err(Error::Abi);
        }
        Ok(Self {
            raw,
            no_send_sync: PhantomData,
        })
    }
}
/// Input header preserves selected SDK bus, exact sample offset, finite PPQ and
/// live/reserved flags (0xc001). No inferred port or note-identity remapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    pub bus: u32,
    pub offset: u32,
    pub ppq_position: f64,
    pub flags: u16,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub channel: u8,
    pub pitch: u8,
    pub note_id: i32,
    pub tuning: f32,
    pub velocity: f32,
    pub length: i32,
}
/// All nine SDK input kinds. SysEx bytes are opaque counted data, with no MIDI
/// framing parser. Text is copied from UTF-8 into bounded native-endian UTF-16.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind<'a> {
    NoteOn(Note),
    NoteOff(Note),
    SysEx(&'a [u8]),
    PolyPressure {
        channel: u8,
        pitch: u8,
        note_id: i32,
        pressure: f32,
    },
    ExpressionValue {
        expression_id: u32,
        note_id: i32,
        value: f64,
    },
    ExpressionText {
        expression_id: u32,
        note_id: i32,
        text: &'a str,
    },
    Chord {
        root: u8,
        bass: u8,
        mask: u16,
        text: &'a str,
    },
    Scale {
        root: u8,
        mask: u16,
        text: &'a str,
    },
    ExpressionInteger {
        expression_id: u32,
        note_id: i32,
        value: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event<'a> {
    pub header: Header,
    pub kind: Kind<'a>,
}
fn unit(v: f64) -> bool {
    v.is_finite() && (0. ..=1.).contains(&v)
}
fn utf16_valid(bytes: &[u8]) -> bool {
    if !bytes.len().is_multiple_of(2) || bytes.len() > 8192 {
        return false;
    }
    let mut words = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_ne_bytes([b[0], b[1]]));
    while let Some(word) = words.next() {
        if word == 0 {
            return false;
        }
        if (0xd800..=0xdbff).contains(&word) {
            if !words.next().is_some_and(|w| (0xdc00..=0xdfff).contains(&w)) {
                return false;
            }
        } else if (0xdc00..=0xdfff).contains(&word) {
            return false;
        }
    }
    true
}
fn event_admitted(
    r: &RawEvent,
    payload: &[u8],
    frames: u32,
    buses: &[u8; 16],
    output: bool,
) -> bool {
    if r.struct_size as usize != size_of::<RawEvent>()
        || r.reserved != 0
        || frames > 65536
        || r.bus >= 16
        || buses[r.bus as usize] == 0
        || buses[r.bus as usize] > 16
        || r.flags & !0xc001 != 0
        || !r.ppq_position.is_finite()
        || (if frames == 0 {
            r.offset != 0
        } else {
            r.offset >= frames
        })
        || payload.len() > 65536
        || r.payload_offset as usize > payload.len()
        || r.payload_bytes as usize > payload.len() - r.payload_offset as usize
    {
        return false;
    }
    let channel = r.channel >= 0 && r.channel < i32::from(buses[r.bus as usize]);
    let pitch = (0..=127).contains(&r.pitch);
    let note = output || r.note_id >= -1;
    let expression = output || r.note_id >= 0;
    let empty = r.payload_bytes == 0;
    let bytes = &payload[r.payload_offset as usize..(r.payload_offset + r.payload_bytes) as usize];
    match r.kind {
        0 => {
            channel
                && pitch
                && note
                && r.note_length >= 0
                && r.tuning.is_finite()
                && unit(f64::from(r.velocity))
                && empty
        }
        1 => {
            channel && pitch && note && r.tuning.is_finite() && unit(f64::from(r.velocity)) && empty
        }
        2 => r.data_type == 0,
        3 => channel && pitch && note && unit(f64::from(r.velocity)) && empty,
        4 => expression && r.expression_id != u32::MAX && unit(r.value) && empty,
        5 => expression && r.expression_id != u32::MAX && utf16_valid(bytes),
        6 => {
            (0..=127).contains(&r.root)
                && (0..=127).contains(&r.bass)
                && r.mask <= 65535
                && utf16_valid(bytes)
        }
        7 => (0..=127).contains(&r.root) && r.mask <= 65535 && utf16_valid(bytes),
        8 => expression && r.expression_id != u32::MAX && empty,
        65535 => {
            output
                && channel
                && r.controller <= 255
                && r.midi_value <= 127
                && r.midi_value2 <= 127
                && empty
        }
        _ => false,
    }
}
fn prepared<T: Clone>(count: usize, value: T) -> Result<Vec<T>, Error> {
    let mut v = Vec::new();
    v.try_reserve_exact(count).map_err(|_| Error::Allocation)?;
    v.resize(count, value);
    Ok(v)
}
/// Fixed CONTROL-allocated storage; pushes and clear perform no heap work.
/// Complete scalar/text/capacity validation precedes any mutation. Input order
/// is nondecreasing; frame/bus validation additionally uses the prepared profile.
pub struct InputEvents {
    pub(super) records: Vec<RawEvent>,
    payload: Vec<u16>,
    byte_capacity: usize,
    pub(super) count: usize,
    pub(super) used: usize,
    // Native copies add UTF-16 terminators independently of wire byte layout.
    native_used: usize,
}
impl InputEvents {
    pub fn new(event_capacity: usize, payload_capacity: usize) -> Result<Self, Error> {
        if event_capacity > 2048 || payload_capacity > 65536 {
            return Err(Error::Capacity);
        }
        Ok(Self {
            records: prepared(event_capacity, RawEvent::default())?,
            payload: prepared(payload_capacity.div_ceil(2), 0)?,
            byte_capacity: payload_capacity,
            count: 0,
            used: 0,
            native_used: 0,
        })
    }
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn payload_bytes(&self) -> usize {
        self.used
    }
    pub fn clear(&mut self) {
        self.count = 0;
        self.used = 0;
        self.native_used = 0;
    }
    pub(super) fn bytes(&self) -> &[u8] {
        // SAFETY: initialized u16 allocation has twice this byte length; bytes
        // admit every bit pattern. Shared borrow excludes mutation/reallocation.
        unsafe { std::slice::from_raw_parts(self.payload.as_ptr().cast(), self.used) }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: exclusive initialized allocation, bounded actual byte budget.
        unsafe {
            std::slice::from_raw_parts_mut(self.payload.as_mut_ptr().cast(), self.byte_capacity)
        }
    }
    pub fn push(&mut self, event: Event<'_>) -> Result<(), Error> {
        if self.count == self.records.len() {
            return Err(Error::Capacity);
        }
        let h = event.header;
        if h.bus >= 16
            || h.offset >= 65536
            || !h.ppq_position.is_finite()
            || h.flags & !0xc001 != 0
            || (self.count > 0 && h.offset < self.records[self.count - 1].offset)
        {
            return Err(Error::Shape);
        }
        let mut r = RawEvent {
            struct_size: size_of::<RawEvent>() as u32,
            bus: h.bus,
            offset: h.offset,
            ppq_position: h.ppq_position,
            flags: h.flags,
            ..RawEvent::default()
        };
        let mut text = None;
        let mut binary = None;
        let put_note = |r: &mut RawEvent, n: Note| {
            r.channel = i32::from(n.channel);
            r.pitch = i32::from(n.pitch);
            r.note_id = n.note_id;
            r.tuning = n.tuning;
            r.velocity = n.velocity;
            r.note_length = n.length;
        };
        match event.kind {
            Kind::NoteOn(n) => put_note(&mut r, n),
            Kind::NoteOff(n) => {
                r.kind = 1;
                put_note(&mut r, n);
            }
            Kind::SysEx(bytes) => {
                r.kind = 2;
                binary = Some(bytes);
            }
            Kind::PolyPressure {
                channel,
                pitch,
                note_id,
                pressure,
            } => {
                r.kind = 3;
                r.channel = i32::from(channel);
                r.pitch = i32::from(pitch);
                r.note_id = note_id;
                r.velocity = pressure;
            }
            Kind::ExpressionValue {
                expression_id,
                note_id,
                value,
            } => {
                r.kind = 4;
                r.expression_id = expression_id;
                r.note_id = note_id;
                r.value = value;
            }
            Kind::ExpressionText {
                expression_id,
                note_id,
                text: value,
            } => {
                r.kind = 5;
                r.expression_id = expression_id;
                r.note_id = note_id;
                text = Some(value);
            }
            Kind::Chord {
                root,
                bass,
                mask,
                text: value,
            } => {
                r.kind = 6;
                r.root = i32::from(root);
                r.bass = i32::from(bass);
                r.mask = u32::from(mask);
                text = Some(value);
            }
            Kind::Scale {
                root,
                mask,
                text: value,
            } => {
                r.kind = 7;
                r.root = i32::from(root);
                r.mask = u32::from(mask);
                text = Some(value);
            }
            Kind::ExpressionInteger {
                expression_id,
                note_id,
                value,
            } => {
                r.kind = 8;
                r.expression_id = expression_id;
                r.note_id = note_id;
                r.integer_value = value;
            }
        }
        // Scalars first, with empty text accepted provisionally. This avoids
        // copying payload before discovering an invalid note/value/header.
        let buses = [16; 16];
        if !event_admitted(&r, &[], 65536, &buses, false) {
            return Err(Error::Shape);
        }
        let start = if text.is_some() {
            (self.used + 1) & !1
        } else {
            self.used
        };
        let length = if let Some(t) = text {
            let n = t.encode_utf16().count();
            if n > 4096 || t.contains('\0') {
                return Err(Error::Shape);
            }
            n * 2
        } else {
            binary.map_or(0, <[u8]>::len)
        };
        if start > self.byte_capacity || length > self.byte_capacity - start {
            return Err(Error::Capacity);
        }
        // Match native storedPayloadEnd before any mutation. Wire capacity and
        // layout stay independent; each native text copy also needs a terminator.
        let native_start = if text.is_some() {
            (self.native_used + 1) & !1
        } else {
            self.native_used
        };
        let terminator = if text.is_some() { 2 } else { 0 };
        if native_start > 65536
            || terminator > 65536 - native_start
            || length > 65536 - native_start - terminator
        {
            return Err(Error::Capacity);
        }
        let native_end = native_start + length + terminator;
        if start > self.used {
            let used = self.used;
            self.bytes_mut()[used] = 0;
        }
        if let Some(t) = text {
            for (i, w) in t.encode_utf16().enumerate() {
                self.payload[start / 2 + i] = w;
            }
        } else if let Some(bytes) = binary {
            self.bytes_mut()[start..start + length].copy_from_slice(bytes);
        }
        r.payload_offset = start as u32;
        r.payload_bytes = length as u32;
        self.records[self.count] = r;
        self.count += 1;
        self.used = start + length;
        self.native_used = native_end;
        Ok(())
    }
    pub(super) fn admitted(&self, frames: u32, buses: &[u8; 16]) -> bool {
        self.records[..self.count]
            .iter()
            .all(|r| event_admitted(r, self.bytes(), frames, buses, false))
    }
    /// Copy one already admitted record and its payload from `source` into this
    /// prepared storage, with the same order, wire and native budget checks as
    /// [`Self::push`]. No heap work; a refusal leaves this storage unchanged.
    // Used only by the hosted Graph-bank translator (vst3-hosted-audio).
    #[cfg(feature = "vst3-hosted-audio")]
    pub(super) fn push_copy(&mut self, source: &InputEvents, index: usize) -> Result<(), Error> {
        let mut r = *source.records[..source.count]
            .get(index)
            .ok_or(Error::Shape)?;
        if self.count == self.records.len() {
            return Err(Error::Capacity);
        }
        if self.count > 0 && r.offset < self.records[self.count - 1].offset {
            return Err(Error::Shape);
        }
        let text = matches!(r.kind, 5..=7);
        let length = r.payload_bytes as usize;
        let from = r.payload_offset as usize;
        let bytes = source
            .bytes()
            .get(from..from + length)
            .ok_or(Error::Shape)?;
        let start = if text {
            (self.used + 1) & !1
        } else {
            self.used
        };
        if start > self.byte_capacity || length > self.byte_capacity - start {
            return Err(Error::Capacity);
        }
        let native_start = if text {
            (self.native_used + 1) & !1
        } else {
            self.native_used
        };
        let terminator = if text { 2 } else { 0 };
        if native_start > 65536
            || terminator > 65536 - native_start
            || length > 65536 - native_start - terminator
        {
            return Err(Error::Capacity);
        }
        if start > self.used {
            let used = self.used;
            self.bytes_mut()[used] = 0;
        }
        self.bytes_mut()[start..start + length].copy_from_slice(bytes);
        r.payload_offset = start as u32;
        self.records[self.count] = r;
        self.count += 1;
        self.used = start + length;
        self.native_used = native_start + length + terminator;
        Ok(())
    }
}
/// CONTROL-allocated output backing. A successful view exclusively borrows it;
/// count limits and every record/payload/queue are admitted before a view exists.
/// Insufficient output capacity is discovered after native DSP: audio is silenced
/// and the child must be retired/recreated. Use [`Self::maximum`] unless a consumer
/// has a separately established bound on every possible plug-in output batch.
pub struct OutputStorage {
    events: Vec<RawEvent>,
    payload: Vec<u16>,
    byte_capacity: usize,
    ids: Vec<u32>,
    points: Vec<Point>,
}
impl OutputStorage {
    /// Allocate the complete native output limits on CONTROL: 2048 events,
    /// 65536 payload bytes, 64 queues and 2048 points (about 320 KiB).
    /// This prevents capacity faults within those bounded protocol limits.
    pub fn maximum() -> Result<Self, Error> {
        Self::new(2048, 65536, 64, 2048)
    }
    pub fn new(events: usize, payload: usize, queues: usize, points: usize) -> Result<Self, Error> {
        if events > 2048 || payload > 65536 || queues > 64 || points > 2048 {
            return Err(Error::Capacity);
        }
        Ok(Self {
            events: prepared(events, RawEvent::default())?,
            payload: prepared(payload.div_ceil(2), 0)?,
            byte_capacity: payload,
            ids: prepared(queues, 0)?,
            points: prepared(
                points,
                Point {
                    id: 0,
                    offset: 0,
                    value: 0.,
                },
            )?,
        })
    }
    pub(super) fn wire(&mut self) -> RawOutputs {
        RawOutputs {
            struct_size: size_of::<RawOutputs>() as u32,
            reserved: 0,
            events: self.events.as_mut_ptr(),
            event_capacity: self.events.len() as u32,
            event_count: 0,
            payload: self.payload.as_mut_ptr().cast(),
            payload_capacity: self.byte_capacity as u32,
            payload_used: 0,
            parameter_ids: self.ids.as_mut_ptr(),
            parameter_queue_capacity: self.ids.len() as u32,
            parameter_queue_count: 0,
            parameter_points: self.points.as_mut_ptr(),
            parameter_point_capacity: self.points.len() as u32,
            parameter_point_count: 0,
        }
    }
    pub(super) fn view(
        &self,
        wire: &RawOutputs,
        frames: u32,
        buses: &[u8; 16],
        report: Report,
    ) -> Result<Output<'_>, Error> {
        if wire.struct_size as usize != size_of::<RawOutputs>()
            || wire.reserved != 0
            || wire.events != self.events.as_ptr().cast_mut()
            || wire.payload != self.payload.as_ptr().cast::<u8>().cast_mut()
            || wire.parameter_ids != self.ids.as_ptr().cast_mut()
            || wire.parameter_points != self.points.as_ptr().cast_mut()
            || wire.event_capacity as usize != self.events.len()
            || wire.payload_capacity as usize != self.byte_capacity
            || wire.parameter_queue_capacity as usize != self.ids.len()
            || wire.parameter_point_capacity as usize != self.points.len()
            || wire.event_count > wire.event_capacity
            || wire.payload_used > wire.payload_capacity
            || wire.parameter_queue_count > wire.parameter_queue_capacity
            || wire.parameter_point_count > wire.parameter_point_capacity
            || !report.published()
            || report.raw.event_count != wire.event_count
            || report.raw.payload_used != wire.payload_used
            || report.raw.parameter_queue_count != wire.parameter_queue_count
            || report.raw.parameter_point_count != wire.parameter_point_count
        {
            return Err(Error::Abi);
        }
        // SAFETY: checked prefix within initialized aligned u16 storage; shared
        // view holds the original mutable storage borrow until it ends.
        let payload = unsafe {
            std::slice::from_raw_parts(
                self.payload.as_ptr().cast::<u8>(),
                wire.payload_used as usize,
            )
        };
        let events = &self.events[..wire.event_count as usize];
        let ids = &self.ids[..wire.parameter_queue_count as usize];
        let points = &self.points[..wire.parameter_point_count as usize];
        for (i, e) in events.iter().enumerate() {
            if !event_admitted(e, payload, frames, buses, true)
                || (i > 0 && e.offset < events[i - 1].offset)
            {
                return Err(Error::Abi);
            }
            if matches!(e.kind, 5..=7) {
                let end = e.payload_offset as usize + e.payload_bytes as usize;
                if e.payload_offset % 2 != 0
                    || end + 2 > payload.len()
                    || payload[end..end + 2] != [0, 0]
                {
                    return Err(Error::Abi);
                }
            }
        }
        let mut last = [None; 64];
        for (i, id) in ids.iter().enumerate() {
            if *id == u32::MAX || ids[..i].contains(id) {
                return Err(Error::Abi);
            }
        }
        for p in points {
            let index = ids.iter().position(|&id| id == p.id).ok_or(Error::Abi)?;
            if !unit(p.value)
                || (if frames == 0 {
                    p.offset != 0
                } else {
                    p.offset >= frames
                })
                || last[index].is_some_and(|offset| p.offset <= offset)
            {
                return Err(Error::Abi);
            }
            last[index] = Some(p.offset);
        }
        Ok(Output {
            events,
            payload,
            ids,
            points,
            report,
        })
    }
}
/// Borrowed complete output. Parameter IDs retain queue creation order, including
/// empty queues. Points retain insertion order; ordering is strict per ID.
pub struct Output<'a> {
    events: &'a [RawEvent],
    payload: &'a [u8],
    ids: &'a [u32],
    points: &'a [Point],
    report: Report,
}
impl Output<'_> {
    pub fn report(&self) -> Report {
        self.report
    }
    pub fn events(&self) -> impl ExactSizeIterator<Item = OutputEvent<'_>> {
        self.events.iter().map(|raw| OutputEvent {
            raw,
            payload: self.payload,
        })
    }
    pub fn parameter_ids(&self) -> &[u32] {
        self.ids
    }
    pub fn parameter_points(&self) -> &[Point] {
        self.points
    }
}
/// Validated owned-wire event view. All scalar fields are copied values; text
/// decodes lazily without allocation and binary payload is a borrowed prefix.
pub struct OutputEvent<'a> {
    raw: &'a RawEvent,
    payload: &'a [u8],
}
impl<'a> OutputEvent<'a> {
    pub fn record(&self) -> &RawEvent {
        self.raw
    }
    pub fn payload(&self) -> &'a [u8] {
        &self.payload[self.raw.payload_offset as usize
            ..(self.raw.payload_offset + self.raw.payload_bytes) as usize]
    }
    pub fn text(&self) -> Option<impl Iterator<Item = char> + 'a> {
        matches!(self.raw.kind, 5..=7).then(|| {
            std::char::decode_utf16(
                self.payload()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u16::from_ne_bytes([b[0], b[1]])),
            )
            .map(|c| c.expect("validated UTF16"))
        })
    }
}
#[derive(Clone, Copy, Default)]
pub(super) struct EventBuses {
    pub(super) input: [u8; 16],
    pub(super) output: [u8; 16],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventBus {
    pub index: u32,
    pub channels: u8,
    pub auxiliary: bool,
    pub default_active: bool,
    pub selected: bool,
    pub name: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mapping {
    bus: u32,
    channel: u32,
    controller: u32,
    id: u32,
}
/// Owned stopped event/mapping snapshot. Later failed refresh cannot alter it.
/// Optional interfaces are represented independently of the copied assignments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalogue {
    inputs: Vec<EventBus>,
    outputs: Vec<EventBus>,
    mapping: bool,
    context_present: bool,
    context_requirements: u32,
    assignments: Vec<Mapping>,
}
impl Catalogue {
    pub fn inputs(&self) -> &[EventBus] {
        &self.inputs
    }
    pub fn outputs(&self) -> &[EventBus] {
        &self.outputs
    }
    pub fn has_midi_mapping(&self) -> bool {
        self.mapping
    }
    pub fn context_requirements(&self) -> Option<u32> {
        self.context_present.then_some(self.context_requirements)
    }
    /// Convert supported channel CC, pressure and pitch bend through actual
    /// copied IMidiMapping assignments. Notes/program/system messages require
    /// their separate routes; no legacy output event is invented for input.
    pub fn midi_point(&self, bus: u32, offset: u32, message: &[u8]) -> Result<Point, Error> {
        let Some(&status) = message.first() else {
            return Err(Error::Shape);
        };
        let kind = status & 0xf0;
        let channel = u32::from(status & 15);
        if !matches!(kind, 0xb0 | 0xd0 | 0xe0)
            || message.len() != (if kind == 0xd0 { 2 } else { 3 })
            || message[1..].iter().any(|&b| b > 127)
        {
            return Err(Error::Shape);
        }
        let (controller, value) = match kind {
            0xb0 => (u32::from(message[1]), f64::from(message[2]) / 127.),
            0xd0 => (128, f64::from(message[1]) / 127.),
            _ => (
                129,
                f64::from(u16::from(message[1]) | u16::from(message[2]) << 7) / 16383.,
            ),
        };
        let id = self
            .assignments
            .iter()
            .find(|m| m.bus == bus && m.channel == channel && m.controller == controller)
            .map(|m| m.id)
            .ok_or(Error::Unassigned)?;
        Ok(Point { id, offset, value })
    }
}
/// Prepared parameter staging for mixed explicit/mapped points. Same-ID equal
/// offsets are an explicit conflict; points are never silently merged/sorted.
///
/// Admission is bounded per point. Offsets are non-decreasing, so the conflict
/// check scans only the trailing equal-offset run. That run holds distinct IDs,
/// so it has at most 64 points. The distinct-ID set (at most 64 entries) is
/// kept incrementally. A full 2048-point block therefore costs at most about
/// 2048 x 128 comparisons, not a quadratic rescan.
pub struct InputPoints {
    points: Vec<Point>,
    count: usize,
    ids: [u32; 64],
    distinct: usize,
}
impl InputPoints {
    pub fn new(capacity: usize) -> Result<Self, Error> {
        if capacity > 2048 {
            return Err(Error::Capacity);
        }
        Ok(Self {
            points: prepared(
                capacity,
                Point {
                    id: 0,
                    offset: 0,
                    value: 0.,
                },
            )?,
            count: 0,
            ids: [0; 64],
            distinct: 0,
        })
    }
    pub fn points(&self) -> &[Point] {
        &self.points[..self.count]
    }
    pub fn clear(&mut self) {
        self.count = 0;
        self.distinct = 0;
    }
    /// Index of the admitted point with `id` at `offset`, if any. Only the
    /// trailing equal-offset run (at most 64 points) is scanned.
    pub(crate) fn same_offset(&self, id: u32, offset: u32) -> Option<usize> {
        self.points()
            .iter()
            .enumerate()
            .rev()
            .take_while(|(_, p)| p.offset == offset)
            .find(|(_, p)| p.id == id)
            .map(|(i, _)| i)
    }
    /// Replace the value of admitted point `index` (a last-wins coalesce of a
    /// same-ID, same-offset value). The value must be normalized.
    // Used only by the hosted Graph-bank translator (vst3-hosted-audio).
    #[cfg(feature = "vst3-hosted-audio")]
    pub(crate) fn replace_value(&mut self, index: usize, value: f64) -> Result<(), Error> {
        if index >= self.count || !unit(value) {
            return Err(Error::Shape);
        }
        self.points[index].value = value;
        Ok(())
    }
    pub fn push(&mut self, point: Point) -> Result<(), Error> {
        if self.count == self.points.len() {
            return Err(Error::Capacity);
        }
        if point.id == u32::MAX
            || !unit(point.value)
            || point.offset >= 65536
            || (self.count > 0 && point.offset < self.points[self.count - 1].offset)
        {
            return Err(Error::Shape);
        }
        if self.same_offset(point.id, point.offset).is_some() {
            return Err(Error::Conflict);
        }
        let known = self.ids[..self.distinct].contains(&point.id);
        if !known {
            if self.distinct == self.ids.len() {
                return Err(Error::Capacity);
            }
            self.ids[self.distinct] = point.id;
            self.distinct += 1;
        }
        self.points[self.count] = point;
        self.count += 1;
        Ok(())
    }
    pub fn push_midi(
        &mut self,
        catalogue: &Catalogue,
        bus: u32,
        offset: u32,
        message: &[u8],
    ) -> Result<(), Error> {
        self.push(catalogue.midi_point(bus, offset, message)?)
    }
}
impl<'m, 'a> Instance<'m, 'a> {
    pub(super) fn processing_admission(&self, provider: Provider<'_>) -> Result<(), Error> {
        if !std::ptr::eq(provider.raw.provider, self.library.provider.raw) {
            return Err(Error::Abi);
        }
        if self.poisoned || self.state_fault {
            return Err(Error::Poisoned);
        }
        if self.running {
            return Err(Error::Lifecycle);
        }
        Ok(())
    }
    /// Explicit stopped selection and copied metadata. Call refresh_parameters
    /// first, including for a controllerless component's empty catalogue. This
    /// neither activates buses nor invents a device/controller routing policy.
    pub fn refresh_processing(
        &mut self,
        provider: Provider<'a>,
        input_mask: u16,
        output_mask: u16,
    ) -> Result<Catalogue, Error> {
        self.processing_admission(provider)?;
        self.processing_provider = None;
        self.processing_metadata = None;
        self.hosted = Default::default();
        let selection = RawSelection {
            struct_size: size_of::<RawSelection>() as u32,
            input_mask: u32::from(input_mask),
            output_mask: u32::from(output_mask),
            reserved: 0,
        };
        let mut info = RawInfo {
            struct_size: size_of::<RawInfo>() as u32,
            input_bus_count: 0,
            output_bus_count: 0,
            input_mask: 0,
            output_mask: 0,
            flags: 0,
            context_requirements: 0,
            reserved: 0,
        };
        let mut report = RawReport::initial();
        // SAFETY: exact paired stopped child, immutable selection and disjoint
        // owned records; native refresh copies all metadata without activation.
        let status = unsafe {
            provider.raw.refresh.unwrap()(
                self.handle.unwrap().as_ptr(),
                &selection,
                &mut info,
                &mut report,
            )
        };
        accepted_control(status, report, 1)?;
        if info.struct_size as usize != size_of::<RawInfo>()
            || info.input_bus_count > 16
            || info.output_bus_count > 16
            || info.input_mask != u32::from(input_mask)
            || info.output_mask != u32::from(output_mask)
            || info.reserved != 0
            || info.flags & !3 != 0
            || info.context_requirements & !2047 != 0
            || (info.flags & 2 == 0 && info.context_requirements != 0)
            || (info.input_mask >> info.input_bus_count) != 0
            || (info.output_mask >> info.output_bus_count) != 0
        {
            return Err(Error::Abi);
        }
        let mut catalogue = Catalogue {
            inputs: Vec::new(),
            outputs: Vec::new(),
            mapping: info.flags & 1 != 0,
            context_present: info.flags & 2 != 0,
            context_requirements: info.context_requirements,
            assignments: Vec::new(),
        };
        let mut buses = EventBuses::default();
        for output in [false, true] {
            let count = if output {
                info.output_bus_count
            } else {
                info.input_bus_count
            };
            let mask = if output {
                info.output_mask
            } else {
                info.input_mask
            };
            let list = if output {
                &mut catalogue.outputs
            } else {
                &mut catalogue.inputs
            };
            list.try_reserve_exact(count as usize)
                .map_err(|_| Error::Allocation)?;
            for i in 0..count {
                let mut b = RawBus {
                    struct_size: size_of::<RawBus>() as u32,
                    index: 0,
                    channels: 0,
                    bus_type: 0,
                    flags: 0,
                    reserved: 0,
                    name: [0; 512],
                };
                // SAFETY: paired stopped child, copied bounded record on stack.
                let status = unsafe {
                    provider.raw.bus_info.unwrap()(
                        self.handle.unwrap().as_ptr(),
                        u32::from(output),
                        i,
                        &mut b,
                    )
                };
                if status != 0 {
                    return Err(Error::Provider {
                        status,
                        report: Report::admit(RawReport::initial())?,
                    });
                }
                if b.struct_size as usize != size_of::<RawBus>()
                    || b.index != i
                    || !(1..=16).contains(&b.channels)
                    || b.bus_type > 1
                    || b.flags & !1 != 0
                    || b.reserved != 0
                {
                    return Err(Error::Abi);
                }
                let end = b.name.iter().position(|&c| c == 0).ok_or(Error::Abi)?;
                if end == 0 {
                    return Err(Error::Abi);
                }
                let name = std::str::from_utf8(&b.name[..end])
                    .map_err(|_| Error::Abi)?
                    .to_owned();
                let selected = mask & (1 << i) != 0;
                if selected {
                    if output {
                        buses.output[i as usize] = b.channels as u8;
                    } else {
                        buses.input[i as usize] = b.channels as u8;
                    }
                }
                list.push(EventBus {
                    index: i,
                    channels: b.channels as u8,
                    auxiliary: b.bus_type == 1,
                    default_active: b.flags & 1 != 0,
                    selected,
                    name,
                });
            }
        }
        if catalogue.mapping {
            let capacity = catalogue
                .inputs
                .iter()
                .filter(|b| b.selected)
                .map(|b| usize::from(b.channels) * 130)
                .sum();
            catalogue
                .assignments
                .try_reserve_exact(capacity)
                .map_err(|_| Error::Allocation)?;
            for b in &catalogue.inputs {
                if b.selected {
                    for channel in 0..u32::from(b.channels) {
                        for controller in 0..130 {
                            let mut id = u32::MAX;
                            // SAFETY: stopped exact table and copied selected bus/channel;
                            // reads native owned mapping, no MIDI controller call on AUDIO.
                            let status = unsafe {
                                provider.raw.midi_assignment.unwrap()(
                                    self.handle.unwrap().as_ptr(),
                                    b.index,
                                    channel,
                                    controller,
                                    &mut id,
                                )
                            };
                            if status != 0 {
                                return Err(Error::Provider {
                                    status,
                                    report: Report::admit(RawReport::initial())?,
                                });
                            }
                            if id != u32::MAX {
                                catalogue.assignments.push(Mapping {
                                    bus: b.index,
                                    channel,
                                    controller,
                                    id,
                                });
                            }
                        }
                    }
                }
            }
        }
        self.processing_metadata = Some((provider.raw, buses));
        Ok(catalogue)
    }
    /// Prepare both native queue families and explicitly selected event buses.
    /// Audio preparation replaces prior parameter-only/main preparation. A late
    /// native processing fault requires stopped instance retirement/recreation.
    pub fn prepare_processing(
        &mut self,
        provider: Provider<'a>,
        profile: Profile,
    ) -> Result<(), Error> {
        self.processing_admission(provider)?;
        if !self
            .processing_metadata
            .is_some_and(|(raw, _)| std::ptr::eq(raw, provider.raw))
        {
            return Err(Error::Lifecycle);
        }
        if profile.struct_size as usize != size_of::<Profile>()
            || !profile.sample_rate.is_finite()
            || profile.sample_rate <= 0.
            || profile.sample_rate > 768000.
            || !(1..=65536).contains(&profile.maximum_frames)
            || !(1..=2).contains(&profile.output_channels)
            || (profile.input_channels != 0 && profile.input_channels != profile.output_channels)
        {
            return Err(Error::Shape);
        }
        self.processing_provider = None;
        self.parameter_provider = None;
        self.profile = None;
        self.hosted = Default::default();
        let mut report = RawReport::initial();
        // SAFETY: paired exclusive stopped child, validated profile and disjoint
        // diagnostic; native owns allocation and exact core pinning on CONTROL.
        let status = unsafe {
            provider.raw.prepare.unwrap()(self.handle.unwrap().as_ptr(), &profile, &mut report)
        };
        accepted_control(status, report, 2)?;
        self.profile = Some(profile);
        self.processing_provider = Some(provider.raw);
        Ok(())
    }
}
/// One immutable prepared input view. Event payload storage and parameter slices
/// stay borrowed for the complete call; context remains exact block-start data.
pub struct Input<'a> {
    pub points: &'a [Point],
    pub events: &'a InputEvents,
    pub context: Option<&'a Context>,
}
impl Audio<'_, '_> {
    /// Process complete events/parameters/context and return one verified output
    /// batch. All Rust shape/value/capacity checks precede native calls and output
    /// writes. Native additionally preflights known IDs, complete aliases and both
    /// retained queue reservations; late faults silence audio and publish no data.
    /// Output lifetime exclusively borrows its storage, independently of Audio.
    /// No method allocation/deallocation occurs after preparation.
    ///
    /// ```compile_fail,E0499
    /// use ravet_host::vst3::instances::{Audio, processing::{Input, InputEvents, OutputStorage}};
    /// fn overwrite(audio: &mut Audio<'_, '_>, events: &InputEvents, storage: &mut OutputStorage) {
    ///     let first = audio.process_events(&[], &mut [], 0, Input {points: &[], events, context: None}, storage).unwrap();
    ///     let next = audio.process_events(&[], &mut [], 0, Input {points: &[], events, context: None}, storage).unwrap();
    ///     drop(first); drop(next);
    /// }
    /// ```
    pub fn process_events<'o>(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        frames: u32,
        input: Input<'_>,
        storage: &'o mut OutputStorage,
    ) -> Result<Output<'o>, Error> {
        let provider = self.processing.ok_or(Error::Lifecycle)?;
        if self.hosted_mode() {
            return Err(Error::Lifecycle);
        }
        if inputs.len() != self.profile.input_channels as usize
            || outputs.len() != self.profile.output_channels as usize
            || inputs.len() > 2
            || outputs.len() > 2
            || frames > self.profile.maximum_frames
            || !super::parameters::points_admitted(input.points, frames)
            || !input.events.admitted(frames, &self.event_buses.input)
            || input
                .context
                .is_some_and(|c| !c.admitted(self.profile.sample_rate))
        {
            return Err(Error::Shape);
        }
        let mut native_input = [InputChannel {
            samples: std::ptr::null(),
            capacity: 0,
            reserved: 0,
        }; 2];
        let mut native_output = [OutputChannel {
            samples: std::ptr::null_mut(),
            capacity: 0,
            reserved: 0,
        }; 2];
        for (i, s) in inputs.iter().enumerate() {
            if s.len() < frames as usize {
                return Err(Error::Shape);
            }
            native_input[i] = InputChannel {
                samples: s.as_ptr(),
                capacity: u32::try_from(s.len()).map_err(|_| Error::Shape)?,
                reserved: 0,
            };
        }
        for (i, s) in outputs.iter_mut().enumerate() {
            if s.len() < frames as usize {
                return Err(Error::Shape);
            }
            native_output[i] = OutputChannel {
                samples: s.as_mut_ptr(),
                capacity: u32::try_from(s.len()).map_err(|_| Error::Shape)?,
                reserved: 0,
            };
        }
        let block = Block {
            struct_size: size_of::<Block>() as u32,
            frames,
            input_channels: inputs.len() as u32,
            output_channels: outputs.len() as u32,
            inputs: if inputs.is_empty() {
                std::ptr::null()
            } else {
                native_input.as_ptr()
            },
            outputs: native_output.as_ptr(),
        };
        let mut wire = storage.wire();
        let mut raw = RawReport::initial();
        // SAFETY: exact scoped exclusive AUDIO table; caller-owned initialized
        // disjoint slices/storage, bounded immutable records/payload/context.
        // All native counts are independently rechecked before indexing/viewing.
        let status = unsafe {
            provider.process.unwrap()(
                self.handle.as_ptr(),
                &block,
                input.points.as_ptr(),
                input.points.len() as u32,
                input.events.records.as_ptr(),
                input.events.count as u32,
                input.events.bytes().as_ptr(),
                input.events.used as u32,
                input.context.map_or(std::ptr::null(), |c| c),
                &mut wire,
                &mut raw,
            )
        };
        let result = accepted(status, raw).and_then(|report| {
            if report.stage() != 3
                || report.returned_status() != Some(0)
                || !report.history_may_have_advanced()
            {
                return Err(Error::Abi);
            }
            storage.view(&wire, frames, &self.event_buses.output, report)
        });
        // An invalid wire reply cannot establish whether native DSP ran. Retire
        // the local processing capability even if its flags omit ATTEMPTED.
        // A valid pre-dispatch refusal leaves the capability available.
        if matches!(result, Err(Error::Abi)) || (result.is_err() && raw.flags & 1 != 0) {
            self.processing = None;
            self.parameters = None;
        }
        result
    }
}
#[cfg(test)]
#[path = "processing_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "processing_followup_tests.rs"]
mod followup_tests;
