# ADR 0223: Opt-in bounded standalone content constraints

- Status: accepted
- Package: W210

## Context

Standalone applications can measure actual content size and consume explicit grid
geometry. Native user-resize minima do not necessarily clamp programmatic
requests. Applications need one reusable policy for bounded content extents and
aspect-compatible fitting while preserving existing window defaults.

## Decision

Add a dependency-free portable `ContentConstraints` value with private finite
min/max fields, checked construction and allocation-free resolution. Widths are
320..=4096, heights 240..=4096. Explicit optional aspect pairs resolve through a
shared finite scale interval. Empty/unrepresentable intervals reject; final
products clamp scalar rounding into the admitted bounds.

The existing main-thread `StandaloneWindow` owns an optional copied policy and
an operation guard. `Some` preflights actual size and aspect compatibility before
native writes, sets content min/max safely, requests immediate fitting, then
publishes the admitted policy. Size and policy operations reject synchronous
reentry; callbacks during an install read the prior admitted policy. This avoids
an outer operation overwriting a nested accepted update.

`None` restores captured creation-time native content bounds without requesting
size. No-policy programmatic behavior, original aspect configuration, standard
Audio/MIDI popup and editor-parent ownership remain unchanged. Foreign Cocoa
notification side effects after native admission cannot be rolled back.

## Evidence and limits

Five pure numerical groups and 19,998 zero-allocation resolver calls exercise
bounds, fitting and rejection. A reusable explicit hidden-window consumer checks
actual Cocoa properties, 22 measured WebKit viewport/grid transitions, four
synchronous callback reentry rejections, incompatible installation atomicity,
restoration without a size request and default-popup/editor-parent lifecycle.

Native dimensions round independently; the fixture compares measured bounds and
the compatible policy projection within one logical point per dimension. This is
measured property/programmatic evidence, separate from human resize drag. The
value/resolver is portable; standalone installation remains macOS only. No device,
DAW, screen-reader or real-time deadline support follows from these checks.
