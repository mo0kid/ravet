# 0062: Deferred bounded host parameter requests

## Context

Hosted metadata needs a host parameter extension that can receive rescan, clear
and flush requests without invoking plugin code from the requesting callback.
Reference invalidation cannot safely guess missing IDs or erase unrelated data.

## Decision

Provide opt-in parameter-aware host handlers with separate prepared mailboxes.
Thread-safe flush publication is coalesced atomically alongside core requests.
Main-thread rescan flags coalesce; at most 64 distinct clear IDs merge flags in
first-observed order. Main-thread shared ownership is deliberately not Send/Sync.
Consumers drain outside callbacks and own lifecycle, reference invalidation and
metadata refresh. No callback executes plugin code or allocates after preparation.

## Consequences and validation

Distinct-ID overflow retains the prefix and reports a fault, requiring consumer
reference reconciliation or instance rejection. It never silently widens the
clear set. Rescan ALL requires deactivation; process/flush remain serialized by
consumer ownership. This is extension handoff, not an automation scheduler.

Actual linked plugin initialization/main-thread calls exercise the advertised
extension. Acceptance covers exact IDs/flags, overflow, concurrent flush/core
preservation and prepared heap workloads. Offline fixtures do not establish
native event-loop integration or arbitrary third-party compatibility.
