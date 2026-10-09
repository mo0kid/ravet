//! Bounded control-file reads and atomic replacement, independent of product paths.
use std::{
    fmt,
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

/// No partial read is returned on error.
#[derive(Debug)]
pub enum FileError {
    Io(io::Error),
    TooLarge,
    InvalidLimit,
}
impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "file operation failed: {e}"),
            Self::TooLarge => f.write_str("file exceeds caller's byte limit"),
            Self::InvalidLimit => f.write_str("file limit cannot be represented"),
        }
    }
}
impl std::error::Error for FileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Io(e) = self {
            Some(e)
        } else {
            None
        }
    }
}
impl From<io::Error> for FileError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
/// Read binary bytes up to a caller-supplied limit. Empty files succeed;
/// missing files fail. Checks the actual stream, including a growing file.
pub fn read(path: impl AsRef<Path>, maximum_bytes: usize) -> Result<Vec<u8>, FileError> {
    read_bounded(File::open(path)?, maximum_bytes)
}
/// Apply the same bound to any reader; reads at most one byte beyond the limit.
pub fn read_bounded(reader: impl Read, maximum_bytes: usize) -> Result<Vec<u8>, FileError> {
    let limit = maximum_bytes
        .checked_add(1)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or(FileError::InvalidLimit)?;
    let mut bytes = Vec::new();
    reader.take(limit).read_to_end(&mut bytes)?;
    if bytes.len() > maximum_bytes {
        Err(FileError::TooLarge)
    } else {
        Ok(bytes)
    }
}
/// Create parent directories, write an exclusive same-directory temporary,
/// flush its content, then atomically replace the destination on supported OSes.
/// A failed replacement preserves the old destination and cleans the temporary.
/// Does not sync the parent directory or promise power-loss durability.
///
/// ```
/// # fn example(path: &std::path::Path) -> Result<(), ravet_runtime::files::FileError> {
/// ravet_runtime::files::atomic_write(path, b"complete state")?;
/// assert_eq!(ravet_runtime::files::read(path, 32)?, b"complete state");
/// # Ok(()) }
/// ```
pub fn atomic_write(path: impl AsRef<Path>, bytes: &[u8]) -> Result<(), FileError> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".ravet-write-")
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|e| FileError::Io(e.error))?;
    Ok(())
}
