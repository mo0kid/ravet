//! Explicit CLAP discovery and optional trusted VST3/AUv3 native ownership.
//! Optional native hosting stays outside the portable umbrella. Discovery never
//! executes candidates; loading is an explicit unsafe native-code trust boundary.
/// Caller-linked macOS AUv3 asynchronous owners and joined main-bus audio.
#[cfg(feature = "au-hosting")]
pub mod au;
/// Caller-linked trusted VST3 catalogue and scoped instance ABIs with main-thread ownership.
#[cfg(feature = "vst3-catalogue")]
pub mod vst3;
use clack_host::prelude::PluginEntry;
mod editor;
pub use editor::{EditorError, PluginEditor};
use std::{
    collections::BTreeSet,
    ffi::{CStr, CString},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum HostError {
    Io(std::io::Error),
    Capacity,
    InvalidMetadata,
    MissingFactory,
    Load(clack_host::entry::PluginEntryError),
}
impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "host error: {self:?}")
    }
}
impl std::error::Error for HostError {}
impl From<std::io::Error> for HostError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
/// Discover .clap files/macOS bundles only in explicitly supplied directories.
/// At most 32 directories, 4096 total directory entries and maximum_candidates
/// 1..1024 are accepted. No recursion, implicit system paths, symlink following,
/// binary loading or persistent cache occurs. Results are canonicalized, unique
/// and path-sorted. Directory I/O/capacity errors reject the whole result. This
/// lists candidates, not verified CLAP libraries. Only macOS accepts directories
/// with a .clap extension; other targets accept regular files.
pub fn discover_clap(
    directories: &[PathBuf],
    maximum_candidates: usize,
) -> Result<Vec<PathBuf>, HostError> {
    if directories.len() > 32 || !(1..=1024).contains(&maximum_candidates) {
        return Err(HostError::Capacity);
    }
    let mut candidates = BTreeSet::new();
    let mut entries = 0;
    let mut visited = BTreeSet::new();
    for directory in directories {
        let metadata = std::fs::symlink_metadata(directory)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(HostError::InvalidMetadata);
        }
        let directory = directory.canonicalize()?;
        if !visited.insert(directory.clone()) {
            continue;
        }
        for entry in std::fs::read_dir(directory)? {
            entries += 1;
            if entries > 4096 {
                return Err(HostError::Capacity);
            }
            let entry = entry?;
            let kind = entry.file_type()?;
            let candidate = kind.is_file() || (cfg!(target_os = "macos") && kind.is_dir());
            if !candidate
                || !entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("clap"))
            {
                continue;
            }
            let path = entry.path().canonicalize()?;
            candidates.insert(path);
            if candidates.len() > maximum_candidates {
                return Err(HostError::Capacity);
            }
        }
    }
    Ok(candidates.into_iter().collect())
}
/// Owned, byte-preserving native descriptor data. Display text need not be UTF-8.
#[derive(Clone, Debug)]
pub struct PluginMetadata {
    pub id: CString,
    pub name: CString,
    pub vendor: Option<CString>,
    pub version: Option<CString>,
    pub features: Vec<CString>,
}
fn owned(text: &CStr, maximum: usize) -> Result<CString, HostError> {
    if text.to_bytes().is_empty() || text.to_bytes().len() > maximum {
        return Err(HostError::InvalidMetadata);
    }
    Ok(text.to_owned())
}
/// A loaded CLAP entry and bounded owned catalogue (1..1024 plugins). Optional
/// vendor/version fields and names permit at most 4096 bytes; IDs/features 256
/// bytes and 128 features per plugin. Missing/duplicate IDs or invalid metadata
/// reject the catalogue. Clack retains native code through every cloned entry
/// and associated instance; dropping this wrapper does not unload a live instance.
/// All operations here allocate/execute plugin code outside audio callbacks.
pub struct ClapLibrary {
    entry: PluginEntry,
    plugins: Vec<PluginMetadata>,
}
impl ClapLibrary {
    /// Inspect an already-valid entry, including safe statically linked entries.
    /// Loading/initializing an entry may already have executed native code.
    pub fn from_entry(entry: PluginEntry) -> Result<Self, HostError> {
        let factory = entry
            .get_plugin_factory()
            .ok_or(HostError::MissingFactory)?;
        let count = factory.plugin_count();
        if !(1..=1024).contains(&count) {
            return Err(HostError::Capacity);
        }
        let mut plugins = Vec::with_capacity(count as usize);
        let mut ids = BTreeSet::new();
        for index in 0..count {
            let descriptor = factory
                .plugin_descriptor(index)
                .ok_or(HostError::InvalidMetadata)?;
            let id = owned(descriptor.id().ok_or(HostError::InvalidMetadata)?, 256)?;
            if !ids.insert(id.clone()) {
                return Err(HostError::InvalidMetadata);
            }
            let mut features = Vec::new();
            for feature in descriptor.features() {
                if features.len() == 128 {
                    return Err(HostError::Capacity);
                }
                features.push(owned(feature, 256)?);
            }
            plugins.push(PluginMetadata {
                id,
                name: owned(descriptor.name().ok_or(HostError::InvalidMetadata)?, 4096)?,
                vendor: descriptor.vendor().map(|v| owned(v, 4096)).transpose()?,
                version: descriptor.version().map(|v| owned(v, 4096)).transpose()?,
                features,
            });
        }
        Ok(Self { entry, plugins })
    }
    /// Load/initialize a caller-selected CLAP binary and inspect its catalogue.
    /// The path need not come from discovery; no trust decision is inferred from
    /// filename, signature, metadata or successful symbol lookup.
    ///
    /// # Safety
    /// The caller must ensure this native library and all its plugins comply with
    /// CLAP and its pointer/lifetime/thread contracts. Plugin code has full process
    /// access; arbitrary/noncompliant code can crash or cause undefined behavior.
    /// This API provides no sandbox, timeout, crash isolation or signing check.
    #[allow(unsafe_code)]
    pub unsafe fn load(path: impl AsRef<Path>) -> Result<Self, HostError> {
        // SAFETY: forwarded caller obligation; Clack owns the loaded lifetime.
        let entry = unsafe { PluginEntry::load(path.as_ref()) }.map_err(HostError::Load)?;
        Self::from_entry(entry)
    }
    pub fn plugins(&self) -> &[PluginMetadata] {
        &self.plugins
    }
    pub fn find(&self, id: &CStr) -> Option<&PluginMetadata> {
        self.plugins
            .iter()
            .find(|plugin| plugin.id.as_c_str() == id)
    }
    /// Access the valid entry for consumer-owned Clack host handlers, instance
    /// audio/state/editor lifecycle and extensions. Native threading/callback
    /// policies remain explicit in those APIs; this catalogue does not drive them.
    pub fn entry(&self) -> &PluginEntry {
        &self.entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_copy_preserves_native_bytes_and_rejects_empty_or_oversized_fields() {
        assert!(owned(c"", 256).is_err());
        assert!(owned(&CString::new(vec![b'x'; 257]).unwrap(), 256).is_err());
        assert_eq!(owned(c"valid", 256).unwrap().as_c_str(), c"valid");
        let native = CStr::from_bytes_with_nul(&[255, 0]).unwrap();
        assert_eq!(owned(native, 256).unwrap().as_bytes(), &[255]);
    }
}

mod requests;
pub use requests::{HostRequests, PendingHostRequests, RequestHost};

mod parameters;
pub use parameters::{HostedParameter, HostedParameterError, HostedParameters};

mod parameter_requests;
pub use parameter_requests::{
    ParameterClear, ParameterHost, ParameterMainRequests, ParameterSharedRequests,
    PendingParameterChanges,
};

mod audio_configuration;
pub use audio_configuration::{
    HostedAudioConfiguration, HostedAudioError, HostedAudioPort, hosted_latency, hosted_tail,
};

mod audio_requests;
pub use audio_requests::{
    AudioConfigurationRequests, PendingAudioChanges, ProcessingHost, ProcessingMainRequests,
};

mod audio_lifecycle;
pub use audio_lifecycle::{
    HostedAudioControl, HostedAudioItem, HostedAudioOfferError, HostedAudioOfferFailure,
    HostedAudioReady, HostedAudioRunning, HostedAudioStartError, HostedAudioSwap,
    hosted_audio_handoff,
};

mod graph_node;
pub use graph_node::{
    HostedGraphNode, HostedGraphNode64, HostedGraphNodeBuildError, HostedGraphNodeBuildFailure,
    HostedGraphNodeBuilder, HostedGraphNodeControl, HostedGraphNodeOptions,
    HostedGraphNodeStatistics, HostedGraphPortMode, prepare_hosted_graph_node,
    prepare_hosted_graph_node64, prepare_hosted_graph_ports_node,
    prepare_hosted_graph_ports_node64, prepare_hosted_parameter_graph_node,
    prepare_hosted_parameter_graph_node64,
};

mod transport;
pub use transport::hosted_transport;

mod graph_parameter_scope;
mod graph_parameters;
pub use graph_parameters::HostedGraphParameterMapping;

mod graph_parameter_output;

mod graph_note_expression;
mod graph_note_input;
mod graph_note_output;
pub use graph_note_input::{
    HostedGraphNoteDialect, HostedGraphNoteError, HostedGraphNoteInput, HostedGraphNoteOutput,
};
