//! RAVET: Rust Audio Visual Engineering Toolkit.
//!
//! A modular framework for audio plug-ins and standalone applications.
//! The portable processor and buffer contracts are implemented; APIs remain
//! provisional. The `ravet-gain` workspace example demonstrates offline gain
//! processing with sample-timed automation and smoothing.
//!
//! The separate `ravet-gain-clap` product exports this processor as a CLAP
//! prototype with complete gain/bypass session state and a macOS embedded editor.
//! Other gain format references and explicit device adapters use the same Rust
//! DSP. Each component's validation limits are its own.
//!
//! # Depending on RAVET
//!
//! Consumers depend on this crate alone, by release tag, and enable components
//! through Cargo features:
//!
//! ```toml
//! ravet = { git = "https://github.com/mo0kid/ravet", tag = "v0.1.0", features = ["clap", "webview"] }
//! ```
//!
//! [`foundation`], [`signal`] and [`surface`] are always available. Every other
//! component crate is behind a feature named after the crate without its
//! `ravet-` prefix, and is re-exported as a module of the same name with
//! hyphens replaced by underscores (`midi-device` → `ravet::midi_device`).
//! Component features such as `vst3-hosted-audio` or `flac-encode` pass through
//! under their original names and enable their component. `ravet-embedded` is
//! not included, because this crate requires `std`; `no_std` consumers depend
//! on `ravet-foundation` and `ravet-embedded` from the same tag.

pub use ravet_foundation as foundation;
pub use ravet_signal as signal;
pub use ravet_surface as surface;

#[cfg(feature = "audio-file")]
pub use ravet_audio_file as audio_file;
#[cfg(feature = "clap")]
pub use ravet_clap as clap;
#[cfg(feature = "core-midi")]
pub use ravet_core_midi as core_midi;
#[cfg(feature = "data")]
pub use ravet_data as data;
#[cfg(feature = "device")]
pub use ravet_device as device;
#[cfg(feature = "graph")]
pub use ravet_graph as graph;
#[cfg(feature = "host")]
pub use ravet_host as host;
#[cfg(feature = "instrument")]
pub use ravet_instrument as instrument;
#[cfg(feature = "media")]
pub use ravet_media as media;
#[cfg(feature = "metal")]
pub use ravet_metal as metal;
#[cfg(feature = "midi-device")]
pub use ravet_midi_device as midi_device;
#[cfg(feature = "native")]
pub use ravet_native as native;
#[cfg(feature = "native-dsp")]
pub use ravet_native_dsp as native_dsp;
#[cfg(feature = "osc")]
pub use ravet_osc as osc;
#[cfg(feature = "plugin")]
pub use ravet_plugin as plugin;
#[cfg(feature = "preset")]
pub use ravet_preset as preset;
#[cfg(feature = "runtime")]
pub use ravet_runtime as runtime;
#[cfg(feature = "runtime-ffi")]
pub use ravet_runtime_ffi as runtime_ffi;
#[cfg(feature = "state")]
pub use ravet_state as state;
#[cfg(feature = "webview")]
pub use ravet_webview as webview;
