//! Load only the trusted public sidechain bundle built by this workspace.
#[path = "../tests/support/mod.rs"]
mod support;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: validate <trusted RAVET Sidechain.clap>")?;
    let entry = loading::load(std::path::Path::new(&path))?;
    let blocks = support::exercise(&entry)?;
    println!(
        "PASS: sidechain native f32/f64 ports/audio/fault/heap checks, {blocks} blocks in three activation cycles"
    );
    Ok(())
}
#[allow(unsafe_code)]
mod loading {
    pub fn load(
        path: &std::path::Path,
    ) -> Result<clack_host::prelude::PluginEntry, Box<dyn std::error::Error>> {
        // SAFETY: this accepts only the trusted public reference built here;
        // Clack retains library ownership for every entry and instance handle.
        Ok(unsafe { clack_host::prelude::PluginEntry::load(path)? })
    }
}
