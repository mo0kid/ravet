use super::*;
use core::cell::Cell;
fn x86(mask: u32, control: u64) -> FloatState {
    FloatState {
        backend: FloatBackend::X86_64,
        control,
        status: 0x25,
        supported_control_mask: Some(mask),
    }
}
fn arm(control: u64) -> FloatState {
    FloatState {
        backend: FloatBackend::Aarch64,
        control,
        status: 0x91,
        supported_control_mask: None,
    }
}
#[test]
fn complete_synthetic_mask_trap_and_alternate_handling_admission() {
    let flush = DenormalMode::FlushBoth;
    for mask in [0, 0xffbf, 1 << 6, 1 << 15] {
        assert_eq!(
            admitted_control(x86(mask, 0x1f80), flush),
            Err(DenormalError::UnsupportedControlMask)
        );
    }
    for bit in 7..=12 {
        assert_eq!(
            admitted_control(x86(0xffff, 0x1f80 & !(1 << bit)), flush),
            Err(DenormalError::UnsupportedExceptionMode)
        );
    }
    for round in 0..4 {
        let state = x86(0xffff, 0x1f80 | (round << 13));
        assert_eq!(admitted_control(state, flush), Ok(state.control | 0x8040));
    }
    assert_eq!(
        admitted_control(arm(2), flush),
        Err(DenormalError::UnsupportedAlternateHandling)
    );
    for bit in [8, 9, 10, 11, 12, 15] {
        assert_eq!(
            admitted_control(arm(1 << bit), flush),
            Err(DenormalError::UnsupportedExceptionMode)
        );
    }
    for round in 0..4 {
        let state = arm((round << 22) | (1 << 25) | (1 << 19));
        assert_eq!(
            admitted_control(state, flush),
            Ok(state.control | (1 << 24))
        );
    }
    for state in [arm(2 | (1 << 8)), x86(0, 0)] {
        assert_eq!(
            admitted_control(state, DenormalMode::Preserve),
            Ok(state.control)
        );
    }
}
#[test]
fn readback_failure_and_read_error_restore_exact_original_once() {
    for prior in [arm(1 << 25), x86(0xffff, 0x1f80)] {
        for read_error in [false, true] {
            let writes = Cell::new(0);
            let last = Cell::new(prior);
            let result = install_control(
                prior,
                DenormalMode::FlushBoth,
                || {
                    if read_error {
                        Err(DenormalError::UnsupportedArchitecture)
                    } else {
                        Ok(prior)
                    }
                },
                |state| {
                    writes.set(writes.get() + 1);
                    last.set(state);
                },
            );
            assert_eq!(
                result,
                Err(if read_error {
                    DenormalError::UnsupportedArchitecture
                } else {
                    DenormalError::ControlWriteRejected
                })
            );
            assert_eq!(writes.get(), 2);
            assert_eq!(last.get(), prior);
        }
    }
    let writes = Cell::new(0);
    let result = install_control(
        x86(0xffbf, 0x1f80),
        DenormalMode::FlushBoth,
        || panic!("rejected admission must not read back"),
        |_| writes.set(writes.get() + 1),
    );
    assert_eq!(result, Err(DenormalError::UnsupportedControlMask));
    assert_eq!(writes.get(), 0);
}
