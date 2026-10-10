# ADR 0190: Explicit one-level popup menu ownership

Status: implemented within the local DOM profile

## Decision

Compose fixed consumer-created native action buttons with the existing roving-focus
helper. Bound membership to 256 buttons and explicit unique action/label strings.
Validate live owner-document membership, button types and complete profile before
attribute mutation. Consumer CSS owns placement and clipping.

Own temporary menu semantics, hidden state and exactly one Tab stop; preserve and
restore exact original roles, label references, expanded state and tabindex values.
Use native click activation for Enter/Space rather than duplicating it in keydown.
Guard the complete close/focus-restoration/consumer-callback operation against
reentrant opening and allow disposal; focus-time disposal cancels the action.

Retain browser Tab navigation and close after its focus destination is selected.
Owner focus events, a Tab keyup fallback and explicit refresh support DOM surfaces
with different focus-event delivery. Outside pointer actions keep their native
default behavior. One pointer identity and one next-task intent reset disambiguate
trigger clicks after focus transfer, including outside release/cancellation.
All close/restoration paths temporarily guard reopening while retaining intentional
disposal. Verify live open ownership/actual selection after synchronous opening
focus handlers so teardown cannot report stale success. Original-document/window
listeners own dismissal and teardown.

## Consequences and evidence

The helper supplies one level of actions, without submenus/typeahead, checked/radio
entries, OS menus, modal focus traps, viewport placement or transport/host policy.
Consumers validate meaningful button labels and dispose before retiring their frame.
Actual Chromium and hidden WebKit DOM fixtures check bounds, focus, exact actions,
callback guard/disposal, pointer cancellation, restoration and iframe ownership.
Trusted Chromium key cases skip disabled entries, activate once, restore on Escape
and exit with Tab. The automation blurs its hidden window after each key; cases
explicitly reopen and capture keyup before that expected dismissal.
`document.hasFocus=false` does not establish native-window focus or screen-reader
support. See [local popup menus](../popup-menus.md).
