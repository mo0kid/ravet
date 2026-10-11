# Explicit standalone content constraints

`ravet_webview::ContentConstraints` is a portable, dependency-free value and size
resolver. On macOS, `StandaloneWindow::set_content_constraints` installs that
policy on the existing main-thread window owner. The policy is opt-in; window
creation and the standard title-bar Audio/MIDI popup keep their existing defaults.

```rust
use ravet_webview::ContentConstraints;
let policy = ContentConstraints::new(640., 480., 1600., 1200.).unwrap();
assert_eq!(policy.resolve(320., 240., None), Some((640., 480.)));
assert_eq!(policy.resolve(1600., 1000., Some((4., 3.))),
           Some((4000. / 3., 1000.)));
```

Minimum and maximum widths must be finite values in 320..=4096 logical points;
heights must be in 240..=4096, with minimum <= maximum on each axis. Bounds may
be fractional or equal. Invalid construction returns `None`. The copied value
has private fields and reports its bounds with `minimum_size` and `maximum_size`.

`resolve(width, height, aspect)` accepts any finite positive requested extent.
Without an aspect it clamps each dimension independently. A finite positive
aspect pair defines a common scale interval from both bounds. The resolver fits
the largest matching rectangle within the request, then clamps its scale into
that interval: an undersized request can grow to the minimum. Empty or
unrepresentable scale intervals, invalid requests and invalid aspects return
`None`. Final products are clamped to bounds to contain floating-point rounding.
The function allocates nothing and has no native, threading or I/O dependency;
it does not certify a real-time deadline or define display-pixel rounding.

## Owned native installation

On an existing macOS `StandaloneWindow`, call
`set_content_constraints(Some(policy))`. The wrapper reads actual content size,
preflights compatibility with the window's original optional aspect ratio,
installs native content minimum/maximum properties, and immediately requests the
resolved extent. Invalid or incompatible proposals return `false` before any
Rust/native policy or geometry write. Native installation widens the old maximum
before setting the new minimum and final maximum, so a smaller replacement does
not temporarily invert the admitted bounds.

`content_constraints()` returns the last admitted copied policy. Both installs
and `set_size` reject synchronous reentry while either operation is in progress.
A resize notification emitted during installation sees the prior admitted
policy; the new value is published only after the native call succeeds. This
prevents an older outer operation from overwriting a nested accepted policy.
The main-thread owner, editor-before-parent borrowing and teardown remain in
force. Once a native operation is admitted, arbitrary foreign Cocoa observer
effects cannot be rolled back by RAVET.

After [window close admission](window-lifecycle.md), policy installation and
resize reject, `content_size()` returns `None`, and the last admitted copied
policy remains readable. Close requests share the synchronous operation guard.

`set_size` retains its finite 320..=4096 by 240..=4096 request limits. An installed
policy resolves those requests before native fitting. `None` restores the exact
native user-resize content minimum/maximum captured at creation without making
a new size request. Subsequent programmatic sizing follows the original
no-policy behavior, including requests below the native user-resize minimum.

Native user constraints, programmatic requests and actual measurements are
different observations. Cocoa can independently round dimensions to logical
points: measured content may differ from policy bounds or the compatible aspect
projection by up to one point. Read `content_size()` after pumping and lay out
the measured viewport; do not treat the requested size as an exact result.
These checks do not establish human drag behavior or another native backend.

## Complete consumer and validation

The interactive layout workspace installs 640 by 480 through 4096 by 4096 content
bounds before embedding its measured grid page:

```sh
cargo run -p ravet-examples --bin layout_workspace --locked
```

Its existing `--fixture` continues to exercise unbounded programmatic sizing and
undersized presentation retention. The separate explicit constraint fixture
builds the framework sources and only its own hidden windows:

```sh
python3 scripts/check-window-constraints.py
```

An optional `--runner` accepts an executable runner command; it is parsed into
arguments and never passed to a shell. The fixture requires macOS 12 or later,
Rust and Xcode command-line tools. It uses process-local settings, opens no
audio/MIDI devices and removes its exact-window notification observers before
dropping the borrowed contexts or editors. Informational min/max queries scan
at most eight windows in its own process and match a unique bounded fixture
title; they do not inspect other applications or accessibility settings.

Five portable numerical groups cover finite bounds, free/aspect fitting,
incompatible/extreme requests and deterministic sweeps. The existing tutorial
allocation-counter fixture measures 19,998 resolver calls with zero allocations.
Two actual owned hidden windows cover 22 measured viewport/grid transitions,
four undersized states with unchanged prior presentation, exact native-property
readback, incompatible proposal rejection and restoration without another size
request. Four actual synchronous resize callbacks reject nested policy and size
operations while preserving admitted readback. Both windows retain the standard
popup and editor-before-parent cleanup. This is native property, callback and
WebKit geometry evidence; no human drag, device, DAW, accessibility-tree or
screen-reader certification is inferred.
