//! Narrow register boundary: private inputs are either actual previously read
//! states or those same states with only proven supported flush bits changed.
//! No public arbitrary register setter, pointer or unchecked capability exists.
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
use crate::FloatBackend;
use crate::{DenormalError, FloatState};

#[cfg(target_arch = "x86_64")]
#[repr(C, align(16))]
struct SaveArea([u8; 512]);

pub(crate) fn read() -> Result<FloatState, DenormalError> {
    #[cfg(target_arch = "aarch64")]
    {
        let (control, status): (u64, u64);
        // SAFETY: user OS FP access is the declared native profile. Registers have
        // no borrowed storage; no pure/nomem assumptions cross policy boundaries.
        unsafe {
            core::arch::asm!("mrs {c}, fpcr", "mrs {s}, fpsr", c=out(reg) control,
                s=out(reg) status, options(nostack, preserves_flags));
        }
        Ok(FloatState {
            backend: FloatBackend::Aarch64,
            control,
            status,
            supported_control_mask: None,
        })
    }
    #[cfg(target_arch = "x86_64")]
    {
        let mut combined = 0u32;
        let mut area = SaveArea([0; 512]);
        // SAFETY: both live writable stack objects cover the complete instruction
        // write. FXSAVE has the required 16-byte alignment and 512-byte extent;
        // x86_64 OS SSE2 profile supplies FXSAVE. Neither instruction changes FP
        // state. Unwritten save bytes were pre-zeroed; only mask offset 28 is read.
        unsafe {
            core::arch::asm!("stmxcsr [{p}]", p=in(reg) &mut combined,
                options(nostack, preserves_flags));
            core::arch::asm!("fxsave [{p}]", p=in(reg) &mut area,
                options(nostack, preserves_flags));
        }
        let raw = u32::from_le_bytes(area.0[28..32].try_into().expect("fixed mask range"));
        let mask = if raw == 0 { 0x0000_ffbf } else { raw };
        Ok(FloatState {
            backend: FloatBackend::X86_64,
            control: (combined & !0x3f) as u64,
            status: (combined & 0x3f) as u64,
            supported_control_mask: Some(mask),
        })
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    Err(DenormalError::UnsupportedArchitecture)
}

pub(crate) fn write(state: FloatState) {
    #[cfg(target_arch = "aarch64")]
    // SAFETY: only actual read register values plus admitted FZ changes enter
    // here; exact saved status/control restoration is on the creating thread.
    unsafe {
        core::arch::asm!("msr fpcr, {c}", "msr fpsr, {s}", c=in(reg) state.control,
            s=in(reg) state.status, options(nostack, preserves_flags));
    }
    #[cfg(target_arch = "x86_64")]
    {
        let combined = (state.control | state.status) as u32;
        // SAFETY: actual read state and proven mask-supported additions only;
        // combined is a live 4-byte readable object. x87 is never modified.
        unsafe {
            core::arch::asm!("ldmxcsr [{p}]", p=in(reg) &combined,
                options(nostack, preserves_flags));
        }
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    let _ = state;
}

#[cfg(test)]
mod hardware_tests {
    use super::*;
    use crate::{DenormalMode, ScopedDenormalMode};
    #[test]
    fn actual_unrelated_control_and_sticky_status_restore_exactly() {
        let original = read().unwrap();
        let _restore_original = ScopedDenormalMode::enter(DenormalMode::Preserve).unwrap();
        let mut prior = original;
        match prior.backend {
            crate::FloatBackend::Aarch64 => {
                prior.control ^= 1 << 22; // supported rounding selector only
                prior.status |= 0x91; // cumulative exception flags, not trap enables
            }
            crate::FloatBackend::X86_64 => {
                prior.control ^= 1 << 13; // supported rounding selector only
                prior.status |= 0x25;
            }
        }
        write(prior);
        assert_eq!(read().unwrap(), prior);
        {
            let scope = ScopedDenormalMode::enter(DenormalMode::FlushBoth).unwrap();
            assert_eq!(scope.prior_state(), prior);
            let during = read().unwrap();
            assert!(during.input_flush() && during.result_flush());
            assert_eq!(during.status, prior.status);
            let mut changed = during;
            changed.status = 0;
            write(changed);
        }
        assert_eq!(read().unwrap(), prior);
    }
}
