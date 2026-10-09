# 0073: Borrowed CLAP parameter registration

## Context

Product-specific metadata assembly duplicates portable schemas and can silently
truncate names or misrepresent discrete physical ranges at the format boundary.

## Decision

Provide borrowed preparation-time registration of a validated schema and ordered
CLAP presentation/process options. Reject incompatible IDs/strings/steps/bypass
before exposure. Keep exact physical values and complete UTF-8 metadata. Unit
integer discrete ranges are the initial supported wire subset; other discrete
schemes require explicit mappings rather than misleading stepped declarations.

Products consume count/info in their extension implementation. Automatable flags
come from the schema, while module/bypass/requires-process options are explicit.
No cookie, modulation, storage, formatting, flush or host scheduling is invented.
The gain reference now uses the registration and marks both output-affecting
parameters as requiring process while active.

## Consequences and validation

The metadata component is generic without pretending to generate a full plug-in.
Consumer state/event/lifecycle ownership remains necessary. Names/modules reject
instead of truncating, so existing oversized consumer schemas need correction.

Tests cover complete metadata and flag policy, exact logarithmic plain values,
integer steps, all string/ID/capacity/bypass/representation failures, zero fields
for empty registration and 10,000 heap-free repeated reads. Linked actual gain
host-extension checks verify the consumer uses this registration.
