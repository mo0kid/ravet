//! Load only the trusted public sine bundle built by this workspace.
#[path = "../tests/support/mod.rs"]
mod support;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: validate <trusted RAVET Sine.clap>")?;
    let entry = loading::load(std::path::Path::new(&path))?;
    let count = support::exercise(&entry)?;
    println!(
        "PASS: sine CLAP actual-binary MIDI/state/audio checks, {count} blocks in three activation cycles"
    );
    Ok(())
}
#[allow(unsafe_code)]
mod loading {
    pub fn load(
        path: &std::path::Path,
    ) -> Result<clack_host::prelude::PluginEntry, Box<dyn std::error::Error>> {
        // SAFETY: this command accepts only the trusted reference built here;
        // Clack retains the loaded library for every entry/instance handle.
        Ok(unsafe { clack_host::prelude::PluginEntry::load(path)? })
    }
}
