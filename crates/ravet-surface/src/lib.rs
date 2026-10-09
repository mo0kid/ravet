//! Editors, controls, layout, and visualisation for RAVET.
//!
//! [`ParameterAttachment`] maps validated metadata to explicit UI edits through
//! a product [`ParameterControl`] transport and refreshes without feedback.
//! Portable [`layout`] geometry, [`drawing`] SVG and [`animation`] tweens are
//! explicit UI facilities; rendering/font selection belongs to the consumer.
//! The optional `text` feature adds local-font single-run shaping and measurement
//! on UI/worker threads; it adds no paragraph layout or raster backend.
//! Editors must be independently creatable and destroyable while processing
//! continues. Parameter changes and metering will communicate through bounded
//! channels with explicit overflow behaviour. Platform windowing and rendering
//! backends remain provisional. The independently built `native/` prototypes use
//! Cocoa/WebKit, Windows WebView2, or Linux X11/WebKitGTK to embed local HTML and
//! exchange JSON on the host UI thread. The explicit
//! `ravet-webview` crate now supplies Cargo/native ownership integration outside
//! the portable umbrella graph; see `docs/native-editor.md`.

mod attachment;
pub use attachment::{AttachmentError, ParameterAttachment, ParameterControl};
mod web_controls;
pub use web_controls::{WEB_CONTROLS_SCRIPT, WEB_THEME_STYLE, embed_web_controls};
pub mod animation;
pub mod drawing;
pub mod layout;

#[cfg(feature = "text")]
pub mod text;

#[cfg(feature = "text")]
pub mod bidi;
