//! Main-thread Cocoa/WebKit ownership for embedded macOS editors.
//!
//! Native code targets macOS 12 or later and requires Xcode command-line tools.
//! This crate is separate from portable DSP and the umbrella dependency graph.
//! Browser context menus are disabled in all loaded frames. Right-click events
//! still reach page controls for reset actions and custom menus.
//! `ContentConstraints` and checked explicit native semantic models are portable.
//! The opt-in companion does not inspect HTML or certify assistive technologies.
//! On other platforms the native API is unavailable. See `docs/native-editor.md`.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{Callbacks, WebView, WebViewError, is_main_thread};
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod editor_abi;

/// Retain this crate and its native editor ABI in a consumer static library.
pub fn link() {}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod standalone;
#[cfg(target_os = "macos")]
pub use standalone::{
    StandaloneEditor, StandaloneOptions, StandaloneWindow, StandaloneWindowState,
};

mod content_constraints;
pub use content_constraints::ContentConstraints;

/// Checked explicit native semantic models for custom editor content.
pub mod accessibility;

/// Explicit opt-in copied catalogue and native system font service.
#[cfg(feature = "system-fonts")]
pub mod fonts;
