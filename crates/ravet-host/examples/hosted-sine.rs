//! Explicit trusted public sine reference, with synthetic offline output only.
#[path = "support/sine_graph_probe.rs"]
mod probe;
use clack_host::prelude::*;
use ravet_host::{ClapLibrary, HostRequests, RequestHost};
#[allow(unsafe_code)]
fn load(path: &std::path::Path) -> Result<ClapLibrary, ravet_host::HostError> {
    // SAFETY: explicitly selected trusted compliant CLAP binary, no trust inference.
    unsafe { ClapLibrary::load(path) }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        return Err("usage: hosted-sine <trusted RAVET Sine CLAP library>".into());
    }
    let library = load(std::path::Path::new(&args[0]))?;
    let instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        library.entry(),
        c"org.ravet.sine",
        &HostInfo::new("Offline sine node probe", "RAVET", "", "0.1")?,
    )?;
    probe::run(instance)
}
