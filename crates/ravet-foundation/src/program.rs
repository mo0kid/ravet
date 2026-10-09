//! Immutable complete parameter programs, independent of host/file policy.
use crate::{ParameterError, ParameterSchema, ParameterState};

/// Permanent product-owned identifier. Host indices are separate and may change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramId(pub u32);

/// Consumer-owned complete physical values in schema order. The bank constructor
/// validates all entries before exposing any selection. MIDI bank/number are
/// zero-based 0..16383/0..127; names are nonblank UTF-8 <=255 bytes without NUL.
#[derive(Clone, Copy, Debug)]
pub struct ParameterProgram<'a, const N: usize> {
    pub id: ProgramId,
    pub name: &'a str,
    pub midi_bank: u16,
    pub midi_number: u8,
    pub values: [f32; N],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramError {
    Capacity,
    Metadata(ProgramId),
    DuplicateId(ProgramId),
    DuplicateAddress {
        bank: u16,
        number: u8,
    },
    Values {
        program: ProgramId,
        error: ParameterError,
    },
}
impl std::fmt::Display for ProgramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parameter program error: {self:?}")
    }
}
impl std::error::Error for ProgramError {}

/// Borrowed 1..128-program bank with 1..1024 parameters and <=65536 values total.
/// Preparation rejects duplicate IDs/addresses and strictly validates every
/// complete state; no partial registration, clamping or default-filled programs.
/// Lookup/state copying uses bounded linear work without allocation, locks or I/O.
/// Returned states remain tied to this schema and normalize signed zero just as
/// `ParameterSchema::state` does. A bank has no mutable current index: the consumer
/// decides when/where to apply a complete state and how edits affect its identity.
/// No file loading, MIDI routing, host registration/notifications, undo, voice
/// reset, DSP history migration or click-free switching is synthesized.
#[derive(Clone, Copy, Debug)]
pub struct ProgramBank<'a, const N: usize> {
    schema: ParameterSchema<'a, N>,
    programs: &'a [ParameterProgram<'a, N>],
}
impl<'a, const N: usize> ProgramBank<'a, N> {
    pub fn new(
        schema: ParameterSchema<'a, N>,
        programs: &'a [ParameterProgram<'a, N>],
    ) -> Result<Self, ProgramError> {
        if !(1..=1024).contains(&N)
            || !(1..=128).contains(&programs.len())
            || N.checked_mul(programs.len()).is_none_or(|n| n > 65536)
        {
            return Err(ProgramError::Capacity);
        }
        for (index, program) in programs.iter().enumerate() {
            if program.name.trim().is_empty()
                || program.name.len() > 255
                || program.name.contains('\0')
                || program.midi_bank > 16383
                || program.midi_number > 127
            {
                return Err(ProgramError::Metadata(program.id));
            }
            if programs[..index].iter().any(|p| p.id == program.id) {
                return Err(ProgramError::DuplicateId(program.id));
            }
            if programs[..index]
                .iter()
                .any(|p| (p.midi_bank, p.midi_number) == (program.midi_bank, program.midi_number))
            {
                return Err(ProgramError::DuplicateAddress {
                    bank: program.midi_bank,
                    number: program.midi_number,
                });
            }
            schema
                .state(program.values)
                .map_err(|error| ProgramError::Values {
                    program: program.id,
                    error,
                })?;
        }
        Ok(Self { schema, programs })
    }
    pub fn programs(self) -> &'a [ParameterProgram<'a, N>] {
        self.programs
    }
    /// Index for a permanent ID, independent of display/MIDI addressing.
    pub fn index(self, id: ProgramId) -> Option<usize> {
        self.programs.iter().position(|p| p.id == id)
    }
    /// Exact MIDI address lookup. Invalid or unavailable addresses return None.
    pub fn midi_index(self, bank: u16, number: u8) -> Option<usize> {
        self.programs
            .iter()
            .position(|p| (p.midi_bank, p.midi_number) == (bank, number))
    }
    /// Copy a complete validated state. Invalid indices return None; the bank and
    /// any consumer state remain unchanged. This is a proposal, not publication.
    pub fn state(self, index: usize) -> Option<ParameterState<'a, N>> {
        self.programs
            .get(index)
            .and_then(|p| self.schema.state(p.values).ok())
    }
}
