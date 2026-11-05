use crate::{
    Format, Project, Result,
    controller::{State, process},
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

pub(crate) fn preflight(p: &Project, format: Format) -> Result<()> {
    if !p.formats.contains(&format) {
        return Err("Select this export in project settings first".into());
    }
    if format == Format::Auv3 {
        return Err("AUv3 is not available for generic Foundry products".into());
    }
    Ok(())
}
fn command(root: &Path, p: &Project, format: Format) -> Command {
    let mut command = Command::new("python3");
    command
        .arg(p.framework_path.join("scripts/bundle-plugin.py"))
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .arg("--package")
        .arg(&p.name)
        .arg("--formats")
        .arg(format.name())
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"));
    for (flag, path) in [
        ("--vst3-sdk", &p.sdks.vst3),
        ("--audio-unit-sdk", &p.sdks.audio_unit),
        ("--aax-sdk", &p.sdks.aax),
        ("--lv2-sdk", &p.sdks.lv2),
    ] {
        if let Some(path) = path {
            command.arg(flag).arg(path);
        }
    }
    command
}
pub(crate) fn check(
    root: &Path,
    p: &Project,
    format: Format,
    state: &State,
    cancel: &AtomicBool,
) -> Result<()> {
    process(command(root, p, format).arg("--check"), state, cancel)
}
pub(crate) fn build(
    root: &Path,
    p: &Project,
    format: Format,
    release: bool,
    offline: bool,
    state: &State,
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    let profile = if release { "release" } else { "debug" };
    let output = root.join("target/plugins").join(profile);
    let mut command = command(root, p, format);
    command.arg("--output-dir").arg(&output);
    if release {
        command.arg("--release");
    }
    if offline {
        command.arg("--offline");
    }
    process(&mut command, state, cancel)?;
    let suffix = match format {
        Format::Au => "component",
        Format::Aax => "aaxplugin",
        _ => format.name(),
    };
    Ok(output.join(format!("{}.{suffix}", p.name)))
}
