# ADR 0225: Monotonic owned standalone close admission

- Status: accepted
- Package: W212

## Context

Native visibility conflates deliberately hidden and closed standalone windows.
Applications need explicit exact-owner status and close requests while a borrowed
editor continues to prevent premature parent destruction.

## Decision

Expose copied macOS-only `StandaloneWindowState` and a read-only `state` getter.
Add `request_close` through native `performClose:` with veto support. Busy
resize/policy/close operations reject; already-closed is a successful no-op only
outside an in-progress operation.

A framework-owned NSWindow subclass marks terminal/closing ivars before native
superclass close and notifications. This avoids relying on observer ordering to
retire input, keeps notification-phase flags distinct from completed closure and
avoids any raw framework-host callback capture or global subscriber. Synthetic
notifications do not set the authoritative terminal flag.

Retired window effects reject, while state/last constraints remain readable.
Borrowed editor resources survive closure until explicitly dropped before the
window. Owner destruction uses direct close without veto. Already-admitted
JavaScript/foreign native effects cannot be rolled back; deferred snapshot file
output checks the strongly retained native window's terminal state.

## Evidence and limits

Five exact owned hidden windows pass 15 scoped native groups, including two real
guarded close notifications, synthetic notification counterexamples, veto,
retired operation/effect inspection and two destructor-specific notification
counts, plus unavailable-content embedding without panic or child/context leak.
Contexts drop once and children detach before parent teardown. Scoped
fixture observers retain only their exact native windows/borrowed counters and
are removed before context retirement.

This is own native action/notification and resource evidence, not human window
interaction or global foreground/minimization behavior. The API is macOS only
and introduces no arbitrary delegate/callback, process-quit, implicit show,
global focus, device, DAW, AX/screen-reader or deadline guarantee.
