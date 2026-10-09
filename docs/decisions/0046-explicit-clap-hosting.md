# 0046 — explicit CLAP candidate and loaded catalogue ownership

## Decision

Add optional native hosting mechanisms outside portable processing. Discover only
explicit bounded directory candidates without loading them. Require an explicit
unsafe native-code trust boundary before CLAP initialization/metadata inspection,
and expose the valid Clack entry for consumer-owned callback/lifecycle policy.
Copy bounded native metadata and preserve byte strings rather than replacing
identifiers through lossy UTF-8 conversion. Clack retains live instance libraries.

## Consequences and scope

Filesystem candidates are neither validated plugins nor trust assertions. A
loaded catalogue supplies usable owned metadata and entry access; it does not
schedule callbacks, render GUI or sandbox plugins. Generic instance management
continues through Clack's valid ownership API. Safe linked entries enable tests
without dynamic unsafe code. Selected native inspection is a separate command,
not implicit recursive/system scanning or production plugin execution.

## Validation

Discovery fixtures verify bounded/sorted/deduplicated explicit paths, non-loading
invalid file bytes, symlink/nested exclusion and entry/candidate limits. Linked
catalogue metadata survives wrapper drop while a live instance completes three
activation cycles. Native upstream Clack gain at pinned v0.2 commit plus RAVET
gain/sine bundles pass selected catalogue/state/offline audio lifecycles locally.
Only a narrow single-main-bus f32 empty-callback host is exercised; other plugin,
platform, editor, process isolation and deadline support remains separate.
