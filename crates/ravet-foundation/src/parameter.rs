//! Borrowed parameter schemas and fixed-size state. No operation allocates.
use crate::ParameterId;

/// Mapping between normalized host/editor values and physical processor values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterScale {
    Linear,
    /// Requires strictly positive bounds.
    Logarithmic,
}

/// A finite nonempty range. Discrete ranges have equally spaced physical steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterRange {
    minimum: f32,
    maximum: f32,
    scale: ParameterScale,
    intervals: Option<u16>,
}

/// Invalid schemas, values, or complete persisted records are rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterError {
    InvalidRange,
    InvalidMetadata,
    DuplicateId(ParameterId),
    InvalidValue(ParameterId),
    InvalidRecord,
    WrongProduct,
    UnsupportedVersion(u32),
    WrongParameter(ParameterId),
}
impl std::fmt::Display for ParameterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parameter error: {self:?}")
    }
}
impl std::error::Error for ParameterError {}

impl ParameterRange {
    /// `intervals = Some(n)` gives n+1 choices, including both endpoints.
    /// Zero intervals and logarithmic discrete ranges are rejected.
    pub const fn new(
        minimum: f32,
        maximum: f32,
        scale: ParameterScale,
        intervals: Option<u16>,
    ) -> Result<Self, ParameterError> {
        if !minimum.is_finite()
            || !maximum.is_finite()
            || minimum >= maximum
            || (matches!(scale, ParameterScale::Logarithmic)
                && (minimum <= 0.0 || intervals.is_some()))
            || matches!(intervals, Some(0))
        {
            return Err(ParameterError::InvalidRange);
        }
        Ok(Self {
            minimum,
            maximum,
            scale,
            intervals,
        })
    }
    pub fn minimum(self) -> f32 {
        self.minimum
    }
    pub fn maximum(self) -> f32 {
        self.maximum
    }
    pub fn scale(self) -> ParameterScale {
        self.scale
    }
    pub fn intervals(self) -> Option<u16> {
        self.intervals
    }

    /// Finite values clamp to bounds and snap to the closest discrete step.
    /// Ties round towards the larger value. Nonfinite values return None.
    pub fn canonicalize(self, value: f32) -> Option<f32> {
        if !value.is_finite() {
            return None;
        }
        let value = value.clamp(self.minimum, self.maximum);
        let value = if let Some(n) = self.intervals {
            let fraction =
                (value as f64 - self.minimum as f64) / (self.maximum as f64 - self.minimum as f64);
            (self.minimum as f64
                + (fraction * n as f64).round() / n as f64
                    * (self.maximum as f64 - self.minimum as f64)) as f32
        } else {
            value
        };
        Some(if value == 0.0 { 0.0 } else { value })
    }
    /// Normalize a clamped, snapped physical value to 0..=1.
    pub fn normalize(self, physical: f32) -> Option<f32> {
        let value = self.canonicalize(physical)? as f64;
        let min = self.minimum as f64;
        let max = self.maximum as f64;
        Some(match self.scale {
            ParameterScale::Linear => ((value - min) / (max - min)) as f32,
            ParameterScale::Logarithmic => ((value / min).ln() / (max / min).ln()) as f32,
        })
    }
    /// Clamp a finite normalized value to 0..=1, then map and snap it.
    pub fn denormalize(self, normalized: f32) -> Option<f32> {
        if !normalized.is_finite() {
            return None;
        }
        let value = normalized.clamp(0.0, 1.0) as f64;
        // Preserve endpoints even for extreme logarithmic ranges.
        if value == 0.0 {
            return Some(self.minimum);
        }
        if value == 1.0 {
            return Some(self.maximum);
        }
        let min = self.minimum as f64;
        let max = self.maximum as f64;
        self.canonicalize(match self.scale {
            ParameterScale::Linear => (min + value * (max - min)) as f32,
            ParameterScale::Logarithmic => (min * (value * (max / min).ln()).exp()) as f32,
        })
    }
    fn accepts(self, value: f32) -> bool {
        value.is_finite()
            && value >= self.minimum
            && value <= self.maximum
            && self.canonicalize(value) == Some(value)
    }
}

/// Product metadata; IDs are permanent, names and units are display text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterDescriptor<'a> {
    pub id: ParameterId,
    pub name: &'a str,
    pub unit: &'a str,
    pub range: ParameterRange,
    pub default: f32,
    pub automatable: bool,
}

/// Validated immutable descriptors, borrowed from product-owned storage.
#[derive(Clone, Copy, Debug)]
pub struct ParameterSchema<'a, const N: usize> {
    descriptors: &'a [ParameterDescriptor<'a>; N],
}
impl<'a, const N: usize> ParameterSchema<'a, N> {
    /// Reject empty names, duplicate IDs, and noncanonical defaults.
    pub fn new(descriptors: &'a [ParameterDescriptor<'a>; N]) -> Result<Self, ParameterError> {
        for (index, descriptor) in descriptors.iter().enumerate() {
            if descriptor.name.is_empty() {
                return Err(ParameterError::InvalidMetadata);
            }
            if !descriptor.range.accepts(descriptor.default) {
                return Err(ParameterError::InvalidValue(descriptor.id));
            }
            if descriptors[..index].iter().any(|d| d.id == descriptor.id) {
                return Err(ParameterError::DuplicateId(descriptor.id));
            }
        }
        Ok(Self { descriptors })
    }
    pub fn descriptors(self) -> &'a [ParameterDescriptor<'a>; N] {
        self.descriptors
    }
    pub fn index(self, id: ParameterId) -> Option<usize> {
        self.descriptors.iter().position(|d| d.id == id)
    }
    pub fn defaults(self) -> ParameterState<'a, N> {
        ParameterState {
            schema: self,
            values: self.descriptors.map(|d| d.default),
        }
    }
    /// Validate all physical values strictly; state loading never clamps damage.
    pub fn state(self, values: [f32; N]) -> Result<ParameterState<'a, N>, ParameterError> {
        for (d, value) in self.descriptors.iter().zip(values) {
            if !d.range.accepts(value) {
                return Err(ParameterError::InvalidValue(d.id));
            }
        }
        Ok(ParameterState {
            schema: self,
            values: values.map(|v| if v == 0.0 { 0.0 } else { v }),
        })
    }
}

/// Complete validated physical values tied to their schema. No DSP history.
#[derive(Clone, Copy, Debug)]
pub struct ParameterState<'a, const N: usize> {
    schema: ParameterSchema<'a, N>,
    values: [f32; N],
}
impl<'a, const N: usize> ParameterState<'a, N> {
    pub fn values(&self) -> &[f32; N] {
        &self.values
    }
    pub fn get(&self, id: ParameterId) -> Option<f32> {
        self.schema.index(id).map(|i| self.values[i])
    }
    /// Validate before mutation; failure leaves the complete state unchanged.
    pub fn set(&mut self, id: ParameterId, value: f32) -> Result<(), ParameterError> {
        let index = self
            .schema
            .index(id)
            .ok_or(ParameterError::WrongParameter(id))?;
        if !self.schema.descriptors[index].range.accepts(value) {
            return Err(ParameterError::InvalidValue(id));
        }
        self.values[index] = if value == 0.0 { 0.0 } else { value };
        Ok(())
    }
    /// Migrate by stable ID: new fields use defaults, removed fields are dropped.
    /// Changed ranges must still accept the old physical value; no silent clamp.
    /// Products must explicitly select an allowed source version before migration.
    pub fn migrate<'b, const M: usize>(
        &self,
        target: ParameterSchema<'b, M>,
    ) -> Result<ParameterState<'b, M>, ParameterError> {
        let values = target
            .descriptors
            .map(|d| self.get(d.id).unwrap_or(d.default));
        target.state(values)
    }
    /// Record size: 8-byte product ID, u32 version, u32 count, then ID/f32 pairs.
    pub fn encoded_len() -> Option<usize> {
        N.checked_mul(8)?.checked_add(16)
    }
    /// Write an exact-length little-endian record. Version zero is reserved.
    /// Invalid destination/version leaves the destination unchanged.
    pub fn encode(
        &self,
        product: [u8; 8],
        version: u32,
        output: &mut [u8],
    ) -> Result<(), ParameterError> {
        if version == 0 || u32::try_from(N).is_err() || Some(output.len()) != Self::encoded_len() {
            return Err(ParameterError::InvalidRecord);
        }
        output[..8].copy_from_slice(&product);
        output[8..12].copy_from_slice(&version.to_le_bytes());
        output[12..16].copy_from_slice(&(N as u32).to_le_bytes());
        for (index, d) in self.schema.descriptors.iter().enumerate() {
            let start = 16 + index * 8;
            output[start..start + 4].copy_from_slice(&d.id.0.to_le_bytes());
            output[start + 4..start + 8].copy_from_slice(&self.values[index].to_le_bytes());
        }
        Ok(())
    }
    /// Decode a complete record in any field order. Reject missing, duplicate,
    /// unknown IDs, trailing bytes, invalid values and unexpected versions.
    /// No live state is mutated; publish only the returned complete result.
    pub fn decode(
        schema: ParameterSchema<'a, N>,
        product: [u8; 8],
        version: u32,
        input: &[u8],
    ) -> Result<Self, ParameterError> {
        if version == 0 || Some(input.len()) != Self::encoded_len() || input.len() < 16 {
            return Err(ParameterError::InvalidRecord);
        }
        if input[..8] != product {
            return Err(ParameterError::WrongProduct);
        }
        let actual_version = u32::from_le_bytes(input[8..12].try_into().unwrap());
        if actual_version != version {
            return Err(ParameterError::UnsupportedVersion(actual_version));
        }
        if u32::from_le_bytes(input[12..16].try_into().unwrap()) as usize != N {
            return Err(ParameterError::InvalidRecord);
        }
        let mut values = [0.0; N];
        let mut seen = [false; N];
        for pair in input[16..].as_chunks::<8>().0 {
            let id = ParameterId(u32::from_le_bytes(pair[..4].try_into().unwrap()));
            let index = schema.index(id).ok_or(ParameterError::WrongParameter(id))?;
            if seen[index] {
                return Err(ParameterError::DuplicateId(id));
            }
            seen[index] = true;
            values[index] = f32::from_le_bytes(pair[4..].try_into().unwrap());
        }
        schema.state(values)
    }
}
