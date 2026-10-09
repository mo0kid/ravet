//! Explicit environment-derived user locations; no directory creation.
use std::path::PathBuf;
/// User home from HOME (Unix/macOS) or USERPROFILE (Windows).
/// Missing/empty variables return None; no current-directory fallback.
pub fn home() -> Option<PathBuf> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(key)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}
/// Control-data root: macOS ~/Library/Application Support, Windows APPDATA,
/// Linux absolute XDG_CONFIG_HOME or ~/.config. Caller appends product paths.
pub fn application_data() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return home().map(|p| p.join("Library/Application Support"));
    }
    if cfg!(windows) {
        return std::env::var_os("APPDATA")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|p| p.join(".config")))
}
