# 0169: Bounded composite focus and piano note ownership

Status: implemented within the local DOM profile; Chromium and macOS WebKit contracts passed.

## Decision

Provide `RavetControls.RovingFocus` for an explicit consumer-labelled composite
of 1..256 non-nested distinct members. It owns tabindex and unmodified directional
navigation, with explicit orientation, RTL and wrapping. It does not own
activation, layout, labels, modality or parameter transport. Eligibility excludes
native/ARIA-disabled, hidden/inert, detached and non-visible members. Refresh
checks the fixed member list without moving focus; explicit focus checks whether
the browser accepts it. Disposal restores original tabindex attributes.

Use this mechanism for the shared piano: one Tab stop, chromatic arrows without
wrap and Home/End endpoints. Separate keyboard play-key tracking holds one
original source until both Space and Enter are released. Focus movement releases
that source once. Native focus events are not guaranteed in an unfocused browser,
so explicit focus transitions also perform source cleanup. Disabled/hidden/inert
keys and removal release held notes at the next document mutation checkpoint;
explicit `refresh()` checks immediately. Node creation, hit testing and lifecycle
listeners bind the root's own live document/window; no-window documents reject
before markup/tabindex mutation. Frame embedding lifecycle remains consumer-owned. The observer watches relevant DOM
attributes/removal, not stylesheet/media-query changes. Consumers refresh or
release notes for those changes and still dispose before bridge teardown.

## Evidence and limits

The synthetic local fixture passes 38 focused contracts in collaborative
Chromium and hidden macOS WebKit. Real browser Tab, ArrowRight and Enter input
checks composite entry/exit and exact piano note-on/off source. No device I/O,
REAPER window or production consumer participates. The collaborative document
reports `hasFocus() === false`; there is no visual focus-ring, assistive-technology,
other platform runtime or conformance claim. Browser callback/observer work is on
the UI thread, with no real-time deadline guarantee.

Sources: [controls](../../crates/ravet-surface/web/controls.js),
[synthetic fixtures](../../crates/ravet-surface/native/tests/focus.js),
[WebKit probe](../../crates/ravet-surface/native/tests/focus.mm),
[guide](../web-controls.md).
