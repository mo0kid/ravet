use crate::{PluginType, Project, Result};
use ravet_data::crypto::sha256;
use std::{collections::BTreeMap, fs};

pub(crate) fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn cmake(text: &str) -> String {
    text.replace('\\', "/")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace(';', "\\;")
}
fn uid(identity: &str, role: &str) -> String {
    sha256(format!("{identity}/{role}").as_bytes())[..16]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| format!("0x{:08X}", u32::from_be_bytes(*bytes)))
        .collect::<Vec<_>>()
        .join(", ")
}
fn code(identity: &str) -> String {
    sha256(identity.as_bytes())[..4]
        .iter()
        .map(|n| char::from(b'A' + n % 26))
        .collect()
}

pub(crate) fn metadata(p: &Project) -> Result<BTreeMap<String, String>> {
    let instrument = p.plugin_type == PluginType::Instrument;
    let root = p
        .framework_path
        .to_str()
        .ok_or("Framework path must be UTF-8")?;
    let mut files = BTreeMap::new();
    let mut header = String::from("// Managed by Foundry.\n#pragma once\n");
    for (key, value) in [
        ("NAME", p.display_name.clone()),
        ("VENDOR", p.company.clone()),
        ("VERSION", p.version.clone()),
        ("ID", p.plugin_id.clone()),
        ("LV2_URI", format!("urn:{}", p.plugin_id)),
    ] {
        header += &format!("#define FOUNDRY_{key} {}\n", serde_json::to_string(&value)?);
    }
    header += &format!(
        "#define FOUNDRY_PROCESSOR_ID {}\n#define FOUNDRY_CONTROLLER_ID {}\n",
        uid(&p.plugin_id, "processor"),
        uid(&p.plugin_id, "controller")
    );
    for key in ["MONO", "STEREO", "PRODUCT", "MANUFACTURER"] {
        let hash = sha256(format!("{}/{key}", p.plugin_id).as_bytes());
        header += &format!(
            "#define FOUNDRY_AAX_{key} 0x{:08X}\n",
            u32::from_be_bytes(hash[..4].try_into().expect("four bytes"))
        );
    }
    files.insert(".foundry/Product.hpp".into(), header);
    let config = include_str!("../native/plugin.cmake")
        .replace("@VERSION@", &p.version)
        .replace("@FRAMEWORK@", &cmake(root))
        .replace("@PLUGIN_ID@", &cmake(&p.plugin_id))
        .replace("@COMPANY@", &cmake(&p.company))
        .replace("@INSTRUMENT@", if instrument { "ON" } else { "OFF" })
        .replace(
            "@EDITOR@",
            if p.native_editor && !instrument {
                "ON"
            } else {
                "OFF"
            },
        );
    files.insert("native/CMakeLists.txt".into(), config);
    let au_reference = if instrument {
        "performance-au"
    } else {
        "gain-au"
    };
    let au_title = if instrument {
        "RAVET Performance Sine AU"
    } else {
        "RAVET Gain AU"
    };
    let title = if instrument {
        "RAVET Performance Sine"
    } else {
        "RAVET Gain"
    };
    let au = fs::read_to_string(
        p.framework_path
            .join(format!("examples/{au_reference}/Info.plist.in")),
    )?
    .replace(
        if instrument {
            "ravet_performance_au"
        } else {
            "ravet_gain_au"
        },
        "foundry_plugin_au",
    )
    .replace(
        if instrument {
            "org.ravet.performance.au"
        } else {
            "org.ravet.gain.au"
        },
        &xml(&format!("{}.au", p.plugin_id)),
    )
    .replace(au_title, &xml(&p.display_name))
    .replace(
        if instrument {
            "RAVET: Performance Sine"
        } else {
            "RAVET: Gain"
        },
        &xml(&format!("{}: {}", p.company, p.display_name)),
    )
    .replace(title, &xml(&p.display_name))
    .replace(
        if instrument { "RvPS" } else { "RvGn" },
        &code(&p.plugin_id),
    )
    .replace("Ravt", &code(&p.company))
    .replace("0.0.1", &p.version)
    .replace(
        "<integer>1</integer>",
        &format!("<integer>{}</integer>", au_version(&p.version)?),
    );
    files.insert(".foundry/AU.plist".into(), au);
    let reference = if instrument {
        "performance-lv2"
    } else {
        "gain-lv2"
    };
    let stem = if instrument { "performance" } else { "gain" };
    let uri = if instrument {
        "urn:ravet:performance-sine"
    } else {
        "urn:ravet:gain"
    };
    for (from, to) in [
        ("manifest.ttl", ".foundry/manifest.ttl"),
        (
            if instrument {
                "performance.ttl"
            } else {
                "gain.ttl"
            },
            ".foundry/plugin.ttl",
        ),
    ] {
        let source = fs::read_to_string(
            p.framework_path
                .join(format!("examples/{reference}/{from}")),
        )?
        .replace(uri, &format!("urn:{}", p.plugin_id))
        .replace(&format!("ravet_{stem}_lv2.so"), "foundry_plugin_lv2.so")
        .replace(&format!("{stem}.ttl"), "plugin.ttl")
        .replace(
            title,
            &p.display_name.replace('\\', "\\\\").replace('"', "\\\""),
        );
        files.insert(to.into(), source);
    }
    if !instrument {
        for (from, to) in [
            ("Info.plist.in", ".foundry/AUv3.plist"),
            ("Container.plist.in", ".foundry/Container.plist"),
        ] {
            let source =
                fs::read_to_string(p.framework_path.join(format!("examples/gain-auv3/{from}")))?
                    .replace("org.ravet.gain", &xml(&p.plugin_id))
                    .replace("Portable RAVET gain effect", &xml(&p.display_name))
                    .replace(
                        "RAVET: Gain",
                        &xml(&format!("{}: {}", p.company, p.display_name)),
                    )
                    .replace("RAVET Gain AudioUnit", &xml(&p.display_name))
                    .replace("RAVET Gain AUv3", &xml(&p.display_name))
                    .replace("RAVET Gain", &xml(&p.display_name))
                    .replace("RvG3", &code(&p.plugin_id))
                    .replace("Ravt", &code(&p.company))
                    .replace("0.0.1", &p.version)
                    .replace(
                        "<key>CFBundleVersion</key><string>1</string>",
                        &format!("<key>CFBundleVersion</key><string>{}</string>", p.version),
                    )
                    .replace(
                        "<key>version</key><integer>1</integer>",
                        &format!(
                            "<key>version</key><integer>{}</integer>",
                            au_version(&p.version)?
                        ),
                    );
            files.insert(to.into(), source);
        }
    }
    Ok(files)
}
fn au_version(version: &str) -> Result<u32> {
    let parts = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if parts[0] > 65535 || parts[1] > 255 || parts[2] > 255 {
        return Err("AU versions require major <= 65535, minor/patch <= 255".into());
    }
    Ok((parts[0] << 16) | (parts[1] << 8) | parts[2])
}
pub(crate) fn sources(p: &Project) -> Result<BTreeMap<String, String>> {
    let instrument = p.plugin_type == PluginType::Instrument;
    let mut files = BTreeMap::new();
    let source = fs::read_to_string(p.framework_path.join(if instrument {
        "examples/performance-vst3/src/performance.cpp"
    } else {
        "examples/gain-vst3/src/gain.cpp"
    }))?;
    let title = if instrument {
        "RAVET Performance Sine"
    } else {
        "RAVET Gain"
    };
    let source = source
        .lines()
        .map(|line| {
            if line.starts_with("const FUID processorID(") {
                "const FUID processorID(FOUNDRY_PROCESSOR_ID);".into()
            } else if line.starts_with("const FUID controllerID(") {
                "const FUID controllerID(FOUNDRY_CONTROLLER_ID);".into()
            } else {
                line.replace(
                    &format!("\"{title} Controller\""),
                    "FOUNDRY_NAME \" Controller\"",
                )
                .replace(&format!("\"{title}\""), "FOUNDRY_NAME")
                .replace("BEGIN_FACTORY(\"RAVET\"", "BEGIN_FACTORY(FOUNDRY_VENDOR")
                .replace("\"0.0.1\"", "FOUNDRY_VERSION")
            }
        })
        .collect::<Vec<String>>()
        .join("\n");
    files.insert(
        "native/plugin.cpp".into(),
        format!("#include \"Product.hpp\"\n{source}\n"),
    );
    let source = fs::read_to_string(p.framework_path.join(if instrument {
        "examples/performance-lv2/performance.cpp"
    } else {
        "examples/gain-lv2/gain.cpp"
    }))?;
    let uri = if instrument {
        "urn:ravet:performance-sine"
    } else {
        "urn:ravet:gain"
    };
    let mut source = source;
    for suffix in ["mono", "stereo", "state"] {
        source = source.replace(
            &format!("\"{uri}:{suffix}\""),
            &format!("FOUNDRY_LV2_URI \":{suffix}\""),
        );
    }
    files.insert(
        "native/lv2.cpp".into(),
        format!("#include \"Product.hpp\"\n{source}"),
    );
    if instrument {
        files.insert(
            "native/au.mm".into(),
            fs::read_to_string(
                p.framework_path
                    .join("examples/performance-au/PerformanceAU.mm"),
            )?,
        );
    } else {
        let source =
            fs::read_to_string(p.framework_path.join("examples/gain-aax/description.cpp"))?
                .replace("\"org.ravet.gain.mono\"", "FOUNDRY_ID \".mono\"")
                .replace("\"org.ravet.gain.stereo\"", "FOUNDRY_ID \".stereo\"")
                .replace("\"RAVET Gain\"", "FOUNDRY_NAME")
                .replace("\"RAVET\"", "FOUNDRY_VENDOR")
                .replace("0x52764731", "FOUNDRY_AAX_MONO")
                .replace("0x52764732", "FOUNDRY_AAX_STEREO")
                .replace("0x52617674", "FOUNDRY_AAX_MANUFACTURER")
                .replace("0x5276476e", "FOUNDRY_AAX_PRODUCT")
                .replace(
                    "0xAFF1AE72, 0xCC374810, 0x89136FEE, 0xF4348390",
                    "FOUNDRY_PROCESSOR_ID",
                );
        files.insert(
            "native/aax.cpp".into(),
            format!("#include \"Product.hpp\"\n{source}"),
        );
        for name in [
            "Container.cpp",
            "Extension.mm",
            "GainAudioUnit.h",
            "GainAudioUnit.mm",
            "Extension.entitlements",
        ] {
            files.insert(
                format!("native/auv3/{name}"),
                fs::read_to_string(p.framework_path.join(format!("examples/gain-auv3/{name}")))?,
            );
        }
    }
    Ok(files)
}
