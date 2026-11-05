//! The versioned state envelope every export format stores.
//!
//! One byte layout is used unchanged in VST3 component state, the AU class-info
//! dictionary, AAX chunks, CLAP state and LV2 state properties. All integers are
//! little-endian:
//!
//! | Bytes | Field |
//! |---|---|
//! | 8 | Magic `RAVETSTA` |
//! | 2 | Envelope version, currently 1 |
//! | 2 | Flags, currently 0 |
//! | 8 | Product state tag ([`StateFormat::tag`]) |
//! | 4 | Product state version ([`StateFormat::version`]) |
//! | 4 | Parameter count *n*, at most 1,024 |
//! | 8 × *n* | Parameter ID (`u32`) and physical value (`f32`) |
//! | 4 | Extra data length *m*, at most 16 MiB |
//! | *m* | Extra data owned by the product |
//!
//! Read-only parameters are not saved. Every function here allocates and must run
//! on a control thread, never in an audio callback.
use crate::product::{ResolvedProduct, StateFormat};
use ravet_foundation::ParameterId;

/// Magic bytes at the start of every RAVET state envelope.
pub const MAGIC: [u8; 8] = *b"RAVETSTA";
/// Envelope layout version written by this release.
pub const ENVELOPE_VERSION: u16 = 1;
/// Maximum saved parameters.
pub const MAXIMUM_SAVED_PARAMETERS: usize = 1024;
/// Maximum product extra data.
pub const MAXIMUM_EXTRA_BYTES: usize = 16 * 1024 * 1024;
const HEADER_BYTES: usize = 28;

/// Reasons state cannot be written or restored. Nothing is partially applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateError {
    /// The bytes are not a RAVET envelope and no import hook accepted them.
    Foreign,
    /// The envelope belongs to another product.
    WrongProduct,
    UnsupportedEnvelope(u16),
    /// Saved by a newer product state version than this build understands.
    TooNew(u32),
    /// Truncated, oversized, trailing or otherwise malformed bytes.
    Malformed,
    DuplicateParameter(ParameterId),
    /// A value is not finite, or not exactly representable in the current range.
    /// Values are never silently clamped.
    InvalidValue(ParameterId),
    /// The value count passed to [`encode`] does not match the product.
    WrongValueCount,
    TooLarge,
    /// A product hook rejected the state.
    Rejected,
}
impl core::fmt::Display for StateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "state error: {self:?}")
    }
}
impl std::error::Error for StateError {}

/// The contents of an envelope, before matching against the current product.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedState {
    /// Product state version the data was written with.
    pub version: u32,
    /// Saved parameter values by host ID, in saved order.
    pub parameters: Vec<(ParameterId, f32)>,
    /// Product extra data.
    pub extra: Vec<u8>,
}

/// Where restored state came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Written by the current product state version.
    Current,
    /// Written by an earlier product state version and migrated.
    Earlier(u32),
    /// Produced by [`StateHooks::import`] from non-RAVET data.
    Imported,
}

/// State matched to the current product, ready to publish as a whole.
#[derive(Clone, Debug, PartialEq)]
pub struct Restored {
    /// Physical values in declaration order. Parameters missing from the saved
    /// data, and read-only parameters, hold their defaults.
    pub values: Vec<f32>,
    pub extra: Vec<u8>,
    pub source: Source,
}

/// Product hooks for older and foreign state. Both run on a control thread.
pub trait StateHooks {
    /// Update state written by an earlier product state version. On return,
    /// parameters are matched to the current product by ID: unknown IDs are
    /// dropped and missing ones take defaults.
    fn migrate(&self, _saved: &mut SavedState) -> Result<(), StateError> {
        Ok(())
    }
    /// Convert data without the RAVET magic, for example sessions written by an
    /// earlier framework. Return `None` to reject it. The result is treated as
    /// the current product state version.
    fn import(&self, _bytes: &[u8]) -> Option<SavedState> {
        None
    }
}

/// Hooks that accept earlier versions unchanged and reject foreign data.
pub struct NoHooks;
impl StateHooks for NoHooks {}

/// Write an envelope from physical values in declaration order.
pub fn encode(
    product: &ResolvedProduct<'_>,
    values: &[f32],
    extra: &[u8],
) -> Result<Vec<u8>, StateError> {
    let parameters = product.product().parameters();
    if values.len() != parameters.len() {
        return Err(StateError::WrongValueCount);
    }
    if extra.len() > MAXIMUM_EXTRA_BYTES {
        return Err(StateError::TooLarge);
    }
    let format = product.product().state_format();
    let saved: Vec<_> = parameters
        .iter()
        .zip(product.parameter_ids())
        .zip(values)
        .filter(|((parameter, _), _)| !parameter.is_read_only())
        .map(|((parameter, id), value)| {
            if parameter.range().canonicalize(*value) == Some(*value) {
                Ok((*id, *value))
            } else {
                Err(StateError::InvalidValue(*id))
            }
        })
        .collect::<Result<_, _>>()?;
    let mut output = Vec::with_capacity(HEADER_BYTES + saved.len() * 8 + 4 + extra.len());
    output.extend_from_slice(&MAGIC);
    output.extend_from_slice(&ENVELOPE_VERSION.to_le_bytes());
    output.extend_from_slice(&0u16.to_le_bytes());
    output.extend_from_slice(&format.tag);
    output.extend_from_slice(&format.version.to_le_bytes());
    output.extend_from_slice(&(saved.len() as u32).to_le_bytes());
    for (id, value) in saved {
        output.extend_from_slice(&id.0.to_le_bytes());
        output.extend_from_slice(&value.to_le_bytes());
    }
    output.extend_from_slice(&(extra.len() as u32).to_le_bytes());
    output.extend_from_slice(extra);
    Ok(output)
}

/// Parse an envelope for `format` without matching it to parameters.
pub fn read(format: StateFormat, bytes: &[u8]) -> Result<SavedState, StateError> {
    if bytes.len() < 8 || bytes[..8] != MAGIC {
        return Err(StateError::Foreign);
    }
    let mut reader = Reader { bytes, offset: 8 };
    let envelope = u16::from_le_bytes(reader.take::<2>()?);
    if envelope != ENVELOPE_VERSION {
        return Err(StateError::UnsupportedEnvelope(envelope));
    }
    if u16::from_le_bytes(reader.take::<2>()?) != 0 {
        return Err(StateError::Malformed);
    }
    if reader.take::<8>()? != format.tag {
        return Err(StateError::WrongProduct);
    }
    let version = reader.u32()?;
    if version == 0 {
        return Err(StateError::Malformed);
    }
    if version > format.version {
        return Err(StateError::TooNew(version));
    }
    let count = reader.u32()? as usize;
    if count > MAXIMUM_SAVED_PARAMETERS {
        return Err(StateError::Malformed);
    }
    let mut parameters = Vec::with_capacity(count);
    for _ in 0..count {
        let id = ParameterId(reader.u32()?);
        let value = f32::from_le_bytes(reader.take::<4>()?);
        if parameters.iter().any(|(existing, _)| *existing == id) {
            return Err(StateError::DuplicateParameter(id));
        }
        if !value.is_finite() {
            return Err(StateError::InvalidValue(id));
        }
        parameters.push((id, value));
    }
    let extra_length = reader.u32()? as usize;
    if extra_length > MAXIMUM_EXTRA_BYTES || reader.remaining() != extra_length {
        return Err(StateError::Malformed);
    }
    Ok(SavedState {
        version,
        parameters,
        extra: bytes[reader.offset..].to_vec(),
    })
}

/// Read, import or migrate as needed, then match saved values to the product.
pub fn restore(
    product: &ResolvedProduct<'_>,
    bytes: &[u8],
    hooks: &impl StateHooks,
) -> Result<Restored, StateError> {
    let format = product.product().state_format();
    let (mut saved, source) = match read(format, bytes) {
        Ok(saved) if saved.version == format.version => (saved, Source::Current),
        Ok(saved) => {
            let version = saved.version;
            (saved, Source::Earlier(version))
        }
        Err(StateError::Foreign) => (
            hooks.import(bytes).ok_or(StateError::Foreign)?,
            Source::Imported,
        ),
        Err(error) => return Err(error),
    };
    if let Source::Earlier(_) = source {
        hooks.migrate(&mut saved)?;
    }
    if source == Source::Imported {
        saved.version = format.version;
    }
    if saved.extra.len() > MAXIMUM_EXTRA_BYTES || saved.parameters.len() > MAXIMUM_SAVED_PARAMETERS
    {
        return Err(StateError::TooLarge);
    }
    let mut values = product.defaults();
    let mut seen = vec![false; values.len()];
    for (id, value) in &saved.parameters {
        let Some(index) = product.parameter_index(*id) else {
            continue;
        };
        if seen[index] {
            return Err(StateError::DuplicateParameter(*id));
        }
        seen[index] = true;
        let parameter = &product.product().parameters()[index];
        if parameter.is_read_only() {
            continue;
        }
        if parameter.range().canonicalize(*value) != Some(*value) {
            return Err(StateError::InvalidValue(*id));
        }
        values[index] = if *value == 0.0 { 0.0 } else { *value };
    }
    Ok(Restored {
        values,
        extra: saved.extra,
        source,
    })
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], StateError> {
        let end = self.offset.checked_add(N).ok_or(StateError::Malformed)?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(StateError::Malformed)?;
        self.offset = end;
        slice.try_into().map_err(|_| StateError::Malformed)
    }
    fn u32(&mut self) -> Result<u32, StateError> {
        Ok(u32::from_le_bytes(self.take::<4>()?))
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
}
