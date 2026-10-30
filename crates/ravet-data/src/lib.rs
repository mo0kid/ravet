//! Control-thread data services, independent of DSP and host backends.
//!
//! Parsing, hashing, and legacy signature verification allocate. Never invoke
//! these services from audio callbacks. [`xml`] rejects external entities and
//! bounds document size, depth, and element count. Cryptographic compatibility
//! is for existing data formats; it does not select a new licensing scheme.
//!
//! [`xml`] is always available. Enable the `crypto` feature for SHA-256 and the
//! legacy integer verifier, and `entitlement` for signed local entitlements.

#[cfg(feature = "crypto")]
pub mod crypto;
#[cfg(feature = "entitlement")]
pub mod entitlement;
pub mod xml;
