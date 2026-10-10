# 0155: Explicit native audio-owner complete state

Status: implemented; Rust and linked macOS arm64 C/C++ validation passed.

## Context

Native control-only byte decoding and stopped-owner state access cannot run during
processing. Some consumers already own complete controller publication/scheduling
protocols and need explicit prepared-owner recall and observation without adopting
a second mailbox. Live coherent publication remains an independent opt-in.

## Decision

Add a separate `NativeAudioStateEntryV1`, preserving all existing tables. Pair it
with the exact `NativeCachedParameters<P, S, N>` cache and `NativeProcessor<P, S>`
owner. Control acquisition validates immutable metadata; audio callbacks reuse the
cache and explicit `NativeAudioParameterProcessor<N>` hooks. The hook trait is now
available without 64-bit atomics; the live mailbox entry retains its atomic gate.

Apply accepts exactly the declared count of physical f32 ID/value fields in any
order. Cached binary lookup rejects unknown/duplicate IDs, then validates complete
canonical finite ranges before one consumer restore. Consumer rejection must be
atomic. Snapshot obtains complete base state, revalidates identities/ranges and
reorders into cached declared order before any foreign write. Outputs preserve
failed fields and successful tails; at most 1024 fields/capacity. Snapshot lookup
uses bounded linear provider-state ID lookup (at most N squared), not heap sorting.

Both calls exclusively borrow the prepared audio owner, sharing processor panic
poison and fresh stopped preparation recovery. Hooks must avoid heap actions,
blocking and I/O. Codecs/schema/role providers are absent from audio calls. Foreign
ranges, exact type/precision pairing, immutable cache/library lifetime and stopped
retirement remain caller obligations. Normal framework paths use bounded stack
storage, no allocation/deallocation, locks or I/O. No audio deadline follows.

## Limits and consequences

No automatic publication, revisions, coalescing, scheduling, UI/host notification
or retry is added. The consumer calls restore only for newly selected state;
repeated recall may restart smoothing. Combining direct recall with the live
mailbox is explicit consumer ordering policy: direct calls do not change its
accepted revision, so consumers must publish a new generation or reprepare when
changing that protocol. Values cover global base state, excluding transient
modulation and voices. Consumer panic may mutate internal state before poison;
there is no rollback. All SDK mapping/factories remain separate.

Rust tests and actual-library C/C++ checks are recorded in work status after gates;
o DAW, device, additional target or production signing claim follows.

Equivalent exported/paired immutable tables may have distinct addresses after
Rust constant promotion. Table address is not a type or owner identity; native
contracts verify callback/precision profiles, with exact originating facade/cache/
library pairing still a caller obligation.
