//! Explicit trusted public global modulation fixture, no device or GUI.
#[path = "support/modulation_probe.rs"]
mod probe;
use clack_host::prelude::*;
use ravet_host::{ClapLibrary, HostRequests, RequestHost};
#[allow(unsafe_code)]
fn load(path: &std::path::Path) -> Result<ClapLibrary, ravet_host::HostError> {
    // SAFETY: explicit trusted compliant native fixture; this does not infer trust.
    unsafe { ClapLibrary::load(path) }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    let use64 = args.first().is_some_and(|arg| arg == "--f64");
    if use64 {
        args.remove(0);
    }
    if args.len() != 1 {
        return Err(
            "usage: hosted-modulation [--f64] <trusted public modulation-fixture library>".into(),
        );
    }
    let library = load(std::path::Path::new(&args[0]))?;
    let instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        library.entry(),
        c"org.ravet.host-modulation-fixture",
        &HostInfo::new("Offline modulation probe", "RAVET", "", "0.1")?,
    )?;
    if use64 {
        probe::run64(instance, 50, true, true, 2);
    } else {
        probe::run32(instance, 50, true, true, 2);
    }
    println!(
        "PASS 1000 native {} blocks/8000 frames, mapped modulation/base-value telemetry, parameter flush, zero measured audio heap actions and owner stop/main retirement",
        if use64 { "f64" } else { "f32" }
    );
    Ok(())
}
