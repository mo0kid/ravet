//! Explicit preset directories for control/worker threads, outside audio callbacks.
//! No default path, filesystem watch, host registration, worker or network activity.
use ravet_runtime::files::{self, FileError};
use ravet_state::{Limits, Node, NodeId, StateError, Store};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Maximum directory entries inspected, including unrelated files/directories.
pub const MAXIMUM_DIRECTORY_ENTRIES: usize = 4096;
/// File suffix for versioned observable-tree records; product tags are validated on load.
pub const PRESET_EXTENSION: &str = "ravetpreset";

#[derive(Debug)]
pub enum PresetError {
    InvalidName,
    Capacity,
    Io(io::Error),
    File(FileError),
    State(StateError),
}
impl From<io::Error> for PresetError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<FileError> for PresetError {
    fn from(e: FileError) -> Self {
        Self::File(e)
    }
}
impl From<StateError> for PresetError {
    fn from(e: StateError) -> Self {
        Self::State(e)
    }
}
impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "preset error: {self:?}")
    }
}
impl std::error::Error for PresetError {}

/// Bounded metadata snapshot; discovery does not decode or endorse file contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetEntry {
    pub name: String,
    pub encoded_bytes: u64,
}

/// Explicit existing directory, product identity and decoding limits.
/// Uses runtime bounded reads/atomic replacement; file operations block and allocate.
/// Directory ownership and concurrent external changes remain caller concerns.
pub struct PresetLibrary {
    directory: PathBuf,
    product: [u8; 8],
    limits: Limits,
}
impl PresetLibrary {
    /// Canonicalize an existing directory and validate state limits. No directory
    /// is created and no presets are loaded implicitly.
    pub fn open(
        directory: impl AsRef<Path>,
        product: [u8; 8],
        limits: Limits,
    ) -> Result<Self, PresetError> {
        Store::new(Node::new(NodeId(0), "x"), limits)?;
        let directory = directory.as_ref().canonicalize()?;
        if !directory.is_dir() {
            return Err(io::Error::other("preset path is not a directory").into());
        }
        Ok(Self {
            directory,
            product,
            limits,
        })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    /// Inspect at most 4,096 entries and return sorted valid-name regular files
    /// with the exact suffix and bounded length. Symlinks/directories are skipped.
    /// Oversized matching files or too many entries reject the whole snapshot.
    /// Corrupt or foreign-product records are detected only when selected.
    pub fn discover(&self) -> Result<Vec<PresetEntry>, PresetError> {
        let mut result = Vec::new();
        for (index, entry) in fs::read_dir(&self.directory)?.enumerate() {
            if index >= MAXIMUM_DIRECTORY_ENTRIES {
                return Err(PresetError::Capacity);
            }
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some(PRESET_EXTENSION) {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if validate_name(name).is_err() {
                continue;
            }
            let length = entry.metadata()?.len();
            if length > self.limits.bytes as u64 {
                return Err(PresetError::Capacity);
            }
            result.push(PresetEntry {
                name: name.to_owned(),
                encoded_bytes: length,
            });
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }
    fn path(&self, name: &str) -> Result<PathBuf, PresetError> {
        validate_name(name)?;
        Ok(self.directory.join(format!("{name}.{PRESET_EXTENSION}")))
    }
    /// Encode/validate the complete record against library limits, then atomically
    /// replace this named file using runtime ownership/finalization semantics.
    /// Existing records with the same name are replaced; no history/delete policy.
    pub fn save(&self, name: &str, state: &Store) -> Result<(), PresetError> {
        let path = self.path(name)?;
        let bytes = state.encode(self.product);
        Store::decode(&bytes, self.product, self.limits)?;
        files::atomic_write(path, &bytes)?;
        Ok(())
    }
    /// Read/validate an explicit name before touching the destination store.
    /// Restore is one undoable transaction and also obeys destination limits.
    /// Failures preserve destination state, revision and undo/redo history.
    pub fn load_into(&self, name: &str, state: &mut Store) -> Result<bool, PresetError> {
        let bytes = files::read(self.path(name)?, self.limits.bytes)?;
        Store::decode(&bytes, self.product, self.limits)?;
        Ok(state.restore(&bytes, self.product)?)
    }
}

/// Portable file stems: 1..128 ASCII letters/digits/spaces/_/-, without leading
/// or trailing spaces or Windows reserved device names. Invalid names reject;
/// no path traversal, automatic sanitization, Unicode titles or rename policy.
pub fn validate_name(name: &str) -> Result<(), PresetError> {
    if name.is_empty()
        || name.len() > 128
        || name.trim() != name
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'_' | b'-'))
    {
        return Err(PresetError::InvalidName);
    }
    let upper = name.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
    {
        return Err(PresetError::InvalidName);
    }
    Ok(())
}
