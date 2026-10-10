# ADR0187: Optional bounded CPU path and local-font rasterization

Status: accepted implementation contract; focused integration validation recorded in work status

## Decision

Add a default-disabled raster feature to ravet-surface and a matching portable
umbrella forwarding feature, using exact tiny-skia0.12.0 with defaults disabled
and std only. Own packed premultiplied RGBA8 CPU canvases and immutable prepared
device-space typed paths. Validate grammar, finite affine coordinates and explicit
raw operation/command/clear-covered pixel budgets before canvas mutation. Keep
source-over numeric with no hidden gamma transform, explicit fill/antialias switches
and bounded device-pixel stroke widths/caps/joins/miter.

Optional text integration prepares every local TrueType glyph outline before
mutating the canvas and validates the complete run's budgets before rendering its
first glyph. Retain no font/source borrow. Font/shaped-run/size identity stays with
the consumer; browser input and paragraph/caret preparation remain separate.

## Consequences

Default dependency graphs remain unchanged. New BSD backend/path/arrayref and MIT
strict-num notices are retained separately; no native Skia, PNG decoder, system
font loader or presentation service is supplied. Controlled public output and
input budgets do not establish global backend working-memory/flattening/time or
audio callback deadlines. Degenerate accepted geometry may produce no ink.
Positive thin antialiased strokes use the backend's modulated hairline coverage;
zero-width hairlines and dashes have no API. Stroke accounting conservatively
includes square cap corners, f32 width/miter and coverage expansion.

Independent integer rectangle/source-over and first-party fontTools glyph bounds
oracles, evenodd/winding holes, affine/curve/stroke/AA coverage and whole-fault
budget preservation define focused acceptance. Actual rendered curve/cutout/Latin/
Arabic preview is inspected separately from consumer/native editor support.
Explicit CPU readback to validated MetalBitmap is separate consumer composition,
without claiming native GPU path rasterization or window support.
