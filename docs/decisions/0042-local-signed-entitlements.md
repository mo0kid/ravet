# 0042 — Explicit local signed entitlement records

Date: 9 October 2026. Status: implemented within stated limits.

Generic unlocking facilities need authenticated local records without embedding
private product activation policy. Supply a bounded canonical claim payload and
explicit trust-set verifier in the optional control-data crate. Issuers sign a
domain-separated complete envelope outside RAVET; no signing/private-key API is
exported. Product/subject/features and half-open time intervals belong to the
consumer. No network, hardware query, storage, clock or DSP gate is selected.

Use pinned ed25519-dalek 2.2.0 strict verification, rejecting weak/invalid trust
and duplicate key IDs. Authenticate before exposing immutable claims; explicitly
check product/time and recheck validity for cached feature grants. Bound trust,
claims and total bytes. This does not provide remote revocation or anti-tamper
clock policy and makes no consumer activation-service claim. Dependency notices
are separate from first-party GPL licensing.

Synthetic tests cover canonical encoding, validity edges, stale caches, every
truncated prefix and changed byte, wrong product/key, malformed fields, trailing
bytes and invalid/weak trust. Real consumer licence fixtures remain private.
