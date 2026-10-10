use crate::{Result, templates};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

/// Available consumer templates. Standalone currently uses the macOS Rust window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Offline,
    Standalone,
    #[serde(alias = "clap")]
    Plugin,
}

/// Processing role selected for a plug-in project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    #[default]
    Effect,
    Instrument,
}

/// Available RAVET export formats. SDKs and target validation remain explicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    Clap,
    Vst3,
    Au,
    Auv3,
    Aax,
    Lv2,
}
impl Format {
    /// Stable CLI/settings spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Clap => "clap",
            Self::Vst3 => "vst3",
            Self::Au => "au",
            Self::Auv3 => "auv3",
            Self::Aax => "aax",
            Self::Lv2 => "lv2",
        }
    }
    /// Parse an explicit export format; VST2 is excluded.
    pub fn parse(text: &str) -> Result<Self> {
        match text {
            "clap" => Ok(Self::Clap),
            "vst3" => Ok(Self::Vst3),
            "au" => Ok(Self::Au),
            "auv3" => Ok(Self::Auv3),
            "aax" => Ok(Self::Aax),
            "lv2" => Ok(Self::Lv2),
            _ => Err("Unknown plug-in format".into()),
        }
    }
}
fn default_formats() -> Vec<Format> {
    vec![Format::Clap]
}

/// External SDK paths used only for selected consumer builds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sdks {
    /// External Steinberg VST3 SDK, also needed for AU/AAX wrapper builds.
    pub vst3: Option<PathBuf>,
    /// External Apple AudioUnitSDK source for AUv2.
    pub audio_unit: Option<PathBuf>,
    /// External Avid AAX SDK source; no SDK redistribution.
    pub aax: Option<PathBuf>,
    /// External LV2 source SDK with include/lv2 and COPYING.
    pub lv2: Option<PathBuf>,
}

/// Version-one human-readable project settings. Unknown fields are rejected.
/// Name and kind are immutable after creation; source/template changes are explicit
/// consumer work. Other settings can be regenerated without rewriting source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// Settings format version; currently exactly one.
    pub schema_version: u32,
    /// Lowercase ASCII Cargo package name, 1..64 bytes.
    pub name: String,
    /// Product title, 1..128 UTF-8 bytes, without control characters.
    pub display_name: String,
    /// Three numeric semantic-version components (prereleases are not yet exposed).
    pub version: String,
    /// Product vendor, 1..128 UTF-8 bytes, without control characters.
    pub company: String,
    /// Explicit Cargo licence metadata; dependency/template licences remain unchanged.
    pub license: String,
    /// Reverse-domain ASCII identity; derives native class IDs and bundle identities.
    pub plugin_id: String,
    /// Initial template; cannot be changed by regeneration.
    pub kind: Kind,
    /// Absolute path to a local RAVET checkout, canonicalized during generation.
    pub framework_path: PathBuf,
    /// Optional macOS effect editor for CLAP/VST3/AU; other editors are format-owned.
    pub native_editor: bool,
    /// Effect or instrument reference. Immutable after creation.
    #[serde(default)]
    pub plugin_type: PluginType,
    /// Selected exports; all adapters are copied once so this can be changed safely.
    #[serde(default = "default_formats")]
    pub formats: Vec<Format>,
    /// Consumer SDK locations. Missing SDKs reject the requested build before Cargo.
    #[serde(default)]
    pub sdks: Sdks,
}

/// Framework checkout used to compile Foundry. Override it in project settings
/// when running a binary built elsewhere; source templates require a checkout.
pub fn framework_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

impl Project {
    /// Default GPL offline project; callers still choose its name/destination.
    pub fn new(name: &str, kind: Kind) -> Self {
        Self {
            schema_version: 1,
            name: name.into(),
            display_name: name.into(),
            version: "0.1.0".into(),
            company: "Your company".into(),
            license: "GPL-3.0-only".into(),
            plugin_id: format!("com.example.{}", name.replace('_', "-")),
            kind,
            framework_path: framework_root(),
            native_editor: true,
            plugin_type: PluginType::Effect,
            formats: default_formats(),
            sdks: Sdks::default(),
        }
    }
    /// Validate settings and resolve the selected local framework path.
    pub fn validate(&mut self) -> Result<()> {
        if self.schema_version != 1 {
            return Err("Unsupported Foundry settings version".into());
        }
        if self.kind == Kind::Plugin {
            let unique: std::collections::BTreeSet<_> = self.formats.iter().collect();
            if self.formats.is_empty() || unique.len() != self.formats.len() {
                return Err("Select at least one distinct plug-in format".into());
            }
            if self.plugin_type == PluginType::Instrument
                && self
                    .formats
                    .iter()
                    .any(|f| matches!(f, Format::Aax | Format::Auv3))
            {
                return Err(
                    "Instrument starter adapters are available for CLAP, VST3, AU and LV2".into(),
                );
            }
        }
        let mut chars = self.name.bytes();
        if !(1..=64).contains(&self.name.len())
            || !chars.next().is_some_and(|c| c.is_ascii_lowercase())
            || !chars
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
        {
            return Err("Package name must be 1..64 lowercase ASCII Cargo characters".into());
        }
        if [
            "self", "super", "crate", "mod", "fn", "lib", "test", "build", "target", "std", "core",
            "alloc",
        ]
        .contains(&self.name.as_str())
        {
            return Err("Choose a non-reserved package name".into());
        }
        for (name, value, maximum) in [
            ("Product name", &self.display_name, 128),
            ("Company", &self.company, 128),
            ("Licence", &self.license, 256),
        ] {
            if value.trim().is_empty()
                || value.len() > maximum
                || value.chars().any(char::is_control)
            {
                return Err(format!("{name} must be nonempty, at most {maximum} bytes and have no control characters").into());
            }
        }
        let parts: Vec<_> = self.version.split('.').collect();
        if parts.len() != 3
            || parts.iter().any(|p| {
                p.is_empty()
                    || (p.len() > 1 && p.starts_with('0'))
                    || p.parse::<u64>().is_err()
                    || !p.bytes().all(|c| c.is_ascii_digit())
            })
        {
            return Err("Version must have three numeric components, for example 0.1.0".into());
        }
        let parts: Vec<_> = self.plugin_id.split('.').collect();
        if self.plugin_id.len() > 255
            || parts.len() < 2
            || parts.iter().any(|p| {
                p.is_empty()
                    || !p
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            })
        {
            return Err("Product identifier must be a reverse-domain ASCII identity".into());
        }
        self.framework_path = self.framework_path.canonicalize()?;
        for path in [
            "crates/ravet/Cargo.toml",
            "crates/ravet-surface/Cargo.toml",
            "examples/gain/src/lib.rs",
            "tools/foundry/Cargo.toml",
        ] {
            if !self.framework_path.join(path).is_file() {
                return Err(format!("RAVET checkout is missing {path}").into());
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Managed {
    project: Project,
    files: BTreeMap<String, String>,
}

pub(crate) fn read_text(path: &Path, maximum: u64) -> Result<String> {
    let mut text = String::new();
    fs::File::open(path)?
        .take(maximum + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > maximum {
        return Err(format!("{} exceeds its size limit", path.display()).into());
    }
    Ok(text)
}

fn outside(root: &Path, framework: &Path) -> Result<()> {
    if root.starts_with(framework) || root.starts_with(framework_root()) {
        return Err("Consumer projects must stay outside the RAVET repository".into());
    }
    Ok(())
}
fn regular(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(format!("{} must be a regular file", path.display()).into());
    }
    Ok(())
}
fn metadata_directory(root: &Path) -> Result<PathBuf> {
    let path = root.join(".foundry");
    if !fs::symlink_metadata(&path)?.is_dir() {
        return Err("Foundry metadata directory must not be a symlink".into());
    }
    Ok(path)
}

/// Read bounded settings from a project directory or its ravet-project.toml.
/// This does not run Cargo or mutate the consumer.
pub fn load(path: &Path) -> Result<Project> {
    let file = if path.is_dir() {
        path.join("ravet-project.toml")
    } else {
        path.to_owned()
    };
    regular(&file)?;
    let mut project: Project = toml::from_str(&read_text(&file, 65_536)?)?;
    project.validate()?;
    outside(
        &file
            .parent()
            .ok_or("Project needs a parent")?
            .canonicalize()?,
        &project.framework_path,
    )?;
    Ok(project)
}

/// Create a new independent workspace with exclusive destination ownership.
/// Parent must exist. Existing destinations and symlink aliases into RAVET reject.
/// Initial DSP/editor source belongs to the consumer and is never regenerated.
pub fn create(destination: &Path, mut project: Project) -> Result<PathBuf> {
    project.validate()?;
    let name = destination
        .file_name()
        .ok_or("Destination needs a directory name")?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let root = parent.canonicalize()?.join(name);
    outside(&root, &project.framework_path)?;
    // Render before mutation; mkdir is exclusive even if another process races us.
    let managed = templates::metadata(&project)?;
    let sources = templates::sources(&project)?;
    fs::create_dir(&root)?;
    let result = (|| -> Result<()> {
        fs::create_dir(root.join(".foundry"))?;
        for (path, contents) in sources.iter().chain(managed.iter()) {
            let file = root.join(path);
            fs::create_dir_all(file.parent().ok_or("Source needs a parent")?)?;
            fs::write(file, contents)?;
        }
        fs::write(
            root.join("ravet-project.toml"),
            toml::to_string_pretty(&project)?,
        )?;
        fs::write(
            root.join(".foundry/managed.json"),
            serde_json::to_vec_pretty(&Managed {
                project,
                files: managed,
            })?,
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&root);
    }
    result?;
    Ok(root)
}

struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Update tracked build metadata after checking every file for consumer edits.
/// Rejects changed package name/kind and edited/symlinked managed files before
/// mutation. Serializes cooperating Foundry processes with an exclusive lock.
/// Each replacement is atomic; the group is not crash-atomic. On I/O failure,
/// best-effort rollback restores originals; inspect a project after disk failure.
pub fn regenerate(root: &Path, mut project: Project) -> Result<()> {
    project.validate()?;
    let root = root.canonicalize()?;
    outside(&root, &project.framework_path)?;
    let metadata = metadata_directory(&root)?;
    let lock_path = metadata.join("operation.lock");
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| format!("Project is busy or its lock cannot be created: {e}"))?;
    let _lock = Lock(lock_path);
    let state = metadata.join("managed.json");
    regular(&state)?;
    let previous: Managed = serde_json::from_str(&read_text(&state, 262_144)?)?;
    if project.name != previous.project.name
        || project.kind != previous.project.kind
        || project.plugin_type != previous.project.plugin_type
    {
        return Err("Package name and template cannot change after creation".into());
    }
    let next = templates::metadata(&project)?;
    // Iterate only trusted engine paths, never paths supplied by metadata.
    for path in next.keys() {
        let file = root.join(path);
        regular(&file)?;
        let mut parent = file.parent().ok_or("Managed file needs a parent")?;
        while parent != root {
            if !fs::symlink_metadata(parent)?.is_dir() {
                return Err("Managed directories must not be symlinks".into());
            }
            parent = parent.parent().ok_or("Managed file escaped project")?;
        }
        if previous.files.get(path) != Some(&read_text(&file, 65_536)?) {
            return Err(format!(
                "Regeneration conflict: {path} was edited; preserve/resolve the edit first"
            )
            .into());
        }
    }
    regular(&root.join("ravet-project.toml"))?;
    let config = toml::to_string_pretty(&project)?;
    if next == previous.files
        && project == previous.project
        && read_text(&root.join("ravet-project.toml"), 65_536)? == config
    {
        return Ok(());
    }
    let mut updates = next.clone();
    updates.insert("ravet-project.toml".into(), config);
    updates.insert(
        ".foundry/managed.json".into(),
        serde_json::to_string_pretty(&Managed {
            project,
            files: next,
        })?,
    );
    let stage = metadata.join("stage");
    fs::create_dir(&stage).map_err(|e| {
        format!(
            "Cannot stage regeneration; inspect .foundry/stage after an interrupted operation: {e}"
        )
    })?;
    let mut originals = Vec::new();
    let result = (|| -> Result<()> {
        for (index, (path, contents)) in updates.iter().enumerate() {
            originals.push((root.join(path), fs::read(root.join(path))?));
            fs::write(stage.join(index.to_string()), contents)?;
        }
        for (index, (file, _)) in originals.iter().enumerate() {
            fs::rename(stage.join(index.to_string()), file)?;
        }
        Ok(())
    })();
    if result.is_err() {
        for (file, original) in originals {
            let _ = fs::write(file, original);
        }
    }
    let _ = fs::remove_dir_all(stage);
    result
}
