//! Narrow C reference ABI. Pointer validity is the consumer's obligation.
use super::{Gain, MAXIMUM_FRAMES};
use ravet_embedded::FixedBlockAdapter;
use ravet_foundation::{AudioLayout, ProcessConfig, Transport};

/// Initialize/reinitialize caller-owned state; returns 0 on success, -1 on rejection.
/// Rejection leaves state unchanged. Stop callbacks before initialization.
///
/// # Safety
/// `state` must be null or aligned writable storage for one Gain, exclusively
/// borrowed for this call. It need not previously have been initialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_embedded_gain_init(
    state: *mut Gain,
    sample_rate: f64,
    frames: u32,
) -> i32 {
    if state.is_null() {
        return -1;
    }
    let Ok(config) = ProcessConfig::new(sample_rate, frames as usize, 8, AudioLayout::STEREO)
    else {
        return -1;
    };
    let mut adapter = FixedBlockAdapter::<_, 2, 2, MAXIMUM_FRAMES>::new(Gain::new());
    if adapter.prepare(config).is_err() {
        return -1;
    }
    unsafe {
        state.write(*adapter.processor());
    }
    0
}

/// Set block-start gain; returns 0 on success, -1 on invalid value/null state.
///
/// # Safety
/// Nonnull `state` points to initialized Gain from init, exclusively borrowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_embedded_gain_set(state: *mut Gain, gain: f32) -> i32 {
    if state.is_null() {
        return -1;
    }
    if unsafe { &mut *state }.set_gain(gain) {
        0
    } else {
        -1
    }
}

/// Process interleaved stereo f32; exact input/output aliasing is allowed.
/// Returns 0 on success, -1 on rejection. Input/output lengths are bounded to
/// 128 samples. Valid output storage is silenced over its requested active prefix
/// on rejection; tails are preserved. Invalid output length/pointer returns
/// without writes. No allocation, panic recovery, driver or deadline guarantee.
///
/// # Safety
/// State is null or an initialized exclusively borrowed Gain. Nonempty input
/// and output spans must be aligned/live for their supplied lengths (<=128).
/// Output is writable and cannot overlap state. Input can overlap output but
/// cannot overlap state. No concurrent firmware/ISR access to any mutable span.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_embedded_gain_process(
    state: *mut Gain,
    input: *const f32,
    input_samples: usize,
    output: *mut f32,
    output_samples: usize,
    frames: usize,
) -> i32 {
    if output_samples > 128 || (output_samples != 0 && output.is_null()) {
        return -1;
    }
    // Copy input before creating a mutable output borrow: native in-place is legal.
    let mut copied = [0.0; 128];
    let valid_input = input_samples <= 128 && (input_samples == 0 || !input.is_null());
    if valid_input && input_samples != 0 {
        unsafe {
            core::ptr::copy_nonoverlapping(input, copied.as_mut_ptr(), input_samples);
        }
    }
    let output = if output_samples == 0 {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(output, output_samples) }
    };
    if state.is_null() || !valid_input {
        let active = frames.saturating_mul(2).min(output.len());
        output[..active].fill(0.0);
        return -1;
    }
    let state = unsafe { &mut *state };
    let mut adapter = FixedBlockAdapter::<_, 2, 2, MAXIMUM_FRAMES>::new(*state);
    let result = state
        .config()
        .is_some_and(|config| adapter.prepare(config).is_ok())
        && adapter
            .process_interleaved(
                &copied[..input_samples],
                output,
                frames,
                &[],
                Transport::default(),
            )
            .is_ok();
    if result {
        *state = *adapter.processor();
        0
    } else {
        let active = frames.saturating_mul(2).min(output.len());
        output[..active].fill(0.0);
        -1
    }
}
