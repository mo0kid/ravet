# 0271: Hosted VST3 editor ownership (macOS NSView)

Status: prototype implemented; local macOS arm64 evidence only. This covers
authored SDK view fixtures, a substituted-loader provider fixture, and the actual
RAVET Gain VST3 editor in a hidden window. No DAW, third-party editor or
accessibility claim.

## Context

Scoped VST3 instances own the component and controller but did not own a plug-in
view. The CLAP path already has a guarded editor (decision 0048). VST3 adds a
host frame callback (`IPlugFrame::resizeView`), size constraints, an optional
content-scale interface, and a controller whose retirement must follow the
view's release.

## Decision

1. Add an additive exact-origin `RavetVst3EditorProviderV1` (88 bytes, nine
   MAIN-only entries) paired with the instance ABI-v2 table. It adds statuses
   16..19 (unavailable, unsupported, reentry, empty) after the shared domain.
   Existing tables, layouts and domains are unchanged.
2. Create views only on the platform main thread with
   `createView("editor")`. Only `NSView` is defined and must report
   `kResultTrue`. The host frame is installed before a bounded initial
   `getSize`. Extents are 1..16384 per dimension, the CLAP bound. Any
   creation failure clears the frame and releases the view, and publishes no
   handle.
3. `attach` borrows a caller-owned `NSView` (an unsafe obligation in Rust). A
   returned refusal leaves the view detached. A throw is treated as attached
   and faulted, so teardown still calls `removed`. A returned `removed`
   refusal stays attached and retryable.
4. A host resize requires `canResize == kResultTrue`, then
   `checkSizeConstraint == kResultTrue`. The SDK defines `kResultTrue` as
   `kResultOk`: the rectangle is valid, possibly after adjustment.
   `kResultFalse`, `kNotImplemented` and every other result are returned
   refusals, with no unconstrained fallback. The adjusted rect must be bounded.
   Then `onSize == kResultOk`. The recorded size is the `getSize` re-queried
   after `onSize`, as the SDK sequence permits. A differing or failed re-query
   is counted, and the passed size is kept when the re-query is invalid.
5. `resizeView` from the plug-in:
   - Off-main calls, calls after detach, foreign views and out-of-range rects
     are refused, as are all calls once the owner is faulted.
   - During host `attached` or `setContentScaleFactor` the request is
     acknowledged and honoured later. It is applied once after the call while
     the view is attached, otherwise at the next successful attach. A
     superseded request, a request refused by the plug-in's own `onSize`, and
     a request lost to a fault or teardown are counted
     (`deferred_discarded`), never silently dropped.
   - During every other host call into the view, and during a frame-initiated
     `onSize`, the request is refused as reentry.
   - Idle requests are applied only while attached. `onSize` then runs in the
     same call stack under the module's busy flag.
   Applied sizes go to a one-slot mailbox that the consumer reads. Detach,
   teardown and a successful host resize clear it. Nothing calls into Rust, and
   no view, frame or controller pointer escapes.
6. Each child has at most one editor. Child destruction returns `LIFECYCLE`
   while it lives, so a released view always precedes handler clearing,
   controller terminate, component terminate and module exit. Thread, argument
   and reentry refusals happen before teardown starts and keep the editor.
   Once teardown starts (`removed`, `setFrame(nullptr)`, frame detach,
   release) it always consumes and reports the first foreign failure.
   Plug-in-retained frames refuse later calls and are freed on their final
   release. An unretired owner destroyed off MAIN, or from inside its own
   callback where `retire` is refused, terminates the process deterministically
   rather than leaving a dangling frame.
7. Editor entries hold the module's existing busy flag, but not the live-core
   CONTROL admission. Nested editor or instance CONTROL calls from view
   callbacks are therefore refused, and the raw native editor entries stay
   usable while an AUDIO capability is active.
8. Every report written by a live owner on MAIN carries a STATE flag, with
   ATTACHED stating the true attached state after the call. Refusals are
   included: a second attach and operations after a fault report "attached".
   Rust `Editor::attached()` updates only from such reports.
9. In Rust, `Editor<'i, 'm, 'a>` exclusively borrows its `Instance` and exposes
   only `&Instance`. `Provider` and `Editor` are neither Send nor Sync. Drop
   destroys once. A drop-time destroy that is refused before teardown starts
   is unreachable in practice: the guard is `!Send` and nothing calls into
   Rust. If it happens anyway, the native editor is kept alive and
   `refused_drops()` increments.

## Accepted limitations

- **Rust editor and audio on one instance.** Because the guard holds
  `&mut Instance`, the scoped Rust calls that also need `&mut Instance`
  (`with_audio`, `prepare`, `start`, `stop`, state and parameter refresh)
  require the editor to be closed. A concurrently open editor and running
  AUDIO are demonstrated only through the raw native tables (fixture and
  real-window CTest). Future direction: a shared-borrow editor for Graph-bank
  children (the analogue of CLAP decision 0116). It would rely on the native
  `LIFECYCLE` refusal, and on bank reclamation that closes editors before
  ending leases, rather than on the borrow checker.
- **Leak on forget.** `mem::forget` on the guard ends the borrow without
  destroying the native editor. Dropping the `Instance` then leaks the native
  child, because its drop ignores the `LIFECYCLE` refusal, and dropping the
  `Library` leaks the module (`CHILDREN`). This memory-safe leak is the accepted
  outcome. Out-of-order release is never possible.

## Consequences

A resizable view that does not implement `checkSizeConstraint` cannot be
host-resized. Content scale is optional and normally unsupported on macOS.
Keyboard, focus and wheel forwarding, accessibility and edit-gesture adoption
are not provided: the existing handler still returns `kNotImplemented` for edit
gestures. Graph-bank child editors are separate work. Evidence is in-process and
local to macOS arm64.
