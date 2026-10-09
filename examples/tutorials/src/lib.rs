//! Example processors and, on macOS, a live window built from RAVET components.
//!
//! The portable subtractive synth is distinct from the existing Sine CLAP export.
//! UI and device ownership remain outside its audio processing implementation.
pub mod delay;
pub mod synth;
#[cfg(all(target_os = "macos", feature = "live-ui"))]
pub mod ui;

/// Open an interactive example on macOS, or explain the current native UI limit.
pub fn launch(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(all(target_os = "macos", feature = "live-ui"))]
    {
        ui::run(mode)
    }
    #[cfg(not(all(target_os = "macos", feature = "live-ui")))]
    {
        Err(format!("{mode}: native UI currently requires macOS and the live-ui feature; offline fixtures remain available").into())
    }
}
