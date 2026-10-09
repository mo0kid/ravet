//! Explicit local signed-entitlement verification on control threads. No network,
//! hardware identity, product gating, clock, private keys or persistence chosen.
use ed25519_dalek::{Signature, VerifyingKey};
use std::collections::BTreeSet;
const MAXIMUM_TOKEN_BYTES: usize = 8192;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntitlementError {
    InvalidRecord,
    InvalidTrust,
    UnknownKey,
    InvalidSignature,
    WrongProduct,
    NotYetValid,
    Expired,
}
impl std::fmt::Display for EntitlementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "entitlement error: {self:?}")
    }
}
impl std::error::Error for EntitlementError {}
/// Unsigned consumer-owned claims. Product is an opaque permanent eight-byte ID;
/// subject is 1..256 UTF-8 bytes without control characters. Feature IDs are
/// 1..64 ASCII alphanumeric/_/- bytes, unique, at most 64. Validity is a half-open
/// Unix-seconds interval; u64::MAX is a practical unbounded end, not a special flag.
pub struct EntitlementClaims<'a> {
    pub product: [u8; 8],
    pub subject: &'a str,
    pub not_before: u64,
    pub expires: u64,
    pub features: &'a [&'a str],
}
fn feature_valid(feature: &str) -> bool {
    !feature.is_empty()
        && feature.len() <= 64
        && feature
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
/// Canonical bounded payload encoding, sorting unique features lexicographically.
/// This is unsigned; consumers sign the complete `signing_message` externally.
/// No signing/private-key operation is exported by RAVET.
pub fn encode_claims(claims: &EntitlementClaims<'_>) -> Result<Vec<u8>, EntitlementError> {
    if claims.subject.is_empty()
        || claims.subject.len() > 256
        || claims.subject.chars().any(char::is_control)
        || claims.not_before >= claims.expires
        || claims.features.len() > 64
    {
        return Err(EntitlementError::InvalidRecord);
    }
    let mut features = BTreeSet::new();
    for &feature in claims.features {
        if !feature_valid(feature) || !features.insert(feature) {
            return Err(EntitlementError::InvalidRecord);
        }
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RVETENT1");
    bytes.extend_from_slice(&claims.product);
    bytes.extend_from_slice(&claims.not_before.to_le_bytes());
    bytes.extend_from_slice(&claims.expires.to_le_bytes());
    bytes.extend_from_slice(&(claims.subject.len() as u16).to_le_bytes());
    bytes.extend_from_slice(claims.subject.as_bytes());
    bytes.push(features.len() as u8);
    for feature in features {
        bytes.push(feature.len() as u8);
        bytes.extend_from_slice(feature.as_bytes());
    }
    Ok(bytes)
}
/// Domain-separated signing bytes: RVETSIG1, LE key ID, canonical payload.
/// Payload is parsed/validated before constructing the bounded message.
pub fn signing_message(key_id: u32, payload: &[u8]) -> Result<Vec<u8>, EntitlementError> {
    if payload.len() > MAXIMUM_TOKEN_BYTES - 76 {
        return Err(EntitlementError::InvalidRecord);
    }
    decode_claims(payload)?;
    let mut message = Vec::with_capacity(12 + payload.len());
    message.extend_from_slice(b"RVETSIG1");
    message.extend_from_slice(&key_id.to_le_bytes());
    message.extend_from_slice(payload);
    Ok(message)
}
/// Append a supplied 64-byte signature to validated signing bytes. This assembles
/// a token but does not claim the signature is valid; use the verifier before use.
pub fn assemble_token(
    key_id: u32,
    payload: &[u8],
    signature: &[u8; 64],
) -> Result<Vec<u8>, EntitlementError> {
    let mut bytes = signing_message(key_id, payload)?;
    bytes.extend_from_slice(signature);
    Ok(bytes)
}
/// Authenticated immutable claims, returned only after strict signature, product
/// and validity verification. `allows` rechecks time to prevent stale-cache grants.
/// Caller supplies trustworthy current time; this is not anti-tamper clock logic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedEntitlement {
    product: [u8; 8],
    subject: String,
    not_before: u64,
    expires: u64,
    features: Vec<String>,
}
impl VerifiedEntitlement {
    pub fn product(&self) -> [u8; 8] {
        self.product
    }
    pub fn subject(&self) -> &str {
        &self.subject
    }
    pub fn valid_at(&self, unix_seconds: u64) -> bool {
        self.not_before <= unix_seconds && unix_seconds < self.expires
    }
    pub fn allows(&self, feature: &str, unix_seconds: u64) -> bool {
        self.valid_at(unix_seconds)
            && self
                .features
                .binary_search_by(|f| f.as_str().cmp(feature))
                .is_ok()
    }
}
/// Explicit 1..32-key trust set with consumer-selected product ID and public
/// Ed25519 keys. Duplicate IDs, invalid/weak keys reject. No keys are obtained
/// from a token or remote source. `verify_strict` is used for every signature;
/// batch/legacy permissive verification is not used. New/drop/verify allocate
/// outside audio. No device, endpoint, activation/trial policy or stored grant.
pub struct EntitlementVerifier {
    product: [u8; 8],
    keys: Vec<(u32, VerifyingKey)>,
}
impl EntitlementVerifier {
    pub fn new(
        product: [u8; 8],
        trusted_keys: &[(u32, [u8; 32])],
    ) -> Result<Self, EntitlementError> {
        if trusted_keys.is_empty() || trusted_keys.len() > 32 {
            return Err(EntitlementError::InvalidTrust);
        }
        let mut keys = Vec::with_capacity(trusted_keys.len());
        for &(id, bytes) in trusted_keys {
            if keys.iter().any(|(existing, _)| *existing == id) {
                return Err(EntitlementError::InvalidTrust);
            }
            let key =
                VerifyingKey::from_bytes(&bytes).map_err(|_| EntitlementError::InvalidTrust)?;
            if key.is_weak() {
                return Err(EntitlementError::InvalidTrust);
            }
            keys.push((id, key));
        }
        Ok(Self { product, keys })
    }
    /// Authenticate the entire envelope/payload before exposing claims; reject
    /// unknown keys, malformed/trailing data, foreign product and invalid time.
    /// Half-open validity accepts exactly not_before and rejects exactly expires.
    pub fn verify(
        &self,
        token: &[u8],
        unix_seconds: u64,
    ) -> Result<VerifiedEntitlement, EntitlementError> {
        if token.len() < 76 || token.len() > MAXIMUM_TOKEN_BYTES || &token[..8] != b"RVETSIG1" {
            return Err(EntitlementError::InvalidRecord);
        }
        let id = u32::from_le_bytes(token[8..12].try_into().expect("validated prefix"));
        let key = &self
            .keys
            .iter()
            .find(|(key_id, _)| *key_id == id)
            .ok_or(EntitlementError::UnknownKey)?
            .1;
        let split = token.len() - 64;
        let signature = Signature::from_slice(&token[split..])
            .map_err(|_| EntitlementError::InvalidSignature)?;
        key.verify_strict(&token[..split], &signature)
            .map_err(|_| EntitlementError::InvalidSignature)?;
        let claims = decode_claims(&token[12..split])?;
        if claims.product != self.product {
            return Err(EntitlementError::WrongProduct);
        }
        if unix_seconds < claims.not_before {
            return Err(EntitlementError::NotYetValid);
        }
        if unix_seconds >= claims.expires {
            return Err(EntitlementError::Expired);
        }
        Ok(claims)
    }
}
fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Result<&'a [u8], EntitlementError> {
    if bytes.len() < count {
        return Err(EntitlementError::InvalidRecord);
    }
    let (head, tail) = bytes.split_at(count);
    *bytes = tail;
    Ok(head)
}
fn decode_claims(mut bytes: &[u8]) -> Result<VerifiedEntitlement, EntitlementError> {
    if bytes.len() > MAXIMUM_TOKEN_BYTES - 76 || take(&mut bytes, 8)? != b"RVETENT1" {
        return Err(EntitlementError::InvalidRecord);
    }
    let product = take(&mut bytes, 8)?.try_into().expect("fixed product");
    let not_before = u64::from_le_bytes(take(&mut bytes, 8)?.try_into().expect("fixed time"));
    let expires = u64::from_le_bytes(take(&mut bytes, 8)?.try_into().expect("fixed time"));
    let length = usize::from(u16::from_le_bytes(
        take(&mut bytes, 2)?.try_into().expect("fixed size"),
    ));
    if !(1..=256).contains(&length) || not_before >= expires {
        return Err(EntitlementError::InvalidRecord);
    }
    let subject = std::str::from_utf8(take(&mut bytes, length)?)
        .map_err(|_| EntitlementError::InvalidRecord)?;
    if subject.chars().any(char::is_control) {
        return Err(EntitlementError::InvalidRecord);
    }
    let count = usize::from(take(&mut bytes, 1)?[0]);
    if count > 64 {
        return Err(EntitlementError::InvalidRecord);
    }
    let mut features: Vec<String> = Vec::with_capacity(count);
    for _ in 0..count {
        let length = usize::from(take(&mut bytes, 1)?[0]);
        let feature = std::str::from_utf8(take(&mut bytes, length)?)
            .map_err(|_| EntitlementError::InvalidRecord)?;
        if !feature_valid(feature)
            || features
                .last()
                .is_some_and(|previous| previous.as_str() >= feature)
        {
            return Err(EntitlementError::InvalidRecord);
        }
        features.push(feature.into());
    }
    if !bytes.is_empty() {
        return Err(EntitlementError::InvalidRecord);
    }
    Ok(VerifiedEntitlement {
        product,
        subject: subject.into(),
        not_before,
        expires,
        features,
    })
}
