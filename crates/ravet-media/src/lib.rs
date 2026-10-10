//! Explicit bounded local-video worker services, outside portable DSP.
//! macOS APIs use AVFoundation and require Xcode; no device/network/window is opened.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{VideoError, VideoFrame, VideoInfo, VideoReader};
