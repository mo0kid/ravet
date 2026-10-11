# ADR 0220: Explicit grid tracks and measured standalone content size

Status: accepted design; target validation complete, public promotion pending.

## Decision

Reuse the bounded axis allocator independently for explicit rows and columns.
Resolve caller-supplied nonzero checked spans into atomic caller-owned output,
with 32 tracks per axis and 256 areas. Keep overlap explicit, no automatic
placement or intrinsic content dependency, and no heap allocation in geometry.

Expose a read-only macOS `StandaloneWindow::content_size` measurement through its
existing main-thread owner. User-resize minima and native aspect fitting remain native policy;
consumer layout uses measured content extent after pumping instead of assuming
requested size. No new window owner, resize callback, routing policy or focus
behavior is introduced. Audio/MIDI settings retain their enabled default.

Demonstrate the two APIs together in a complete ordinary HTML workspace consumer.
Publish one latest fixed-field geometry snapshot and defer presentation until the
WebKit viewport agrees, without asking the native coalescing bridge to redeliver
an identical JSON state. Retain last valid geometry on insufficient room. The
explicit hidden window fixture uses only process-local settings and no devices.

## Limits and evidence

The layout is an explicit geometry algorithm, not CSS Flexbox/Grid conformance.
Consumer breakpoints, layering, clipping and content measurement remain explicit.
Native content measurement and the complete window consumer are macOS only.
Actual browser/native measurements are separate from portable allocator tests;
none imply screen-reader, hardware, DAW or real-time deadline certification.

Five portable groups pass, with zero allocations over10,000 resize transitions.
Actual Chromium transport/geometry/trusted-input checks and two actual owned
hidden windows validate12 measured viewport transitions, standard popup
composition and teardown. Programmatic small sizes can bypass user-resize minima;
undersized measurements retain the exact previous presentation with a diagnostic.
Native aspect fitting rounds at logical-point precision; consumers sample actual
bounds. Browser/native results do not imply native AX/screen-reader certification.
