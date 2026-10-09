//! Hashing and integer-signature compatibility for existing on-disk formats.

use num_bigint::BigUint;
use sha2::{Digest, Sha256};

/// Compute a SHA-256 digest using RustCrypto. This may allocate in dependencies;
/// it is a control-thread service rather than a DSP primitive.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Legacy RSA integer verification, including little-endian SHA-256 digest
/// interpretation. This implements an existing unpadded wire format, not
/// PKCS#1 or PSS. Construct and verify only on control/worker threads.
pub struct LegacyIntegerPublicKey {
    exponent: BigUint,
    modulus: BigUint,
}

impl LegacyIntegerPublicKey {
    /// Parse hexadecimal `exponent,modulus`, rejecting empty, zero, and oversized
    /// keys. The supported modulus width is at most 4096 bits.
    pub fn parse(text: &str) -> Option<Self> {
        let (exponent, modulus) = text.split_once(',')?;
        if exponent.is_empty()
            || modulus.is_empty()
            || exponent.len() > 1024
            || modulus.len() > 1024
            || !exponent.bytes().all(|b| b.is_ascii_hexdigit())
            || !modulus.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return None;
        }
        let exponent = BigUint::parse_bytes(exponent.as_bytes(), 16)?;
        let modulus = BigUint::parse_bytes(modulus.as_bytes(), 16)?;
        if exponent == BigUint::from(0_u8) || modulus <= BigUint::from(1_u8) {
            return None;
        }
        Some(Self { exponent, modulus })
    }

    pub fn verify_sha256_le(&self, payload: &[u8], signature_hex: &str) -> bool {
        if signature_hex.is_empty()
            || signature_hex.len() > 1024
            || !signature_hex.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return false;
        }
        let Some(signature) = BigUint::parse_bytes(signature_hex.as_bytes(), 16) else {
            return false;
        };
        if signature >= self.modulus {
            return false;
        }
        let expected = BigUint::from_bytes_le(&sha256(payload));
        signature.modpow(&self.exponent, &self.modulus) == expected
    }
}
