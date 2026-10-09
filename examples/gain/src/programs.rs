//! Complete public reference programs; host selection and DSP publication separate.
use ravet::foundation::{ParameterProgram, ProgramBank, ProgramId};

/// Permanent IDs 1..3, MIDI bank 0/numbers 0..2. Unity/Silence/Bypass contain
/// complete gain/bypass values `[1,0]`/`[0,0]`/`[1,1]`. Programs have no DSP history,
/// selected identity, host notification, smoothing or file policy of their own.
pub static GAIN_PROGRAMS: [ParameterProgram<'static, 2>; 3] = [
    ParameterProgram {
        id: ProgramId(1),
        name: "Unity",
        midi_bank: 0,
        midi_number: 0,
        values: [1., 0.],
    },
    ParameterProgram {
        id: ProgramId(2),
        name: "Silence",
        midi_bank: 0,
        midi_number: 1,
        values: [0., 0.],
    },
    ParameterProgram {
        id: ProgramId(3),
        name: "Bypass",
        midi_bank: 0,
        midi_number: 2,
        values: [1., 1.],
    },
];

/// Validate/borrow the fixed complete catalogue for preparation. Consumers can
/// copy a state and restore either native-precision processor under its existing
/// lifecycle rules. No current program identity is inferred from custom edits.
pub fn factory_program_bank() -> ProgramBank<'static, 2> {
    ProgramBank::new(crate::parameter_schema(), &GAIN_PROGRAMS)
        .expect("fixed complete gain programs")
}
