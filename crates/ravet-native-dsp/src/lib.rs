//! Explicit current-thread native numerical policy, independent of portable DSP defaults.
//!
//! No automatic thread policy changes occur. Supported profiles are ordinary OS
//! user-mode AArch64 FP/Advanced SIMD and x86_64 SSE2 with actual DAZ/FTZ admission.
//! Register access assumes the OS enables user floating-point instructions; a CPU
//! architecture cfg does not establish privileged/bare-metal/sandbox access.
//! Policy affects runtime executed instructions, not every optimized Rust source
//! expression or external math function. This is not a deadline guarantee.
#![cfg_attr(not(feature = "std"), no_std)]

#[allow(unsafe_code)]
mod native;

#[cfg(feature = "processor")]
mod processor;
#[cfg(feature = "processor")]
pub use processor::{DenormalConstructionFailure, DenormalProcessor};

/// Native register family; this does not identify physical CPU provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloatBackend {
    Aarch64,
    X86_64,
}
/// Explicit scope policy. Preserve retains the actual current control bits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DenormalMode {
    Preserve,
    /// Flush f32/f64 denormal operands and results after actual capability admission.
    FlushBoth,
}
/// Read-only actual thread state. Control/status separation is logical on x86_64.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloatState {
    pub backend: FloatBackend,
    pub control: u64,
    pub status: u64,
    /// Actual MXCSR_MASK (zero hardware mask uses the specified 0xffbf fallback).
    /// AArch64 has no corresponding mask in this profile.
    pub supported_control_mask: Option<u32>,
}
impl FloatState {
    /// Observed standard f32/f64 operand-flush profile. AArch64 AH=1 is
    /// deliberately not interpreted as the standard AH=0 profile.
    pub fn input_flush(self) -> bool {
        match self.backend {
            FloatBackend::X86_64 => self.control & (1 << 6) != 0,
            FloatBackend::Aarch64 => self.control & (1 << 24) != 0 && self.control & 2 == 0,
        }
    }
    /// Observed standard f32/f64 result flush profile (x86 underflow is masked).
    /// AH=1 is intentionally not
    /// interpreted as the standard profile accepted by this crate.
    pub fn result_flush(self) -> bool {
        match self.backend {
            FloatBackend::X86_64 => self.control & (1 << 15) != 0 && self.control & (1 << 11) != 0,
            FloatBackend::Aarch64 => self.control & (1 << 24) != 0 && self.control & 2 == 0,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DenormalError {
    UnsupportedArchitecture,
    UnsupportedControlMask,
    UnsupportedAlternateHandling,
    UnsupportedExceptionMode,
    ControlWriteRejected,
}
impl core::fmt::Display for DenormalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::UnsupportedArchitecture => "native floating-control profile is unavailable",
            Self::UnsupportedControlMask => {
                "actual MXCSR mask does not support both flush controls"
            }
            Self::UnsupportedAlternateHandling => {
                "alternate floating-point handling does not support this profile"
            }
            Self::UnsupportedExceptionMode => {
                "floating exception environment does not support this profile"
            }
            Self::ControlWriteRejected => "native floating-control write was not accepted",
        })
    }
}
impl core::error::Error for DenormalError {}

/// Read actual current-thread state without changing it. On x86_64 this performs
/// one bounded, aligned 512-byte FXSAVE to inspect the supported control mask.
pub fn current_float_state() -> Result<FloatState, DenormalError> {
    native::read()
}

/// Thread-bound RAII scope restoring exact prior control AND sticky status.
///
/// Scope-created cumulative exception flags are isolated, not merged into the
/// caller's observations. Nested scopes restore lexically. Other code on this
/// thread inside the scope observes the selected environment. There is no heap,
/// lock or I/O; x86_64 entry includes up to two bounded 512-byte FXSAVE capability reads when the control changes.
/// Forget, abort and foreign longjmp can prevent restoration. This type cannot
/// be transferred or shared between threads.
///
/// ```compile_fail
/// fn send<T: Send>() {}
/// send::<ravet_native_dsp::ScopedDenormalMode>();
/// ```
/// ```compile_fail
/// fn sync<T: Sync>() {}
/// sync::<ravet_native_dsp::ScopedDenormalMode>();
/// ```
pub struct ScopedDenormalMode {
    prior: FloatState,
    _thread: core::marker::PhantomData<*mut ()>,
}
impl ScopedDenormalMode {
    /// Refuse unsupported actual environments before changing state. A rejected
    /// write/read-back restores the complete prior state before returning Err.
    pub fn enter(mode: DenormalMode) -> Result<Self, DenormalError> {
        let prior = native::read()?;
        install_control(prior, mode, native::read, native::write)?;
        Ok(Self {
            prior,
            _thread: core::marker::PhantomData,
        })
    }
    pub fn prior_state(&self) -> FloatState {
        self.prior
    }
}
impl Drop for ScopedDenormalMode {
    fn drop(&mut self) {
        native::write(self.prior);
    }
}
fn install_control(
    prior: FloatState,
    mode: DenormalMode,
    mut read: impl FnMut() -> Result<FloatState, DenormalError>,
    mut write: impl FnMut(FloatState),
) -> Result<(), DenormalError> {
    let control = admitted_control(prior, mode)?;
    if control != prior.control {
        write(FloatState { control, ..prior });
        match read() {
            Ok(actual) if actual.control == control => {}
            Ok(_) => {
                write(prior);
                return Err(DenormalError::ControlWriteRejected);
            }
            Err(error) => {
                write(prior);
                return Err(error);
            }
        }
    }
    Ok(())
}
fn admitted_control(state: FloatState, mode: DenormalMode) -> Result<u64, DenormalError> {
    if mode == DenormalMode::Preserve {
        return Ok(state.control);
    }
    match state.backend {
        FloatBackend::X86_64 => {
            let supported = state.supported_control_mask.unwrap_or(0);
            let needed = (1 << 6) | (1 << 15);
            if supported & needed != needed {
                return Err(DenormalError::UnsupportedControlMask);
            }
            if state.control & 0x1f80 != 0x1f80 {
                return Err(DenormalError::UnsupportedExceptionMode);
            }
            Ok(state.control | needed as u64)
        }
        FloatBackend::Aarch64 => {
            if state.control & 2 != 0 {
                return Err(DenormalError::UnsupportedAlternateHandling);
            }
            if state.control & ((1 << 15) | (0x1f << 8)) != 0 {
                return Err(DenormalError::UnsupportedExceptionMode);
            }
            Ok(state.control | (1 << 24))
        }
    }
}

#[cfg(test)]
mod tests;

mod packed;
pub use packed::{F32x4, F64x2, LaneMask2, LaneMask4, VectorBackend, VectorError};
#[allow(unsafe_code)]
mod packed_native;
mod vector_ops;
pub use vector_ops::{PreparedVectorOps32, PreparedVectorOps64};
