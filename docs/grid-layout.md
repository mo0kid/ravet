# Explicit grid layout and a resizable workspace

`ravet_surface::layout::layout_grid` resolves independent column and row tracks
into caller-owned `Rect` output. Use `LayoutItem` for minimum, preferred, maximum
and growth weight, and `GridArea` for explicit zero-based placement and nonzero
spans. A spanning area includes its covered tracks and internal gaps. Input area
order is preserved; overlapping areas are allowed and paint order belongs to the
consumer.

```rust
use ravet_surface::layout::{GridArea, LayoutItem, Rect, layout_grid};
let bounds = Rect::new(0., 0., 800., 600.).unwrap();
let mut panels = [bounds; 2];
let columns = [LayoutItem::fixed(180.), LayoutItem {
    minimum: 220., preferred: 400., maximum: 1600., grow: 1.,
}];
let rows = [LayoutItem::fixed(48.), LayoutItem {
    minimum: 180., preferred: 400., maximum: 1600., grow: 1.,
}];
layout_grid(bounds, &columns, &rows, 8., 8., &[
    GridArea { row: 0, column: 0, row_span: 1, column_span: 2 },
    GridArea { row: 1, column: 1, row_span: 1, column_span: 1 },
], &mut panels).unwrap();
assert_eq!(panels[0].width(), 800.);
```

Each axis requires 1..=32 tracks. At most 256 areas are accepted, with an exactly
matching output length. Empty area lists still validate and resolve both axes.
All spans use checked ranges; zero, overflowing and out-of-range spans are
invalid. Gaps and track values must be finite and nonnegative, with minimum <=
preferred <= maximum. Each axis follows `layout_axis`: proportional shrinking
above minima, weighted growth with maximum-cap redistribution, and trailing
unused space when no track can grow. Every failure preserves the entire output,
including when the second axis cannot fit. Scratch storage is fixed on the stack;
no allocation, I/O or native dependency occurs in the geometry function. This
contract does not certify a real-time deadline.

Placement, nested grids and responsive breakpoints are explicit consumer policy.
There is no intrinsic content measurement, automatic placement, wrapping, CSS
conformance, pixel rounding, clipping or inferred display scale.

## Standalone consumption

On macOS, `StandaloneWindow::content_size()` reads its owned content view's actual
finite positive dimensions in logical points, or returns `None`. It follows the
existing main-thread owner and reads without resizing or emitting callbacks.
Sample after `pump`; a `set_size` request can differ from measured size because
Cocoa applies native fitting and aspect constraints. User-resize minimum sizes
are not a guaranteed clamp for programmatic requests: the owned-window fixture
actually reaches an undersized viewport, and verifies the diagnostic with exact
retention of the previous panel geometry. `StandaloneEditor` still
borrows its parent window, and standard title-bar Audio/MIDI settings remain
enabled unless `StandaloneOptions::io_settings` is explicitly false.

Run the complete consumer with:

```sh
cargo run -p ravet-examples --bin layout_workspace --locked
cargo run -p ravet-examples --bin layout_workspace --locked -- --fixture
```

The interactive example installs explicit 640 by 480 through 4096 by 4096 content
constraints; its `--fixture` keeps no-policy sizing to test undersized states.
See [standalone content constraints](window-constraints.md) for the reusable policy,
native callback guard and separate exact-window validation command.

The interactive example uses labelled HTML header, library, editor, inspector and
status panels. Rust selects a two- or three-column grid from measured width, and
publishes bounded fixed-field geometry through the existing snapshot bridge only
when measured size changes. JavaScript retains one latest pending snapshot until
the actual viewport agrees within one logical point, then applies all five panel
bounds; resize retries that snapshot. This fixed page rejects panel boxes below
26 points, which cannot contain its owned padding/border; Rust track minima exceed
that presentation bound. There is no growing queue or background
polling thread. Insufficient room retains the last applied geometry and presents
a diagnostic. No audio/MIDI devices are opened; the default popup shows the
application's truthful unavailable-device state.

The explicit `--fixture` creates only its own hidden native windows with
process-local settings, validates actual measured WebKit panel geometry across
wide/tall/small/aspect-fitted and rapidly coalesced size changes, and exercises
the default popup. It drops each editor before its window. This is an explicit
window fixture, separate from headless DSP validation. The UI consumer is macOS
only; portable grid geometry does not imply another native backend, device,
DAW, screen-reader or deadline certification.

Five portable groups pass constraints/spans/subpixels/capacities/atomic errors and
10,000 resize transitions with zero measured allocations. Actual Chromium checks
cover four Rust-derived grid viewport sizes plus undersized retention, seven
fixed-field transport groups and trusted text/checkbox input retained across
resize with focus/selection intact. Two actual owned hidden macOS windows cover
12 measured viewport transitions (including two undersized states), default
popup open/close, invalid-request preservation and editor-before-parent teardown.
Native aspect results are measured at logical-point precision rather than
asserting an exact requested ratio after Cocoa rounding. These are scoped browser
DOM/native-window measurements; no native accessibility tree or screen-reader
validation is inferred.
