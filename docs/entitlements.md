# Explicit local signed entitlements

`ravet_data::entitlement` provides bounded local claim encoding and verification
for consumers that explicitly choose it. No module construction performs network,
file, hardware-identity or clock access. It does not activate a product, gate DSP,
start a trial, select a purchase endpoint or replace a consumer's licence policy.
The consumer supplies its permanent product ID, trusted issuer public keys,
signed bytes and trustworthy current time. Private issuer keys, real product
records, release policy and UI stay in private sibling projects.

## Claims, token and trust boundary

Claims contain an opaque eight-byte product ID, 1..256 UTF-8 subject bytes without
control characters, half-open Unix-second validity and up to 64 unique feature
IDs. Feature IDs use 1..64 ASCII alphanumeric/_/- bytes. Canonical encoding sorts
features; noncanonical order, duplicates, malformed UTF-8, control characters,
invalid validity and trailing data reject. Empty feature lists are valid.
`u64::MAX` is a practical unbounded validity end, not a separate perpetual flag.
Subject semantics and personal-data policy remain consumer-owned.

The payload is `RVETENT1`, product, LE not-before/expiry u64s, LE subject u16
length/bytes, feature u8 count and u8 length/bytes for each feature. Signing bytes
prepend `RVETSIG1` and a LE u32 key ID. Token assembly appends the supplied 64-byte
Ed25519 signature; assembly does not authenticate it. Issuers sign the complete
signing bytes externally. Total token bytes cap at 8,192.

`EntitlementVerifier::new` accepts 1..32 explicit trusted public Ed25519 keys;
invalid/weak keys and duplicate key IDs reject. Tokens cannot provide trusted
keys. Verification authenticates the full signing envelope before exposing
claims, checks the consumer product ID and rejects not-yet-valid/expired times.
Pinned BSD-3-Clause `ed25519-dalek` 2.2.0 performs
[strict verification](https://docs.rs/ed25519-dalek/2.2.0/ed25519_dalek/struct.VerifyingKey.html#method.verify_strict);
RAVET does not implement its own signature algorithm or use batch verification.
Dependency licences/notices remain applicable to consumer distribution.

Only successful verification returns immutable `VerifiedEntitlement`.
`allows(feature, now)` rechecks half-open validity, accepting exactly not-before
and rejecting exactly expiry. Cached records must be checked against current
trusted time. This provides no clock anti-tamper, remote revocation, machine
binding, online account service, key discovery/rotation schedule or stored grant.
Consumers choose how to handle refresh failures and previously verified records.

All APIs allocate and perform cryptography on control/worker threads, outside
audio. No endpoint, identity, credential, default key or actual product fixture
is included. Tests use public synthetic seeds/claims only and cover canonical
encoding, exact validity boundaries/stale-cache denial, changed bytes throughout
the envelope/signature, every truncated prefix, trailing/oversized data, wrong
products/keys, weak/duplicate trust and malformed unsigned payloads. This is
framework verification evidence, not an external activation-service integration.

See [decision 0042](decisions/0042-local-signed-entitlements.md),
[control data](control-data.md) and [runtime services](runtime-services.md).
