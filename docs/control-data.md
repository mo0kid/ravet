# Control-thread data services

`ravet-data` provides bounded attribute XML, SHA-256, and verification of an
existing unpadded RSA integer signature format. It is an explicit dependency for
control and worker threads; the umbrella, foundation, and signal crates do not
bring it into a processor's dependency graph. Parsing, encoding, and verification
may allocate and must stay outside audio callbacks.

`xml::Element` stores an element name, sorted attributes, and children. Parsing
and encoding accept up to 65,536 UTF-8 bytes, depth 16, and 1,024 elements.
Element and attribute names use ASCII XML identifiers; Unicode attribute values
and standard escaped/numeric entities roundtrip. Namespaces are not resolved.
Text content, CDATA, processing instructions, DOCTYPE declarations, and external
entities are rejected. This is a small state/preset tree codec, not a general XML
document model. Encoding also enforces the limits and rejects invalid names.

`crypto::sha256` returns a 32-byte digest. `LegacyIntegerPublicKey` reads a
hexadecimal `exponent,modulus` key, bounded to 4,096 bits, and verifies a
hexadecimal signature against the SHA-256 digest interpreted as a little-endian
integer. It rejects malformed keys and signatures outside the modulus. This
codec exists for compatibility with existing records; it implements neither
PKCS#1 padding nor PSS and does not define a new licensing protocol.

Pinned runtime dependencies are quick-xml 0.38.3 (MIT), sha2 0.10.9
(MIT or Apache-2.0), and num-bigint 0.4.6 (MIT or Apache-2.0), plus their locked
transitive dependencies. Product keys, machine identifiers, records, storage
paths, activation endpoints, and migration policy belong to consumers.

Run `cargo test -p ravet-data --locked`. Tests cover XML escaping and Unicode,
malformed records, resource bounds, a SHA-256 vector, and integer byte ordering.
Private consumer fixtures are separate from public framework tests.


The separate [local signed-entitlement API](entitlements.md) offers canonical
consumer claims, explicit public-key trust and strict Ed25519 verification.
It does not change the legacy compatibility verifier's wire behavior or choose
a product activation/purchase policy.
