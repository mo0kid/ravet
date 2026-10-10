# Local control semantic contracts

RAVET's [shared controls](web-controls.md) use native form elements and explicit
ARIA state for custom controls. Consumers provide visible labels, useful names,
content, layout and focus styling. These runtime DOM contracts let a consumer
build a labelled UI; they do not certify an OS accessibility tree or screenreader.

## Custom knob disabled state

`Knob.setEnabled(boolean)` coordinates the custom slider and its optional native
number. Disabling publishes `aria-disabled=true`, removes the knob from sequential
Tab traversal and natively disables the paired number. Re-enabling restores the
owned prior tabindex and the number's original disabled attribute. Initial native
or ARIA-disabled state starts disabled; constructing inside a hidden panel does
not permanently disable the knob. An originally disabled number remains disabled
when the knob is re-enabled. Consumers own ancestor/native disabled state and
explicitly refresh after stylesheet/media changes.

Disabling first ends an active parameter gesture. A refused or throwing end leaves
programmatic enabled state and attributes unchanged, retains the original owner
and blocks all new edits and re-enable requests until an accepted `end()` retry.
`setEnabled(false)` may also retry that end. Key/pointer release and explicit
`end()` remain recovery paths. Successful disposal ends the gesture, disconnects
observation/listeners and restores original tabindex, aria-disabled and paired
number disabled attributes; rejected disposal preserves the owner for retry.
Transport callback reentry cannot mutate, dispose or replace that owner mid-send.

Disabled knobs reject keyboard, pointer, number, double-click and direct begin/
change/commit edits. No global focus move occurs: removing an already-focused knob
from Tab order does not imply that native focus is cleared. Healthy disabled knobs
remain eligible for `update(value)` subject to the existing focused/active-edit
refresh suppression. Updates and enabling reject removed/adopted nodes or paired
inputs and a closed original window. Disposal still restores already-owned
attributes as cleanup, including on retired nodes.

Original-document observation detects disabled/hidden/inert/ARIA, style/class and
membership changes; `refresh()` provides an immediate check. Unavailable live original
pairs leave Tab traversal and attempt end while preserving refused ownership. A
retired/adopted pair takes only the recovery path, with no new presentation writes.
Consumers finish/dispose before retiring the document/window. There is no cross-
origin or shadow-root traversal, automatic focus relocation or end-slot guarantee.

Before new host begin/edit messages, eligibility is checked again after existing
owner end callbacks. After an accepted message, pair ownership and edit eligibility
are checked again before display updates. If that callback retires the target,
an accepted begin remains owned and attempts end; refusal leaves pending recovery.
An accepted value/edit is already sent to the consumer host and cannot be rolled
back by this local UI. The UI returns false and leaves its old display rather than
painting a removed/adopted/disabled target. Consumers reconcile host acknowledgement.

## Meter label and formatter ownership

`AccessibleMeter` requires one connected native meter and reading element
in the same live document. Its explicit bounded label owns `aria-label` and removes
competing `aria-labelledby`, which has priority in the browser's
[accessible name computation](https://www.w3.org/TR/accname-1.1/#mapping_additional_nd_te).
Disposal restores both original name attributes exactly. Consumers supply any
visible caption and meaningfully associate related text themselves.

Formatting still accepts a frozen finite reading and returns 1..4096 nonblank
UTF-16 code units. Nested `update()` during a formatter rejects. Disposal, removal
or adoption during formatting rejects before meter value, value text or visible
reading mutation. The snapshot/visible text retains peak and RMS beyond the visual
meter's clamped range; invalid readings/formatters preserve the display.

## Evidence

The [semantic fixture](../crates/ravet-surface/native/tests/control-semantics.html)
checks all twelve components' native or explicit roles, names and current states,
plus disabled ownership, rejected-end recovery, post-callback retirement and exact
restoration in Chromium and hidden macOS WebKit. It reads DOM/native properties;
collaborative preview semantic names are automation observations, not a recorded
native accessibility tree. Trusted Chromium Tab reaches the enabled knob and
number, then skips the disabled pair. ArrowRight produces one begin/value/end on
the enabled knob and no edit on the programmatically focused disabled knob.
`document.hasFocus() === false` outside trusted routing remains explicit. Other
platforms, actual assistive technology, native-window focus and deadline/global-
memory bounds require independent evidence. See [ADR0200](decisions/0200-local-control-semantics.md).
