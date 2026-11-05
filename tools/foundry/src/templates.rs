use crate::{FrameworkSource, Kind, PluginType, Project, Result};
use std::{collections::BTreeMap, fs, path::Path};

/// Git remote used for tag-sourced RAVET dependencies.
pub const FRAMEWORK_GIT: &str = "https://github.com/mo0kid/ravet";

fn quoted(value: &str) -> Result<String> {
    Ok(toml::Value::String(value.into()).to_string())
}
/// Release tag matching the selected checkout's workspace version.
pub fn framework_tag(framework: &Path) -> Result<String> {
    let manifest: toml::Table = read_manifest(&framework.join("Cargo.toml"))?.parse()?;
    let version = manifest
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(toml::Value::as_str)
        .ok_or("RAVET checkout has no workspace version")?;
    Ok(format!("v{version}"))
}
fn read_manifest(path: &Path) -> Result<String> {
    crate::project::read_text(path, 1 << 20)
}
/// Inline dependency table for one RAVET package. Every RAVET dependency in a
/// generated manifest uses the same source, so Cargo builds a single copy.
fn dep(project: &Project, relative: &str, extra: &str) -> Result<String> {
    Ok(match project.framework_source {
        FrameworkSource::Path => format!(
            "{{ path = {}{extra} }}",
            quoted(
                project
                    .framework_path
                    .join(relative)
                    .to_str()
                    .ok_or("Framework path must be UTF-8")?,
            )?
        ),
        FrameworkSource::Tag => format!(
            "{{ git = {}, tag = {}{extra} }}",
            quoted(FRAMEWORK_GIT)?,
            quoted(&framework_tag(&project.framework_path)?)?
        ),
    })
}
pub(crate) fn metadata(p: &Project) -> Result<BTreeMap<String, String>> {
    let mut manifest = format!(
        "# Managed by RAVET Foundry. Edit ravet-project.toml and regenerate.\n[package]\nname = {}\nversion = {}\nedition = \"2024\"\npublish = false\nlicense = {}\n\n[workspace]\n\n",
        quoted(&p.name)?,
        quoted(&p.version)?,
        quoted(&p.license)?
    );
    match p.kind {
        Kind::Offline => {
            manifest += &format!(
                "[dependencies]\nravet = {}\nravet-gain = {}\n",
                dep(p, "crates/ravet", "")?,
                dep(p, "examples/gain", "")?
            )
        }
        Kind::Standalone => {
            manifest += &format!(
                r#"[dependencies]
ravet-foundation = {}
ravet-signal = {}
ravet-surface = {}
ravet-instrument = {}
ravet-audio-file = {}

[features]
default = ["live-ui"]
live-ui = ["dep:ravet-webview", "dep:ravet-device", "dep:ravet-midi-device", "dep:serde_json", "dep:rtrb"]

[target.'cfg(target_os = "macos")'.dependencies]
ravet-webview = {}
ravet-device = {}
ravet-midi-device = {}
serde_json = {{ version = "=1.0.151", optional = true }}
rtrb = {{ version = "=0.4.0", optional = true }}
"#,
                dep(p, "crates/ravet-foundation", "")?,
                dep(p, "crates/ravet-signal", "")?,
                dep(p, "crates/ravet-surface", "")?,
                dep(p, "crates/ravet-instrument", "")?,
                dep(p, "crates/ravet-audio-file", "")?,
                dep(p, "crates/ravet-webview", ", optional = true")?,
                dep(p, "crates/ravet-device", ", optional = true")?,
                dep(p, "crates/ravet-midi-device", ", optional = true")?
            )
        }
        Kind::Plugin => {
            manifest += &format!(
                "[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n\n[features]\ndefault = [{}]\nnative-editor = []\n\n[dependencies]\nravet-foundation = {}\nravet-plugin = {}\n",
                if p.native_editor {
                    "\"native-editor\""
                } else {
                    ""
                },
                dep(p, "crates/ravet-foundation", "")?,
                dep(
                    p,
                    "crates/ravet-plugin",
                    ", features = [\"clap\", \"native\"]"
                )?
            );
        }
    }
    if p.kind != Kind::Offline {
        manifest += "\n[dev-dependencies]\nallocation-counter = \"=0.8.1\"\n";
    }
    manifest += "\n[lints.rust]\nunsafe_code = \"deny\"\n";
    // Values live in generated build metadata, keeping editor/DSP source consumer-owned.
    let mut build = String::from(
        "// Managed by RAVET Foundry.\nfn main() {\n    println!(\"cargo:rerun-if-changed=build.rs\");\n",
    );
    for (key, value) in [
        ("RAVET_PRODUCT_NAME", &p.display_name),
        ("RAVET_COMPANY", &p.company),
        ("RAVET_PLUGIN_ID", &p.plugin_id),
    ] {
        build += &format!("    println!(\"cargo:rustc-env={key}={{}}\", {value:?});\n");
    }
    build += "}\n";
    let mut files = BTreeMap::from([("Cargo.toml".into(), manifest), ("build.rs".into(), build)]);
    if p.kind == Kind::Plugin {
        let hash = ravet_data::crypto::sha256(p.plugin_id.as_bytes());
        files.insert(".foundry/product.rs".into(), format!(
            "// Managed by Foundry. Identity-derived saved-state tag.\nconst STATE_TAG: [u8; 8] = {:?};\n", &hash[..8]));
    }
    Ok(files)
}

fn copy_tree(
    root: &Path,
    prefix: &str,
    destination: &str,
    files: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in fs::read_dir(root.join(prefix))? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Template filename must be UTF-8")?;
        let from = format!("{prefix}/{name}");
        let to = format!("{destination}/{name}");
        if entry.file_type()?.is_dir() {
            copy_tree(root, &from, &to, files)?;
        } else if entry.file_type()?.is_file() {
            files.insert(to, fs::read_to_string(entry.path())?);
        } else {
            return Err("Template symlinks are not copied".into());
        }
    }
    Ok(())
}
pub(crate) fn sources(p: &Project) -> Result<BTreeMap<String, String>> {
    let root = &p.framework_path;
    let mut files = BTreeMap::new();
    match p.kind {
        Kind::Offline => {
            files.insert(
                "src/main.rs".into(),
                fs::read_to_string(root.join("templates/offline-gain/main.rs"))?,
            );
        }
        Kind::Plugin => {
            let instrument = p.plugin_type == PluginType::Instrument;
            for (name, destination) in [("lib.rs", "src/lib.rs"), ("dsp.rs", "src/dsp.rs")] {
                let source = fs::read_to_string(root.join("templates/plugin-generic").join(name))?
                    .replace("@INSTRUMENT@", if instrument { "true" } else { "false" })
                    .replace(
                        "@CATEGORY@",
                        if instrument { "Instrument" } else { "Effect" },
                    )
                    .replace(
                        "@LAYOUT@",
                        if instrument {
                            "STEREO_INSTRUMENT"
                        } else {
                            "STEREO"
                        },
                    )
                    .replace(
                        "@NOTES@",
                        if instrument {
                            ".with_notes(ravet_plugin::NotePorts { input: true, output: false })"
                        } else {
                            ""
                        },
                    );
                files.insert(destination.into(), source);
            }
            files.insert("editor/index.html".into(), "<!doctype html><html><head><meta charset=\"utf-8\"></head><body><label>Gain <input id=\"gain\" type=\"range\" min=\"0\" max=\"1\" step=\"0.001\"></label><script>const dial=document.getElementById('gain');dial.oninput=()=>window.ravetPlugin.set('gain',Number(dial.value));window.ravetPlugin.on('values',()=>{dial.value=window.ravetPlugin.value('gain')});</script></body></html>\n".into());
        }
        Kind::Standalone => {
            // Consumer-owned public reference: complete settings, audio/MIDI routing,
            // synth/delay DSP and offline native UI probe, with no device auto-start.
            for name in ["synth.rs", "delay.rs"] {
                files.insert(
                    format!("src/{name}"),
                    fs::read_to_string(root.join(format!("examples/tutorials/src/{name}")))?,
                );
            }
            copy_tree(root, "examples/tutorials/src/ui", "src/ui", &mut files)?;
            copy_tree(root, "examples/tutorials/editor", "editor", &mut files)?;
            let ui = files
                .get_mut("src/ui/mod.rs")
                .ok_or("Missing standalone template")?;
            *ui = ui.replace(
                "title: \"RAVET · Live examples\".into(),",
                "title: env!(\"RAVET_PRODUCT_NAME\").into(),",
            );
            // The reference probe predates connected-document control ownership.
            // Keep its synthetic knob attached for the same lifetime as real controls.
            *ui = ui.replace(
                "number.type='number';",
                "number.type='number';dial.style.cssText='position:fixed;left:-200px';number.style.cssText='position:fixed;left:-200px';document.body.append(dial,number);",
            ).replace(
                "denied.begin()||denied.active||!denied.dispose())return false;",
                "denied.begin()||denied.active||!denied.dispose())return false;dial.remove();number.remove();",
            );
            files.insert(
                "src/lib.rs".into(),
                fs::read_to_string(root.join("examples/tutorials/src/lib.rs"))?,
            );
            files.insert("src/main.rs".into(),format!("// Consumer-owned standalone reference. Edit delay.rs, synth.rs and editor/index.html.\nfn main() -> Result<(), Box<dyn std::error::Error>> {{\n    {}::launch(\"effect\")\n}}\n",p.name.replace('-',"_")));
        }
    }
    files.insert(
        ".gitignore".into(),
        "/target/\n/.foundry/operation.lock\n/.foundry/stage/\n".into(),
    );
    files.insert(
        "LICENSE-RAVET-TEMPLATE".into(),
        fs::read_to_string(root.join("LICENSE"))?,
    );
    files.insert("README.md".into(), format!("# {}\n\nCreated by RAVET Foundry. This is an independent consumer workspace.\n\nEdit DSP/editor source freely. Foundry regenerates Cargo.toml, build.rs and identity metadata;\nchanges to those files reject regeneration. Edit ravet-project.toml for product\nsettings, then use Foundry Regenerate. Package name and template remain fixed.\n\nBuild with Foundry or `cargo generate-lockfile` followed by `cargo build --locked`.\nKeep Cargo.lock. `framework_source` in ravet-project.toml selects how Cargo finds RAVET:\n`path` uses the local checkout (update it if RAVET moves); `tag` uses the release tag\nmatching the checkout's version. Bundle tooling/adapters always read the local checkout.\nFoundry Build packages the selected format locally. Native exports require CMake and external SDK paths in ravet-project.toml. Plug-in DSP lives in src/dsp.rs and PluginExport in src/lib.rs. Every format uses that same product through scripts/bundle-plugin.py in the selected RAVET checkout. SDK discovery also reads ravet-sdks.toml and RAVET_*_SDK environment variables.\nStandalone uses the public multi-mode synth/delay UI and default Audio/MIDI popup\non macOS. Devices start only from an explicit Start action. Other standalone\nplatform bindings remain pending.\n\nThe selected Cargo licence metadata does not relicense copied RAVET templates\nor dependencies. Copied reference source remains GPL-3.0-only under the included\nLICENSE-RAVET-TEMPLATE or separately agreed RAVET commercial terms.\n",p.display_name));
    Ok(files)
}
