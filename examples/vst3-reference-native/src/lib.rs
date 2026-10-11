//! Portable references behind the RAVET reference VST3 modules.
//!
//! These processors exist so that a VST3 host implementation can be tested
//! against real loaded modules whose output is analytically predictable:
//!
//! - [`KeyedGain`] is a stateless Foundation [`Processor`] with a stereo main
//!   bus and an optional mono or stereo input-only key bus. Each output sample
//!   is `main * key`, where `key` is the mono key sample, the stereo mean
//!   `(left + right) * 0.5`, or exactly one when no key bus is prepared (an
//!   inactive VST3 auxiliary bus).
//! - [`EventProbe`] renders a deterministic impulse code for every typed probe
//!   record: the left channel receives `code + 1` and the right channel the
//!   record's 20-bit FNV-1a hash scaled by `2^-20`, both at the record's exact
//!   sample offset, and summed when records share an offset. A running 20-bit
//!   digest covers every record, including those in zero-frame blocks.
//!
//! Records that share a sample offset are summed, so the left-channel code sum
//! does not uniquely identify the records at that offset (for example a note-on
//! plus a note-off, `1 + 2`, reads like one data event, `3`). Decoders must
//! either know the candidate records or use distinct offsets.
//!
//! The C ABI in `include/RavetVst3Reference.h` is consumed by the external-SDK
//! VST3 wrappers in `examples/vst3-reference`. Preparation allocates. Processing
//! and reset are designed to perform no heap operations. This is measured for
//! the Rust ABI entry points with `allocation-counter` in this crate's unit
//! tests, and inside the loaded bundles through the test-only
//! `allocation-probe` feature (see `docs/vst3-reference-modules.md`). These are
//! validation references, not production dynamics or instrument designs, and
//! they carry no state, editor or real-time deadline claim.
use ravet_foundation::{
    AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor,
    Sample,
};

mod abi;
#[cfg(all(feature = "allocation-probe", not(test)))]
mod allocation_probe;

/// Arithmetic needed by the references, implemented for native f32 and f64.
pub trait ReferenceSample:
    Sample + core::ops::Add<Output = Self> + core::ops::Mul<Output = Self> + PartialEq
{
    const ONE: Self;
    const HALF: Self;
    /// `2^-20`, the exact scale applied to 20-bit record hashes.
    const HASH_SCALE: Self;
    /// Exact conversion for values below `2^24`.
    fn exact(value: u32) -> Self;
}
impl ReferenceSample for f32 {
    const ONE: Self = 1.;
    const HALF: Self = 0.5;
    const HASH_SCALE: Self = 1. / 1_048_576.;
    fn exact(value: u32) -> Self {
        value as f32
    }
}
impl ReferenceSample for f64 {
    const ONE: Self = 1.;
    const HALF: Self = 0.5;
    const HASH_SCALE: Self = 1. / 1_048_576.;
    fn exact(value: u32) -> Self {
        f64::from(value)
    }
}

/// Stateless stereo keyed gain driven by an optional input-only key bus.
///
/// Prepare with [`AudioLayout::STEREO`] and either no auxiliary bus or exactly
/// one input-only mono/stereo auxiliary bus. Output events are not supported.
#[derive(Debug, Default)]
pub struct KeyedGain {
    config: Option<ProcessConfig>,
    key_channels: usize,
}
impl KeyedGain {
    /// Unprepared keyed gain.
    pub fn new() -> Self {
        Self::default()
    }
    /// Prepared key channel count: 0 (unity key), 1 or 2.
    pub fn key_channels(&self) -> usize {
        self.key_channels
    }
}
impl<S: ReferenceSample> Processor<S> for KeyedGain {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        let key_channels = match config.auxiliary_buses() {
            [] => 0,
            [bus] if bus.output_channels() == 0 && (1..=2).contains(&bus.input_channels()) => {
                bus.input_channels()
            }
            _ => return Err(PrepareError::UnsupportedLayout),
        };
        if config.layout() != AudioLayout::STEREO || config.maximum_output_events() != 0 {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.key_channels = key_channels;
        self.config = Some(config);
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.config = None;
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, S>) -> ProcessStatus {
        let Some(config) = self.config else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::NotPrepared);
        };
        if let Err(error) = config.validate_block(block) {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let frames = block.audio.frames();
        for frame in 0..frames {
            let key = match block.auxiliary.as_deref() {
                Some([bus]) if self.key_channels == 2 => {
                    (bus.input(0).expect("validated key")[frame]
                        + bus.input(1).expect("validated key")[frame])
                        * S::HALF
                }
                Some([bus]) => bus.input(0).expect("validated key")[frame],
                _ => S::ONE,
            };
            for channel in 0..2 {
                let value = block.audio.input(channel).expect("validated main")[frame] * key;
                block.audio.output_mut(channel).expect("validated main")[frame] = value;
            }
        }
        ProcessStatus::Continue
    }
}

/// Number of hash bits rendered by [`EventProbe`].
pub const HASH_BITS: u32 = 20;
/// Maximum records per [`EventProbe::render`] call (VST3: 2048 events plus
/// 2048 parameter points).
pub const MAXIMUM_PROBE_RECORDS: usize = 4096;
/// Code used for parameter-point records; codes 0..=8 are the SDK input event
/// types.
pub const PARAMETER_CODE: u16 = 16;

/// Common record header. `offset` positions the rendered impulse and is not
/// hashed; bus, flags and PPQ position are hashed. Parameter points use zero
/// bus, flags and PPQ position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeHeader {
    pub offset: u32,
    pub bus: i32,
    pub flags: u16,
    pub ppq_position: f64,
}

/// Typed record payload: the nine VST3 SDK input event kinds and a parameter
/// point. Text is native UTF-16 code units without a terminator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProbeEvent<'a> {
    NoteOn {
        channel: i32,
        pitch: i32,
        note_id: i32,
        length: i32,
        tuning: f32,
        velocity: f32,
    },
    NoteOff {
        channel: i32,
        pitch: i32,
        note_id: i32,
        tuning: f32,
        velocity: f32,
    },
    Data {
        data_type: u32,
        bytes: &'a [u8],
    },
    PolyPressure {
        channel: i32,
        pitch: i32,
        note_id: i32,
        pressure: f32,
    },
    ExpressionValue {
        type_id: u32,
        note_id: i32,
        value: f64,
    },
    ExpressionText {
        type_id: u32,
        note_id: i32,
        text: &'a [u16],
    },
    Chord {
        root: i32,
        bass: i32,
        mask: u16,
        text: &'a [u16],
    },
    Scale {
        root: i32,
        mask: u16,
        text: &'a [u16],
    },
    ExpressionInteger {
        type_id: u32,
        note_id: i32,
        value: u64,
    },
    Parameter {
        id: u32,
        value: f64,
    },
}
impl ProbeEvent<'_> {
    /// SDK event type 0..=8, or [`PARAMETER_CODE`].
    pub fn code(&self) -> u16 {
        match self {
            Self::NoteOn { .. } => 0,
            Self::NoteOff { .. } => 1,
            Self::Data { .. } => 2,
            Self::PolyPressure { .. } => 3,
            Self::ExpressionValue { .. } => 4,
            Self::ExpressionText { .. } => 5,
            Self::Chord { .. } => 6,
            Self::Scale { .. } => 7,
            Self::ExpressionInteger { .. } => 8,
            Self::Parameter { .. } => PARAMETER_CODE,
        }
    }
}

/// One probe record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeRecord<'a> {
    pub header: ProbeHeader,
    pub event: ProbeEvent<'a>,
}

struct Fnv(u32);
impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u32::from(*byte)).wrapping_mul(0x0100_0193);
        }
    }
    fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    fn i32(&mut self, value: i32) {
        self.bytes(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    fn text(&mut self, text: &[u16]) {
        self.u32(text.len() as u32);
        for unit in text {
            self.u16(*unit);
        }
    }
}

/// 20-bit record hash: 32-bit FNV-1a (basis `0x811c9dc5`, prime `0x01000193`)
/// over the little-endian canonical serialization, folded as
/// `(h ^ (h >> 20)) & 0xfffff`.
///
/// Serialization: `u16 code, u16 flags, i32 bus, u64 ppq bits`, then per kind
/// (floats as IEEE bits, text as `u32` unit count then `u16` units):
/// note-on `channel, pitch, note_id, length, tuning, velocity`; note-off
/// `channel, pitch, note_id, tuning, velocity`; data `u32 type, u32 size,
/// bytes`; poly pressure `channel, pitch, note_id, pressure`; expression value
/// `u32 type, note_id, f64 value`; expression text `u32 type, note_id, text`;
/// chord `root, bass, u32 mask, text`; scale `root, u32 mask, text`; integer
/// expression `u32 type, note_id, u64 value`; parameter `u32 id, f64 value`.
pub fn record_hash(record: &ProbeRecord<'_>) -> u32 {
    let mut h = Fnv(0x811c_9dc5);
    let header = record.header;
    h.u16(record.event.code());
    h.u16(header.flags);
    h.i32(header.bus);
    h.u64(header.ppq_position.to_bits());
    match record.event {
        ProbeEvent::NoteOn {
            channel,
            pitch,
            note_id,
            length,
            tuning,
            velocity,
        } => {
            for v in [channel, pitch, note_id, length] {
                h.i32(v);
            }
            h.u32(tuning.to_bits());
            h.u32(velocity.to_bits());
        }
        ProbeEvent::NoteOff {
            channel,
            pitch,
            note_id,
            tuning,
            velocity,
        } => {
            for v in [channel, pitch, note_id] {
                h.i32(v);
            }
            h.u32(tuning.to_bits());
            h.u32(velocity.to_bits());
        }
        ProbeEvent::Data { data_type, bytes } => {
            h.u32(data_type);
            h.u32(bytes.len() as u32);
            h.bytes(bytes);
        }
        ProbeEvent::PolyPressure {
            channel,
            pitch,
            note_id,
            pressure,
        } => {
            for v in [channel, pitch, note_id] {
                h.i32(v);
            }
            h.u32(pressure.to_bits());
        }
        ProbeEvent::ExpressionValue {
            type_id,
            note_id,
            value,
        } => {
            h.u32(type_id);
            h.i32(note_id);
            h.u64(value.to_bits());
        }
        ProbeEvent::ExpressionText {
            type_id,
            note_id,
            text,
        } => {
            h.u32(type_id);
            h.i32(note_id);
            h.text(text);
        }
        ProbeEvent::Chord {
            root,
            bass,
            mask,
            text,
        } => {
            h.i32(root);
            h.i32(bass);
            h.u32(u32::from(mask));
            h.text(text);
        }
        ProbeEvent::Scale { root, mask, text } => {
            h.i32(root);
            h.u32(u32::from(mask));
            h.text(text);
        }
        ProbeEvent::ExpressionInteger {
            type_id,
            note_id,
            value,
        } => {
            h.u32(type_id);
            h.i32(note_id);
            h.u64(value);
        }
        ProbeEvent::Parameter { id, value } => {
            h.u32(id);
            h.u64(value.to_bits());
        }
    }
    (h.0 ^ (h.0 >> HASH_BITS)) & ((1 << HASH_BITS) - 1)
}

/// Running digest step: `(digest * 0x01000193 ^ hash) & 0xfffff` (wrapping).
pub fn digest_step(digest: u32, hash: u32) -> u32 {
    (digest.wrapping_mul(0x0100_0193) ^ hash) & ((1 << HASH_BITS) - 1)
}

/// Rejected [`EventProbe::render`] call; outputs are silenced and the digest
/// is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeError {
    NotPrepared,
    TooManyFrames,
    TooManyRecords,
    ShortOutput,
    OffsetOutOfRange { index: usize },
}

/// Deterministic event-to-audio encoder with a running digest.
///
/// Rendering is exact in f32 while at most 16 records share one offset (20-bit
/// hash fractions then fit the 24-bit significand) and always exact in f64 for
/// the 4096-record limit.
#[derive(Debug, Default)]
pub struct EventProbe {
    maximum_frames: Option<usize>,
    digest: u32,
}
impl EventProbe {
    /// Unprepared probe with a zero digest.
    pub fn new() -> Self {
        Self::default()
    }
    /// Admit blocks of 1..=65536 maximum frames and clear the digest.
    pub fn prepare(&mut self, maximum_frames: usize) -> Result<(), PrepareError> {
        if !(1..=65536).contains(&maximum_frames) {
            self.maximum_frames = None;
            return Err(PrepareError::InvalidMaximumFrames);
        }
        self.maximum_frames = Some(maximum_frames);
        self.digest = 0;
        Ok(())
    }
    /// Clear the running digest; no allocation.
    pub fn reset(&mut self) {
        self.digest = 0;
    }
    /// Prepared maximum frames, or `None` before a successful prepare.
    pub fn maximum_frames(&self) -> Option<usize> {
        self.maximum_frames
    }
    /// Running 20-bit digest over every record rendered since prepare/reset.
    pub fn digest(&self) -> u32 {
        self.digest
    }
    /// Validate every record, then write `frames` samples of both channels and
    /// fold every record (in slice order) into the digest. Offsets must be
    /// below `frames`, or zero for a zero-frame block; order is not required.
    pub fn render<S: ReferenceSample>(
        &mut self,
        records: &[ProbeRecord<'_>],
        left: &mut [S],
        right: &mut [S],
        frames: usize,
    ) -> Result<u32, ProbeError> {
        let checked = self.check(records, left.len(), right.len(), frames);
        let frames = frames.min(left.len()).min(right.len());
        left[..frames].fill(S::default());
        right[..frames].fill(S::default());
        checked?;
        let mut digest = self.digest;
        for record in records {
            let hash = record_hash(record);
            digest = digest_step(digest, hash);
            if frames > 0 {
                let offset = record.header.offset as usize;
                left[offset] = left[offset] + S::exact(u32::from(record.event.code()) + 1);
                right[offset] = right[offset] + S::exact(hash) * S::HASH_SCALE;
            }
        }
        self.digest = digest;
        Ok(digest)
    }
    fn check(
        &self,
        records: &[ProbeRecord<'_>],
        left: usize,
        right: usize,
        frames: usize,
    ) -> Result<(), ProbeError> {
        let maximum = self.maximum_frames.ok_or(ProbeError::NotPrepared)?;
        if frames > maximum {
            return Err(ProbeError::TooManyFrames);
        }
        if left < frames || right < frames {
            return Err(ProbeError::ShortOutput);
        }
        if records.len() > MAXIMUM_PROBE_RECORDS {
            return Err(ProbeError::TooManyRecords);
        }
        for (index, record) in records.iter().enumerate() {
            let offset = record.header.offset as usize;
            if (frames == 0 && offset != 0) || (frames > 0 && offset >= frames) {
                return Err(ProbeError::OffsetOutOfRange { index });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
