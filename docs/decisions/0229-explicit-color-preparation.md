# ADR 0229: Explicit encoded-space alpha and linear-light preparation

- Status: accepted
- Package: W216

## Context

Drawing, decoded images, CPU raster and Metal scenes preserve numeric components.
Applications need explicit standard transfer and linear-light image preparation
without implicitly changing existing renderer output or inferring file profiles.

## Decision

Add dependency-free checked linear scalar colors and immutable bounded RGBA views
with caller-selected sRGB/linear-sRGB and straight/premultiplied association.
Association is in the declared encoded space; unpremultiply before source transfer
and premultiply after output transfer. Source-over computes linear-light RGB and
linear alpha. Completely transparent composites canonicalize RGB to zero, while
ordinary straight conversion preserves hidden RGB.

Preflight shape/profiles/output before writes, keeping caller-owned buffers and
safe nonaliasing borrows. Identical profiles copy exactly; same-encoding association
uses exact integer half-up arithmetic, avoiding floating transfer-roundtrip ties.
Changed transfer uses f64 and byte quantization with explicit lossy linear storage.
Source-over uses normalized contribution weights to preserve scalar RGB at
subnormal alpha when compositing over transparent.
No implicit rendering, decoder metadata, native global profile or file/network I/O.

Consume the reusable APIs in a standalone labelled image-preparation workspace,
with bounded local messages, checked model revision, Rust-produced full image
arrays, normal resizing/default settings popup, headless export and scoped native
fixture. Keep fixture-only browser transport ownership separate from Rust tokens.

## Evidence and limits

Nine numeric/allocation groups and two consumer-message groups, independent
70-digit Decimal 256 transfer/4096 conversion/2048 composite checks, actual owned
WK 11 consumer groups and trusted Chromium Rust-data/control/resize receipts cover
the scoped behavior. The independent oracle exposed an alpha reassociation tie;
exact rational integer arithmetic plus a dedicated regression resolves it.

Cap dimensions 4096 per axis, pixels 64 MiB, consumer images 32x8/snapshot 16 KiB/messages
64 bytes/two palettes. Working/native/libm time and consumer allocations remain
external to callback deadlines. sRGB primaries/transfer only: ICC, Display-P3, HDR,
monitor calibration, implicit profile embedding/display transformation, SVG/font
facilities and platform/screen-reader/device certification remain independent.
