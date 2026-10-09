//! Audited fixed public reference program metadata ABI, no live DSP access.
use std::{panic::catch_unwind, ptr};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeProgramInfo {
    pub id: u32,
    pub gain: f32,
    pub bypass: u32,
    pub midi_bank: u16,
    pub midi_number: u8,
    pub reserved: u8,
    pub name: [u8; 128],
}

#[unsafe(no_mangle)]
pub extern "C" fn ravet_gain_program_count() -> u32 {
    ravet_gain::GAIN_PROGRAMS.len() as u32
}

/// Copy one immutable complete program. No DSP handle or current identity.
///
/// # Safety
/// Output must be null or point to one live writable aligned NativeProgramInfo,
/// exclusively owned for this call, as specified in `include/RavetGainPrograms.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_gain_program_info(
    index: u32,
    output: *mut NativeProgramInfo,
) -> i32 {
    if output.is_null() {
        return 1;
    }
    let result = catch_unwind(|| {
        let bank = ravet_gain::factory_program_bank();
        let program = bank.programs().get(index as usize)?;
        let state = bank.state(index as usize)?;
        let mut name = [0; 128];
        if program.name.len() >= name.len() {
            return None;
        }
        name[..program.name.len()].copy_from_slice(program.name.as_bytes());
        Some(NativeProgramInfo {
            id: program.id.0,
            gain: state.values()[0],
            bypass: state.values()[1] as u32,
            midi_bank: program.midi_bank,
            midi_number: program.midi_number,
            reserved: 0,
            name,
        })
    });
    match result {
        Ok(Some(info)) => {
            unsafe {
                ptr::write(output, info);
            }
            0
        }
        Ok(None) => 1,
        Err(_) => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_metadata_atomic_rejection_and_repeated_heap_free_reads() {
        assert_eq!(ravet_gain_program_count(), 3);
        let mut info = NativeProgramInfo {
            id: 99,
            gain: 3.,
            bypass: 0,
            midi_bank: 4,
            midi_number: 2,
            reserved: 7,
            name: [42; 128],
        };
        let original = info;
        unsafe {
            assert_eq!(ravet_gain_program_info(3, &mut info), 1);
            assert_eq!(info, original);
            assert_eq!(ravet_gain_program_info(u32::MAX, &mut info), 1);
            assert_eq!(info, original);
            assert_eq!(ravet_gain_program_info(0, ptr::null_mut()), 1);
        }
        let counts = allocation_counter::measure(|| {
            for _ in 0..10_000 {
                for (index, (name, gain, bypass)) in
                    [("Unity", 1., 0), ("Silence", 0., 0), ("Bypass", 1., 1)]
                        .into_iter()
                        .enumerate()
                {
                    assert_eq!(
                        unsafe { ravet_gain_program_info(index as u32, &mut info) },
                        0
                    );
                    assert_eq!(
                        (info.id, info.gain, info.bypass),
                        (index as u32 + 1, gain, bypass)
                    );
                    assert_eq!(
                        (info.midi_bank, info.midi_number, info.reserved),
                        (0, index as u8, 0)
                    );
                    assert_eq!(&info.name[..name.len()], name.as_bytes());
                    assert!(info.name[name.len()..].iter().all(|b| *b == 0));
                }
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
    }
}
