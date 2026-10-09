# 0007: Explicit control-thread data services

Status: bounded XML, SHA-256, and legacy integer verification implemented.

## Context and decision

Consumers need state and preset trees and existing signature compatibility while
keeping filesystem, parsing, and cryptography out of audio callbacks. Put these
services in an explicit `ravet-data` crate, separate from the DSP umbrella.
Use pinned quick-xml, RustCrypto sha2, and num-bigint implementations instead of
implementing a general XML parser or arbitrary-precision arithmetic in RAVET.

## Consequences and validation

The XML codec deliberately supports attribute trees with bounded size, depth,
and count and rejects entities that require external input. ASCII names and
Unicode values cover the current consumer records; richer XML remains outside
scope. Legacy unpadded integer verification preserves a wire format and does not
select a new signature protocol. Storage paths, product public keys, activation
requests, and proprietary fixtures stay in private consumers.

Generic tests cover malformed and bounded XML, Unicode/entity roundtrip,
SHA-256, and digest byte interpretation. These services allocate and belong on
control or worker threads. See [control data guide](../control-data.md) for exact
limits and dependency licences.
