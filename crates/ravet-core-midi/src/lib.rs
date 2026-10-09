//! Explicit macOS CoreMIDI UMP endpoints, outside the portable umbrella.
//! macOS exposes protocol-selected inputs, exact-ID outputs and owned virtual
//! destinations and sources with bounded packet handoff. Other targets expose no backend.
//! Connect/enumerate/send/close belong to control/worker threads. Timestamp units
//! are CoreMIDI host-clock ticks (zero output means now), never microseconds or
//! audio frames. Protocol conversion performed by CoreMIDI remains host policy.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
