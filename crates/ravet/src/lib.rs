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
//! DSP. Format adapters, file/runtime services and native platform integration
//! live outside this portable umbrella; their validation limits are independent.

pub use ravet_foundation as foundation;
pub use ravet_signal as signal;
pub use ravet_surface as surface;
