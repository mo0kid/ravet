//! Allocating, blocking services explicitly kept outside audio callbacks.
//!
//! Files, HTTPS, owned background work and worker timers contain no product
//! paths, endpoints, licensing policy or automatic network activity. Native
//! consumers use the separate `ravet-runtime-ffi` boundary. This crate forbids
//! unsafe code and is not a dependency of the portable `ravet` umbrella.
pub mod analytics;
pub mod clock;
pub mod files;
pub mod http;
pub mod paths;
pub mod tasks;
