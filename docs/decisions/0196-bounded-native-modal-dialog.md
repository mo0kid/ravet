# ADR0196: Bounded native modal dialog ownership

Status: Accepted

## Context

Reusable local dialogs need actual browser modality and predictable completion
without replacing native focus routing. Native close events are queued: reopening
or replacing a wrapper before old completion can accidentally retire its successor.
Disposal must cancel consumer callbacks while preserving this event boundary.

## Decision

Wrap connected native `<dialog>` with a fixed focus descendant, explicit bounded
label and scalar 0..128-code-unit completion values. Require runtime native API and
modal-selector support. Delegate modal containment and previous-focus restoration
to the browser. Own exact ARIA label/modal attributes and original returnValue;
restore on disposal. Consumers own contents/actions/presentation and host transport.

Use original-document WeakMap modal admission and per-node WeakMap live/drain
ownership. Hold normal live admission through queued native close and guarded
notification. Native close and method=dialog forms are supported with validated
final returnValue; invalid values suppress notification with a diagnostic. Explicit
cancel routes once through native close. Completion/drain accepts actual trusted
native close events, preventing synthetic early release.

Disposal restores attributes/cancels callbacks but retains one node tombstone until
actual native completion. Preserve admission through synchronous close/focus and
any executing callback, then release document admission once native open/modal state
is false. Different-node modals remain usable if retired-node event scheduling is
abandoned. Old drains only release their own identity. Same-node reuse stays blocked
until actual completion; no guessed timeout. Deferred opening disposal also keeps
ownership until native show outcome is known. Consumers keep node/document alive
for reuse or abandon the retired node when destroying its owner.

Observe fixed lifecycle and require explicit refresh for stylesheet/media changes.
External native reopening and open-attribute mutation are unsupported. Actual engines
can retain modality after open removal; recover through guarded native re-show/close
without assigning open, with no guarantee of previous-focus bookkeeping after that
violation. There is no custom modal trap, native OS dialog, window-blur dismissal,
nested stack, content/action discovery or dependency/manifests change.

## Validation and limits

Actual Chromium/hidden WebKit fixtures cover native modality/focus blocking, queued
completion, scalar bounds, form/native cancellation, per-document/node ownership,
replacement/drain races, synthetic-event rejection, reentry/disposal/adoption and
exact restoration. Trusted Chromium keys cover Tab/ShiftTab/Escape and restoration.
Hidden native queues can delay or abandon event delivery after node/frame retirement;
this does not establish a deadline. Inactive-document focus-event fixture wrappers
remain explicit. Evidence does not certify native-window focus, screenreaders, other
backend/platform support, application-global modality or global memory bounds.
