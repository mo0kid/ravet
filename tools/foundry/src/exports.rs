use crate::{
    Format, Project, Result,
    controller::{State, process},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

fn sdk(path: &Option<PathBuf>, name: &str, files: &[&str]) -> Result<PathBuf> {
    let path = path
        .as_ref()
        .ok_or_else(|| format!("Set the {name} SDK path in project settings"))?
        .canonicalize()?;
    for file in files {
        if !path.join(file).exists() {
            return Err(format!("{name} SDK is missing {file}").into());
        }
    }
    Ok(path)
}
pub(crate) fn preflight(p: &Project, format: Format) -> Result<()> {
    if !p.formats.contains(&format) {
        return Err("Select this export in project settings first".into());
    }
    if matches!(format, Format::Au | Format::Auv3) && !cfg!(target_os = "macos") {
        return Err("AU starters currently require macOS".into());
    }
    if format == Format::Aax && !cfg!(any(target_os = "macos", target_os = "windows")) {
        return Err("AAX requires macOS or Windows".into());
    }
    if matches!(format, Format::Vst3 | Format::Au | Format::Aax) {
        sdk(
            &p.sdks.vst3,
            "VST3",
            &[
                "CMakeLists.txt",
                "public.sdk/source/main/pluginfactory.h",
                "LICENSE.txt",
            ],
        )?;
    }
    if format == Format::Au {
        sdk(
            &p.sdks.audio_unit,
            "AudioUnit",
            &[
                "AudioUnitSDK.xcodeproj",
                "include/AudioUnitSDK/MusicDeviceBase.h",
                "LICENSE.txt",
            ],
        )?;
    }
    if format == Format::Aax {
        sdk(&p.sdks.aax, "AAX", &["Interfaces/AAX.h"])?;
    }
    if format == Format::Lv2 {
        sdk(&p.sdks.lv2, "LV2", &["include/lv2/core/lv2.h", "COPYING"])?;
    }
    Ok(())
}
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    if fs::symlink_metadata(source)?.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if fs::symlink_metadata(source)?.is_file() {
        fs::copy(source, destination)?;
    } else {
        return Err("Native bundles must contain regular files/directories".into());
    }
    Ok(())
}
fn find_bundle(root: &Path, name: &str) -> Result<PathBuf> {
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_name() == name {
                return Ok(entry.path());
            }
            if entry.file_type()?.is_dir() && entry.file_name() != "sdk-build" {
                pending.push(entry.path());
            }
        }
    }
    Err(format!("Native build did not produce {name}").into())
}
pub(crate) fn build(
    root: &Path,
    p: &Project,
    format: Format,
    release: bool,
    state: &State,
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    let profile = if release { "release" } else { "debug" };
    let config = if release { "Release" } else { "Debug" };
    let build = root.join("target/native").join(format.name()).join(profile);
    let stem = p.name.replace('-', "_");
    let archive = root
        .join("target")
        .join(profile)
        .join(if cfg!(target_os = "windows") {
            format!("{stem}.lib")
        } else {
            format!("lib{stem}.a")
        });
    let mut command = Command::new("cmake");
    command
        .arg("-S")
        .arg(root.join("native"))
        .arg("-B")
        .arg(&build)
        .arg(format!("-DCMAKE_BUILD_TYPE={config}"))
        .arg(format!("-DFOUNDRY_FORMAT={}", format.name()))
        .arg(format!("-DFOUNDRY_LIBRARY={}", archive.display()));
    for (key, path) in [
        ("VST3", &p.sdks.vst3),
        ("AAX", &p.sdks.aax),
        ("LV2", &p.sdks.lv2),
        ("AUDIOUNIT", &p.sdks.audio_unit),
    ] {
        let required = match key {
            "VST3" => matches!(format, Format::Vst3 | Format::Au | Format::Aax),
            "AAX" => format == Format::Aax,
            "LV2" => format == Format::Lv2,
            "AUDIOUNIT" => format == Format::Au,
            _ => false,
        };
        if let Some(path) = path.as_ref().filter(|_| required) {
            command.arg(format!(
                "-DFOUNDRY_{key}_SDK={}",
                path.canonicalize()?.display()
            ));
        }
    }
    if format == Format::Au {
        let source = p
            .sdks
            .audio_unit
            .as_ref()
            .ok_or("AudioUnit SDK is required")?
            .canonicalize()?;
        let output = root.join("target/audio-unit-sdk");
        process(
            Command::new("xcodebuild")
                .arg("-project")
                .arg(source.join("AudioUnitSDK.xcodeproj"))
                .args([
                    "-target",
                    "AudioUnitSDK",
                    "-configuration",
                    config,
                    "-jobs",
                    "2",
                ])
                .arg(format!("SYMROOT={}", output.display()))
                .arg(format!(
                    "OBJROOT={}",
                    root.join("target/audio-unit-sdk-objects").display()
                ))
                .arg(format!(
                    "ARCHS={}",
                    if std::env::consts::ARCH == "aarch64" {
                        "arm64"
                    } else {
                        "x86_64"
                    }
                ))
                .args([
                    "ONLY_ACTIVE_ARCH=YES",
                    "CODE_SIGNING_ALLOWED=NO",
                    "MACOSX_DEPLOYMENT_TARGET=12.0",
                ]),
            state,
            cancel,
        )?;
        command.arg(format!(
            "-DFOUNDRY_AUDIOUNIT_LIBRARY={}",
            output.join(config).join("libAudioUnitSDK.a").display()
        ));
    }
    process(&mut command, state, cancel)?;
    process(
        Command::new("cmake")
            .arg("--build")
            .arg(&build)
            .arg("--config")
            .arg(config)
            .arg("--target")
            .arg(format!("foundry_plugin_{}", format.name()))
            .args(["--parallel", "2"]),
        state,
        cancel,
    )?;
    let (native, suffix) = match format {
        Format::Vst3 => (find_bundle(&build, "foundry_plugin_vst3.vst3")?, "vst3"),
        Format::Au => (
            find_bundle(&build, "foundry_plugin_au.component")?,
            "component",
        ),
        Format::Aax => (
            find_bundle(&build, "foundry_plugin_aax.aaxplugin")?,
            "aaxplugin",
        ),
        Format::Auv3 => (find_bundle(&build, "RavetGainContainer.app")?, "app"),
        Format::Lv2 => (find_bundle(&build, "foundry_plugin_lv2.so")?, "lv2"),
        Format::Clap => return Err("CLAP uses the Rust build directly".into()),
    };
    let artifact = root
        .join("target/plugins")
        .join(profile)
        .join(format!("{}.{suffix}", p.name));
    if artifact.exists() {
        let metadata = fs::symlink_metadata(&artifact)?;
        if !metadata.is_dir() {
            return Err("Existing native output is not a regular directory".into());
        }
        fs::remove_dir_all(&artifact)?;
    }
    if format == Format::Lv2 {
        fs::create_dir_all(&artifact)?;
        fs::copy(native, artifact.join("foundry_plugin_lv2.so"))?;
        for name in ["manifest.ttl", "plugin.ttl"] {
            fs::copy(root.join(".foundry").join(name), artifact.join(name))?;
        }
    } else {
        copy_tree(&native, &artifact)?;
    }
    let resources = if format == Format::Lv2 {
        artifact.clone()
    } else {
        artifact.join("Contents/Resources")
    };
    fs::create_dir_all(resources.join("notices"))?;
    fs::copy(
        root.join("LICENSE-RAVET-TEMPLATE"),
        resources.join("notices/RAVET-LICENSE.txt"),
    )?;
    for (format_needed, sdk, license, name) in [
        (
            matches!(format, Format::Vst3 | Format::Au | Format::Aax),
            &p.sdks.vst3,
            "LICENSE.txt",
            "Steinberg-VST3-LICENSE.txt",
        ),
        (
            format == Format::Au,
            &p.sdks.audio_unit,
            "LICENSE.txt",
            "Apple-AudioUnitSDK-LICENSE.txt",
        ),
        (
            format == Format::Lv2,
            &p.sdks.lv2,
            "COPYING",
            "LV2-COPYING.txt",
        ),
    ] {
        if format_needed {
            fs::copy(
                sdk.as_ref().ok_or("Missing SDK")?.join(license),
                resources.join("notices").join(name),
            )?;
        }
    }
    if format == Format::Aax {
        fs::write(
            resources.join("notices/AAX-SDK-NOTICE.txt"),
            "Built with an externally supplied Avid AAX SDK. Review SDK header licences and your Avid agreement before distribution. Local development artifact; PACE signing is separate.\n",
        )?;
    }
    if format == Format::Au {
        let vst3 = find_bundle(&build, "foundry_plugin_vst3.vst3")?;
        copy_tree(&vst3, &resources.join("plugin.vst3"))?;
        sign(&resources.join("plugin.vst3"), state, cancel)?;
    }
    if format == Format::Auv3 {
        sign(
            &artifact.join("Contents/PlugIns/RavetGainExtension.appex"),
            state,
            cancel,
        )?;
    }
    if format != Format::Lv2 {
        sign(&artifact, state, cancel)?;
    }
    Ok(artifact)
}
fn sign(path: &Path, state: &State, cancel: &AtomicBool) -> Result<()> {
    if cfg!(target_os = "macos") {
        process(
            Command::new("codesign")
                .args(["--force", "--sign", "-"])
                .arg(path),
            state,
            cancel,
        )?;
    }
    Ok(())
}
