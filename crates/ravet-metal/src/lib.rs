//! Optional explicit macOS Metal offscreen drawing, outside portable DSP.
//! Native GPU creation/submission/readback belongs to UI/workers, never audio.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{MetalBitmap, MetalError, MetalImage, MetalPaint, MetalRectangle, MetalRenderer};
