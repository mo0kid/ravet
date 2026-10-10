//! RAVET Foundry's shared project engine and local UI controller.
//!
//! Consumers remain outside the framework checkout. Creation does not build,
//! install plug-ins, open devices or use the network. Builds are explicit and
//! execute the consumer's trusted Cargo build scripts. Source files become
//! consumer-owned after creation; regeneration only changes tracked metadata.
mod controller;
mod directories;
pub use directories::{DirectoryRequest, DirectorySelection};
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod picker;

/// Main-thread Cocoa directory chooser for Foundry's native frontend.
/// Cancellation returns `None`. `probe` auto-cancels for the hidden UI fixture.
#[cfg(target_os = "macos")]
pub fn choose_directory(
    request: &DirectoryRequest,
    probe: bool,
) -> Result<Option<std::path::PathBuf>> {
    picker::choose(request, probe)
}
mod exports;
#[allow(unsafe_code)]
mod ffi;
mod native_templates;
mod project;
mod templates;
pub use controller::{Controller, Snapshot, doctor};
pub use project::{
    Format, Kind, PluginType, Project, Sdks, create, framework_root, load, regenerate,
};

/// Owned errors from validation, filesystem operations and build processes.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Self-contained Foundry document using RAVET controls and the shared theme.
pub fn document() -> String {
    ravet_surface::embed_web_controls(&include_str!("../web/index.html").replace(
        "<!-- RAVET_LOGO -->",
        include_str!("../../../media/ravet_LOGO.svg"),
    ))
    .expect("Foundry document has one controls marker")
}
