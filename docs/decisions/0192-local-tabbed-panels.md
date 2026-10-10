# ADR0192: Bounded local labelled tabbed panels

Status: Accepted

## Context

Consumer UI needs reusable tab selection and keyboard semantics with independent
panel contents. Manual focus navigation must not commit selection; automatic
activation must not notify twice when native button click follows focus. DOM
ownership must survive rejected focus, consumer reentry and detached content.

## Decision

Add `TabbedPanels` to the local controls asset, composing `RovingFocus` with
1..256 fixed native `type=button` tabs and distinct nonnested panels. Require one
live original document and consumer-supplied stable IDs. Validate all profiles
before mutation. Own bounded ARIA associations, exact hidden state and tabindex;
restore exact original values/presence on disposal.

Manual activation is default. Automatic changed focus commits once. Native button
activation preserves browser Enter/Space/click behavior. Selection commits before
optional focus and notification; guard both against reentry, allow disposal, and
check live ownership/focus after synchronous consumer handlers. Failed focus does
not roll back selection or imply a host acknowledgement. Callback errors retain
committed state with a diagnostic. Refresh preserves eligible selection or falls
back to first/null, transferring owned retiring-panel focus when possible. Quiet
unfocused consumer updates explicitly own retiring-panel focus consequences.

Observe original-document eligibility and association changes; invalid membership,
IDs, native button type, adoption or nesting retires the entire association and
restores attributes. Avoid observer self-write loops through conditional attribute
writes. Require explicit refresh for stylesheet/media changes and explicit disposal
before owner iframe/window retirement. Membership stays fixed; consumers own text,
contents, styling, layout and transport. No new dependency or native window default
is introduced.

## Validation and limits

Browser and hidden WebKit fixtures cover bounded preflight, manual/automatic
focus, single notification, null selection, owned focus fallback, reentry, callback
failure, exact restoration and iframe document routing. Trusted Chromium arrows,
Enter/Space and Tab verify native key behavior. Hidden preview reports inactive
`document.hasFocus`; deterministic focus-listener fixtures explicitly dispatch the
focus event after native focus. Evidence does not certify native-window focus,
assistive technology, other backend/platform support or deadlines. Document ID
ownership checks cannot certify arbitrary consumer markup has no duplicate IDs.
