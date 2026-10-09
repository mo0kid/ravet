//! Legacy scalar state and complete portable gain/bypass state.
//!
//! The 20-byte little-endian record stores product magic, schema version,
//! permanent parameter ID, and requested amplitude. It excludes smoothing
//! progress, host channel configuration, and transient diagnostics. Stream I/O
//! belongs on the control thread; decode fully before changing live state.
//! [`encode_parameters`] and [`decode_parameters`] provide the separate complete
//! version-two record, with explicit migration from version one. CLAP now saves
//! version two and loads either version; [`GainState`] retains the scalar API.

use crate::{GAIN_ID, MAXIMUM_GAIN};
use std::{
    fmt,
    io::{self, Read, Write},
};

pub const STATE_BYTES: usize = 20;
const MAGIC: &[u8; 8] = b"RAVETGN\0";
const VERSION: u32 = 1;

/// A validated gain snapshot, suitable for complete atomic publication.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GainState {
    gain: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateError {
    InvalidLength,
    WrongProduct,
    UnsupportedVersion(u32),
    WrongParameter,
    InvalidGain,
}
impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => f.write_str("gain state must contain exactly 20 bytes"),
            Self::WrongProduct => f.write_str("state belongs to another product"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported gain state version {v}"),
            Self::WrongParameter => f.write_str("invalid gain parameter identity"),
            Self::InvalidGain => f.write_str("state gain must be finite and in 0..=4"),
        }
    }
}
impl std::error::Error for StateError {}

impl GainState {
    /// State rejects out-of-range values rather than clamping damaged records.
    /// Negative zero is canonicalized to positive zero.
    pub fn new(gain: f32) -> Result<Self, StateError> {
        if !gain.is_finite() || !(0.0..=MAXIMUM_GAIN).contains(&gain) {
            return Err(StateError::InvalidGain);
        }
        Ok(Self {
            gain: if gain == 0.0 { 0.0 } else { gain },
        })
    }
    pub fn gain(self) -> f32 {
        self.gain
    }
    pub fn encode(self) -> [u8; STATE_BYTES] {
        let mut bytes = [0; STATE_BYTES];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..12].copy_from_slice(&VERSION.to_le_bytes());
        bytes[12..16].copy_from_slice(&GAIN_ID.0.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.gain.to_le_bytes());
        bytes
    }
    /// Version one is the first persisted schema. Unknown versions fail closed;
    /// future migrations must retain version-one fixture coverage.
    pub fn decode(bytes: &[u8]) -> Result<Self, StateError> {
        let bytes: &[u8; STATE_BYTES] = bytes.try_into().map_err(|_| StateError::InvalidLength)?;
        if &bytes[..8] != MAGIC {
            return Err(StateError::WrongProduct);
        }
        let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        if version != VERSION {
            return Err(StateError::UnsupportedVersion(version));
        }
        if u32::from_le_bytes(bytes[12..16].try_into().unwrap()) != GAIN_ID.0 {
            return Err(StateError::WrongParameter);
        }
        Self::new(f32::from_le_bytes(bytes[16..20].try_into().unwrap()))
    }
    /// Reads at most 21 bytes, including an EOF probe; supports short reads.
    /// Nothing is published if parsing or stream I/O fails.
    pub fn read(reader: &mut impl Read) -> io::Result<Self> {
        let mut bytes = [0; STATE_BYTES];
        reader.read_exact(&mut bytes)?;
        let mut trailing = [0];
        loop {
            match reader.read(&mut trailing) {
                Ok(0) => break,
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        StateError::InvalidLength,
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Self::decode(&bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
    /// Writes the complete canonical record, supporting short writes.
    /// The caller must discard a partially written destination on failure.
    pub fn write(self, writer: &mut impl Write) -> io::Result<()> {
        writer.write_all(&self.encode())
    }
}

/// Complete gain/bypass record size; CLAP also uses this version-two format.
pub const PARAMETERS_STATE_BYTES: usize = 32;

/// Encode version two using the reusable stable-ID codec. Validate against the
/// product schema before encoding; display/schema order does not define identity.
pub fn encode_parameters(
    state: &ravet::foundation::ParameterState<'_, 2>,
) -> Result<[u8; PARAMETERS_STATE_BYTES], ravet::foundation::ParameterError> {
    use ravet::foundation::ParameterError;
    let gain = state
        .get(GAIN_ID)
        .ok_or(ParameterError::WrongParameter(GAIN_ID))?;
    let bypass = state
        .get(crate::BYPASS_ID)
        .ok_or(ParameterError::WrongParameter(crate::BYPASS_ID))?;
    let state = crate::parameter_schema().state([gain, bypass])?;
    let mut bytes = [0; PARAMETERS_STATE_BYTES];
    state.encode(*MAGIC, 2, &mut bytes)?;
    Ok(bytes)
}

/// Decode a complete version-two record or explicitly migrate a legacy
/// version-one scalar record with bypass off. No processor state is changed.
pub fn decode_parameters(
    bytes: &[u8],
) -> Result<ravet::foundation::ParameterState<'static, 2>, ravet::foundation::ParameterError> {
    use ravet::foundation::ParameterError;
    if bytes.len() == STATE_BYTES {
        let legacy = GainState::decode(bytes).map_err(|error| match error {
            StateError::WrongProduct => ParameterError::WrongProduct,
            StateError::UnsupportedVersion(v) => ParameterError::UnsupportedVersion(v),
            StateError::WrongParameter => ParameterError::WrongParameter(GAIN_ID),
            StateError::InvalidGain => ParameterError::InvalidValue(GAIN_ID),
            StateError::InvalidLength => ParameterError::InvalidRecord,
        })?;
        return crate::parameter_schema().state([legacy.gain(), 0.0]);
    }
    ravet::foundation::ParameterState::decode(crate::parameter_schema(), *MAGIC, 2, bytes)
}

/// Read a complete version-two or legacy version-one record using at most
/// 33 bytes (including EOF probe). Short reads and Interrupted are supported.
/// Decode fully before publishing; stream I/O belongs on the control thread.
pub fn read_parameters(
    reader: &mut impl Read,
) -> io::Result<ravet::foundation::ParameterState<'static, 2>> {
    let mut bytes = [0; PARAMETERS_STATE_BYTES + 1];
    let mut length = 0;
    while length < bytes.len() {
        match reader.read(&mut bytes[length..]) {
            Ok(0) => {
                return decode_parameters(&bytes[..length])
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
            }
            Ok(count) => length += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "gain parameter state exceeds 32 bytes",
    ))
}

/// Write the canonical complete version-two record, supporting short writes.
/// Discard a partially written destination if an I/O failure occurs.
pub fn write_parameters(
    state: &ravet::foundation::ParameterState<'_, 2>,
    writer: &mut impl Write,
) -> io::Result<()> {
    writer.write_all(
        &encode_parameters(state).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
    )
}
