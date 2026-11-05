//! Main-thread native web views for embedded editors and standalone windows:
//! Cocoa/WebKit on macOS, Win32/WebView2 on Windows and GTK 3/WebKitGTK 4.1
//! (X11 or XWayland) on Linux.
//!
//! macOS native code targets macOS 12 or later and requires Xcode command-line
//! tools. Windows builds need `RAVET_WEBVIEW2_SDK` (an extracted WebView2 NuGet
//! package); MSVC builds link its `WebView2LoaderStatic.lib`, while GNU builds
//! ship its `WebView2Loader.dll` beside the consuming binary. Linux
//! builds need the GTK 3, WebKitGTK 4.1 and JSON-GLib development packages, and
//! hosts must call [`pump`] from their UI loop. This crate is separate from
//! portable DSP and the umbrella dependency graph.
//! [`WebView::from_html_with_assets`] serves embedded [`Asset`]s to the page
//! from a local origin, beyond the 1 MiB page and 64 KiB message limits.
//! Browser context menus are disabled in all loaded frames. Right-click events
//! still reach page controls for reset actions and custom menus.
//! `ContentConstraints` and checked explicit native semantic models are portable;
//! the native accessibility companion is macOS only. See `docs/native-editor.md`.
#![deny(clippy::undocumented_unsafe_blocks)]
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[allow(unsafe_code)]
mod native;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use native::{
    Asset, Callbacks, MAXIMUM_ASSET_BYTES, MAXIMUM_ASSETS, MAXIMUM_TOTAL_ASSET_BYTES, WebView,
    WebViewError, assets_valid, is_main_thread, pump,
};
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod editor_abi;

/// Retain this crate and its native editor ABI in a consumer static library.
pub fn link() {}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[allow(unsafe_code)]
mod standalone;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
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
