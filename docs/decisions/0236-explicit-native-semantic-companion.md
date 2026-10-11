# ADR0236: Explicit native semantic companion for custom editor content

Status: accepted implementation profile, platform evidence bounded below.

Custom editor content needs reusable native roles, labels, values, frames and
guarded actions in addition to browser semantics. Native inspection of a remote
WebKit content proxy did not provide content access in the owned fixture profile.
DOM assertions cannot substitute for native protocol evidence.

Publish an explicitly authored complete content hierarchy through copied checked
`AccessibilitySnapshot` values. Flat static-text/button/slider nodes belong to the
original native container, attached parent/window and installation revision.
Optional installation replaces only the container's automatic accessibility
children; None restores them and the default standalone title-bar settings popup
remains independent. No arbitrary HTML traversal or implicit semantic inference.

AppKit element subclasses adopt public role-specific protocols and use weak
original-owner references. Retired elements preserve immutable descriptive data,
while parent/frame/focus/actions become inert. Complete publications have fresh
native objects/revisions; invalid input and exhaustion preserve the previous
tree. Clearing remains possible at revision exhaustion and does not create a new
installable revision. Root/native transitions and the existing Rust callback
borrow/panic/lifetime guards protect reentry and synchronous self-destruction.
Callbacks accept intent; later explicit native and WebKit model acknowledgement
owns presentation. Accepted consumer effects cannot be rolled back by the wrapper.

The model is allocating UI work with64-node/65536-string-byte limits and finite
bounded geometry. It makes no elapsed-time/global-allocation claim. Native owner
ancestor inspection is capped at32. Consumer-reported focus moves no global/page
focus. Value/focus setters, editable text, nested trees/tables and arbitrary roles
remain outside this profile; existing foreign callback layout is unchanged.

Owned native protocol fixtures and a shared Rust/native/WebKit consumer provide
actual getter/action/frame/transport/retirement evidence. An injected AppKit key is
an owned fixture event, not human or assistive-technology input. Direct protocol
execution and notification posts do not certify system AX discovery, delivery,
VoiceOver or other screen readers. Those validation criteria remain separate.

See [native semantics](../native-semantics.md) and Apple's
[public custom-control contracts](https://developer.apple.com/library/archive/documentation/Accessibility/Conceptual/AccessibilityMacOSX/ImplementingAccessibilityforCustomControls.html).
