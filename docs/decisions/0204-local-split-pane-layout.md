# ADR 0204: bounded local split-pane layout ownership

Status: Accepted

## Decision

Supply an actual two-pane CSS grid component with one labelled separator and
fixed direct-child/ID associations. Bound direct root child nodes to64, reject non-CSS-collapsible direct text including NBSP and suspend in-flow generated root pseudo-content so anonymous grid items cannot displace members. Pane/separator content remains consumer-owned. Pixel minima clamp a bounded fraction while
the measured extent fits them. An undersized root retains clipped fractional
tracks and explicitly suspends divider input with TooSmall rather than forcing
unbounded document overflow or claiming a native window minimum.

Keep coordinates conservative: reject all member/root-ancestor CSS transforms,
individual transforms and non-default zoom, and cancel held pointer ownership
when available extent changes. Keyboard direction is tied to the explicit axis
and RTL profile. One original document/window owns every event and observer;
association mutation retires, unavailable layouts suspend, and queued disposed
work has no effect.

Record exact owned inline longhands/priorities and separator attributes. Bound
original per-node style text to16384code units and owned values to4096; preserve
unrelated consumer edits during disposal. Restore raw style serialization only
when no unrelated style edits remain. Guard callbacks and focus transitions,
allow disposal, and revalidate synchronous consumer mutations before success.
Already committed/notified state cannot be rolled back when a callback throws.

## Evidence and limits

37 Chromium and hidden WKWebView groups pass actual grid geometry and lifecycle
contracts; trusted Chromium Tab/arrow/Home/End pass. Hidden Chromium hasFocus is
false outside key delivery. Pointer/focus reentry fixtures are synthetic.
No native pointer, accessibility-tree, screen-reader, transformed coordinate,
shadow/cross-origin, window minimum or deadline support is established.
No new dependencies or audio-thread/API coupling are needed.
