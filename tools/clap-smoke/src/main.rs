use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: ravet-clap-smoke <trusted RAVET Gain CLAP bundle>")?;
    let entry = loading::load_trusted_ravet_bundle(&path)?;
    let blocks = ravet_clap_smoke::exercise(&entry)?;
    println!(
        "RAVET CLAP Smoke Host: passed {blocks} blocks, mono/stereo, automation, parameter text, inactive/active flush, reset, active state recall, and six activation cycles."
    );
    Ok(())
}

#[allow(unsafe_code)]
mod loading {
    use clack_host::prelude::PluginEntry;
    use std::path::Path;
    pub(super) fn load_trusted_ravet_bundle(
        path: &Path,
    ) -> Result<PluginEntry, Box<dyn std::error::Error>> {
        // SAFETY: this development command is for the trusted RAVET library
        // built by this workspace. Loading arbitrary/noncompliant native code
        // is outside its contract. Clack owns the loaded entry and keeps the
        // library alive for every plugin instance/handle derived from it.
        Ok(unsafe { PluginEntry::load(path)? })
    }
}
