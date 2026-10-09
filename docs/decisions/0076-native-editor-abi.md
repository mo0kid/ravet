# 0076: Rust-owned native editor ABI

## Context

Native format controllers need a callable boundary that preserves Rust editor
ownership and callback retention, without duplicating product or platform logic.

## Decision

Expose a macOS C ABI over WebView with counted trusted HTML/messages/snapshots,
explicit status codes, native parent attachment and main-thread operation checks.
Foreign context has an optional release callback consumed once on creation
failure or after teardown and any entered callback. Snapshot copying and the
existing retained Rust context allow asynchronous callback self-destruction.
Wrong-thread destroy rejects while retaining ownership for retry.

Consumers provide valid live opaque handles/buffers/parent views and nonthrowing
callbacks; null checks cannot repair forged foreign pointers. Native method
reentrancy/lifecycle remain serialized UI ownership obligations. No window/run
loop, parameter/state/format policy or automatic host integration is synthesized.
Gain-native has a default-disabled linkage feature for native consumers.

## Consequences and validation

A shared public header avoids consumer assumptions about Rust object layout.
Other OS native backends remain separate from this macOS Rust ownership boundary.
No real-time contract applies to allocation, callback formatting or teardown.

Actual C++/Cocoa tests pass three lifecycle cycles, main-thread rejection/retry,
complete snapshot/message delivery and destruction from message/snapshot calls,
with exactly-once context release after callback return. The initial probe
checked a snapshot before page readiness; bounded DOM readiness polling fixes
the fixture. Rust fixtures cover malformed snapshots and failure consumption.
DAW-specific editor mapping remains the next independent integration work.
