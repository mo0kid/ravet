# 0005: Local web content in a native editor

Status: macOS feasibility prototype. No portable editor API or DAW support claim.

## Context and decision

Private consumers need an editor without importing their product widgets or
assets into the framework. Prototype an independently built Cocoa `NSView` that
owns a `WKWebView`, loads local consumer assets, and exchanges JSON on the main
thread. Use the system WebKit with a small native window shim. Keep native
pointers and platform SDK code outside the safe DSP contract. Product controls,
layout, activation, and state stay private.

## Consequences and validation

The caller owns view/context lifetime and main-loop integration. A 50 ms timer
can support control display updates but cannot replace a real-time parameter
handoff. Navigation restrictions do not sandbox consumer JavaScript networking.
The C++ prototype is built with CMake and ARC, independently of Cargo; a safe Rust
binding and other platform backends remain open. The main-loop lifecycle probe
checks message/snapshot delivery and callback teardown. Host embedding, focus,
resize policy, accessibility, CPU use, and supported deployment targets need
further validation. See [native editor guide](../native-editor.md).

## Painting and display updates

Use an opaque, layer-backed container with a configurable backing colour so
host redraws do not expose an unrelated default view colour. Keep at most one
snapshot JavaScript evaluation in flight, and suppress identical JSON after a
successful delivery. This prevents a steady polling timer from forcing repeated
page mutations. Consumer pages still own their rendering and should update only
changed controls. The lifecycle probe checks these mechanics; visual stability
in a particular host still requires testing in that host.
