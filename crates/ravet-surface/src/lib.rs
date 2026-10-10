//! Editors, controls, layout, and visualisation for RAVET.
//!
//! [`ParameterAttachment`] maps validated metadata to explicit UI edits through
//! a product [`ParameterControl`] transport and refreshes without feedback.
//! Portable [`layout`] geometry, [`drawing`] SVG and [`animation`] tweens are
//! explicit UI facilities; rendering/font selection belongs to the consumer.
//! The optional `text` feature adds local-font single-run shaping and measurement
//! and bounded paragraph wrapping, source caret/hit mapping and caller-buffer
//! selection geometry, plus bounded source editing/undo and prepared layout identity
//! on UI/worker threads. The independent optional `raster` feature adds owned CPU
//! path/stroke pixels; combining it with `text` adds prepared local-font glyph pixels.
//! The optional `image` feature owns bounded static PNG/JPEG pixels and PNG export.
//! Optional `jpeg-encode` adds explicit opaque baseline JPEG writing.
//! These allocating APIs do not create a window or establish callback deadlines.
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

#[cfg(feature = "text")]
pub mod paragraph;

#[cfg(feature = "text")]
pub mod caret;

#[cfg(feature = "text")]
pub mod editing;

#[cfg(feature = "raster")]
pub mod raster;

#[cfg(feature = "image")]
pub mod image;
