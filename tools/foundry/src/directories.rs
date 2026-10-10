use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A validated request for an existing directory, serviced on the native UI thread.
/// Destination browsing selects its parent; the form appends the package name.
#[derive(Clone, Debug)]
pub struct DirectoryRequest {
    /// One of Foundry's directory input IDs; arbitrary DOM targets are rejected.
    pub field: String,
    /// Current field value used to seed the chooser at its nearest existing directory.
    pub initial: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Browse {
    action: String,
    field: String,
    initial: PathBuf,
}
pub(crate) fn valid_field(field: &str) -> bool {
    matches!(
        field,
        "destination"
            | "framework"
            | "open-root"
            | "sdk-vst3"
            | "sdk-audio_unit"
            | "sdk-aax"
            | "sdk-lv2"
    )
}
impl DirectoryRequest {
    /// Parse a bounded browse message. Other action types return `None`.
    pub fn from_message(text: &str) -> Result<Option<Self>> {
        if text.len() > 16_384 {
            return Err("Foundry action exceeds 16 KiB".into());
        }
        let value: serde_json::Value = serde_json::from_str(text)?;
        if value.get("action").and_then(serde_json::Value::as_str) != Some("browse") {
            return Ok(None);
        }
        let request: Browse = serde_json::from_value(value)?;
        if request.action != "browse" || !valid_field(&request.field) {
            return Err("Unknown directory field".into());
        }
        if request.initial.as_os_str().len() > 4096 {
            return Err("Directory path exceeds 4096 bytes".into());
        }
        Ok(Some(Self {
            field: request.field,
            initial: request.initial,
        }))
    }
    /// Short native dialog title for the requested field.
    pub fn title(&self) -> &'static str {
        match self.field.as_str() {
            "destination" => "Choose project parent directory",
            "framework" => "Choose RAVET checkout",
            "open-root" => "Choose project directory",
            "sdk-vst3" => "Choose VST3 SDK",
            "sdk-audio_unit" => "Choose AudioUnitSDK",
            "sdk-aax" => "Choose AAX SDK",
            "sdk-lv2" => "Choose LV2 SDK",
            _ => "Choose directory",
        }
    }
    /// Canonical initial location, including parents of a new destination or file.
    pub fn initial_directory(&self) -> Option<PathBuf> {
        self.initial
            .ancestors()
            .find(|p| p.is_dir())
            .and_then(|p| p.canonicalize().ok())
    }
}
/// A transient, sequenced directory choice. It edits the form, not saved settings.
#[derive(Clone, Debug, Serialize)]
pub struct DirectorySelection {
    /// Target directory input ID.
    pub field: String,
    /// Canonical existing directory selected by the user.
    pub path: PathBuf,
    /// Increases for each accepted choice, including repeated choices of one path.
    pub sequence: u64,
}
