use crate::{Kind, PluginType, Project, Result};
use std::{collections::BTreeMap, fs, path::Path};

fn quoted(value: &str) -> Result<String> {
    Ok(toml::Value::String(value.into()).to_string())
}
fn path(project: &Project, relative: &str) -> Result<String> {
    quoted(
        project
            .framework_path
            .join(relative)
            .to_str()
            .ok_or("Framework path must be UTF-8")?,
    )
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
                "[dependencies]\nravet = {{ path = {} }}\nravet-gain = {{ path = {} }}\n",
                path(p, "crates/ravet")?,
                path(p, "examples/gain")?
            )
        }
        Kind::Standalone => {
            manifest += &format!(
                r#"[dependencies]
ravet-foundation = {{ path = {} }}
ravet-signal = {{ path = {} }}
ravet-surface = {{ path = {} }}
ravet-instrument = {{ path = {} }}
ravet-audio-file = {{ path = {} }}

[features]
default = ["live-ui"]
live-ui = ["dep:ravet-webview", "dep:ravet-device", "dep:ravet-midi-device", "dep:serde_json", "dep:rtrb"]

[target.'cfg(target_os = "macos")'.dependencies]
ravet-webview = {{ path = {}, optional = true }}
ravet-device = {{ path = {}, optional = true }}
ravet-midi-device = {{ path = {}, optional = true }}
serde_json = {{ version = "=1.0.151", optional = true }}
rtrb = {{ version = "=0.4.0", optional = true }}
"#,
                path(p, "crates/ravet-foundation")?,
                path(p, "crates/ravet-signal")?,
                path(p, "crates/ravet-surface")?,
                path(p, "crates/ravet-instrument")?,
                path(p, "crates/ravet-audio-file")?,
                path(p, "crates/ravet-webview")?,
                path(p, "crates/ravet-device")?,
                path(p, "crates/ravet-midi-device")?
            )
        }
        Kind::Plugin => {
            manifest += &format!(
                r#"[lib]
crate-type = ["cdylib", "rlib", "staticlib"]

[features]
default = [{}]
native-editor = ["dep:ravet-webview", "dep:serde_json", "clack-extensions/gui", "ravet-gain-native/native-editor"]

[dependencies]
ravet-foundation = {{ path = {} }}
ravet-surface = {{ path = {} }}
ravet-gain = {{ path = {} }}
ravet-clap = {{ path = {} }}
clack-plugin = "=0.2.0"
clack-extensions = {{ version = "=0.2.0", features = ["clack-plugin", "audio-ports", "audio-ports-config", "params", "state"] }}

[target.'cfg(target_os = "macos")'.dependencies]
ravet-webview = {{ path = {}, optional = true }}
serde_json = {{ version = "=1.0.151", optional = true }}
"#,
                if p.native_editor {
                    "\"native-editor\""
                } else {
                    ""
                },
                path(p, "crates/ravet-foundation")?,
                path(p, "crates/ravet-surface")?,
                path(p, "examples/gain")?,
                path(p, "crates/ravet-clap")?,
                path(p, "crates/ravet-webview")?
            );
            let native = path(p, "examples/gain-native")?;
            manifest = manifest.replace(
                "clack-plugin = \"=0.2.0\"",
                &format!("ravet-gain-native = {{ path = {native} }}\nclack-plugin = \"=0.2.0\""),
            );
            if p.plugin_type == PluginType::Instrument {
                manifest = format!(
                    "# Managed by RAVET Foundry.\n[package]\nname = {}\nversion = {}\nedition = \"2024\"\npublish = false\nlicense = {}\n\n[workspace]\n\n[lib]\ncrate-type = [\"cdylib\", \"rlib\", \"staticlib\"]\n\n[dependencies]\nravet-foundation = {{ path = {} }}\nravet-instrument = {{ path = {} }}\nravet-clap = {{ path = {} }}\nravet-performance-native = {{ path = {} }}\nclack-plugin = \"=0.2.0\"\nclack-extensions = {{ version = \"=0.2.0\", features = [\"clack-plugin\", \"audio-ports\", \"note-ports\", \"params\", \"state\", \"tail\"] }}\n",
                    quoted(&p.name)?,
                    quoted(&p.version)?,
                    quoted(&p.license)?,
                    path(p, "crates/ravet-foundation")?,
                    path(p, "crates/ravet-instrument")?,
                    path(p, "crates/ravet-clap")?,
                    path(p, "examples/performance-native")?
                );
            }
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
        files.extend(crate::native_templates::metadata(p)?);
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
            if p.plugin_type == PluginType::Instrument {
                let source = fs::read_to_string(root.join("examples/sine-clap/src/lib.rs"))?
                    .replace(
                        "pub const PLUGIN_ID: &str = \"org.ravet.sine\";",
                        "pub const PLUGIN_ID: &str = env!(\"RAVET_PLUGIN_ID\");",
                    )
                    .replace(
                        "PluginDescriptor::new(PLUGIN_ID, \"RAVET Sine\")",
                        "PluginDescriptor::new(PLUGIN_ID, env!(\"RAVET_PRODUCT_NAME\"))",
                    )
                    .replace(
                        ".with_vendor(\"RAVET\")",
                        ".with_vendor(env!(\"RAVET_COMPANY\"))",
                    );
                files.insert("src/lib.rs".into(), source + "\n// Native instrument exports used by the other format adapters.\npub use ravet_performance_native::*;\n");
            } else {
                copy_tree(root, "examples/gain-clap/src", "src", &mut files)?;
                copy_tree(root, "examples/gain-clap/editor", "editor", &mut files)?;
                files.remove("src/control_tests.rs");
                let source = files.get_mut("src/lib.rs").ok_or("Missing CLAP template")?;
                *source = source
                    .replace(
                        "pub const PLUGIN_ID: &str = \"org.ravet.gain\";",
                        "pub const PLUGIN_ID: &str = env!(\"RAVET_PLUGIN_ID\");",
                    )
                    .replace(
                        "PluginDescriptor::new(PLUGIN_ID, \"RAVET Gain\")",
                        "PluginDescriptor::new(PLUGIN_ID, env!(\"RAVET_PRODUCT_NAME\"))",
                    )
                    .replace(
                        ".with_vendor(\"RAVET\")",
                        ".with_vendor(env!(\"RAVET_COMPANY\"))",
                    )
                    .replace(
                        "#[cfg(test)]\n#[allow(unsafe_code)]\nmod control_tests;",
                        "",
                    );
                source.push_str("\n// Native effect exports used by the other format adapters.\npub use ravet_gain_native::*;\n");
            }
            files.extend(crate::native_templates::sources(p)?);
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
    files.insert("README.md".into(), format!("# {}\n\nCreated by RAVET Foundry. This is an independent consumer workspace.\n\nEdit DSP/editor source freely. Foundry regenerates Cargo.toml, build.rs and native build/product metadata;\nchanges to those files reject regeneration. Edit ravet-project.toml for product\nsettings, then use Foundry Regenerate. Package name and template remain fixed.\n\nBuild with Foundry or `cargo generate-lockfile` followed by `cargo build --locked`.\nKeep Cargo.lock. Local framework paths must be updated if RAVET moves.\nFoundry Build packages the selected format locally. Native exports require CMake and external SDK paths in ravet-project.toml. Native adapters and CLAP use the public gain/sine references; update each adapter's processor binding when replacing the reference DSP.\nStandalone uses the public multi-mode synth/delay UI and default Audio/MIDI popup\non macOS. Devices start only from an explicit Start action. Other standalone\nplatform bindings remain pending.\n\nThe selected Cargo licence metadata does not relicense copied RAVET templates\nor dependencies. Copied reference source remains GPL-3.0-only under the included\nLICENSE-RAVET-TEMPLATE or separately agreed RAVET commercial terms.\n",p.display_name));
    Ok(files)
}
