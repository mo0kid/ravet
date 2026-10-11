# ADR0208: bounded fixed flat-list selection ownership

Status: Accepted

## Decision

Implement one labelled root Tab stop and virtual active-option reference for
1–256 fixed direct option members, separately from single/multiple selected
indices. Explicit bounded initial selection establishes the caller's model in
hidden tabs; later admission cannot newly select unavailable options. Eligibility
refresh keeps selection for model continuity, records unavailable selected IDs,
and updates active/Tab fallback quietly. Explicit accepted replacements may
remove retained unavailable members. Fixed association/profile mutation retires.

Use a bounded additive Shift-range baseline that never newly adds unavailable
members. Eligibility changes, non-range intent and Shift lifecycle clear it.
Notify only actual selected changes with frozen bounded indices/IDs/unavailable
IDs and active/source state. Guard callback/focus transitions, revalidate original
ownership, and preserve committed consumer state when later focus/notification
fails. Exact original owned attrs restore on idempotent disposal; observers are
original-document scoped and queued disposed work is inert.

## Evidence and limits

32 actual Chromium/hidden WK groups and trusted Chromium Tab/ArrowDown/Space
prove local DOM keyboard and state contracts. Pointer/focus reentry is synthetic;
hidden preview hasFocus=false outside key delivery. This does not establish a
native accessibility tree, screen-reader/native pointer/native-focus certificate,
a dynamic/virtualized list, tree/table/typeahead, global shortcut or deadline.
No new dependency or audio-thread coupling is supplied.
