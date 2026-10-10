# Explicit Metal offscreen drawing

The separate `ravet-metal` prototype prepares an explicitly owned macOS Metal
renderer and draws ordered filled rectangles or explicit bitmap scenes into owned
CPU readback images.
It depends on portable `ravet-surface` geometry/colors, stays outside the umbrella
processing graph, and creates no window or implicit drawing backend.

Create `MetalRenderer::new(width, height, maximum_rectangles)` inside the UI/worker
thread that will render and drop it. The renderer is neither Send nor Sync.
Dimensions are 1..4096, input capacity 0..4096, and an RGBA image is at most64 MiB.
`render(background, rectangles)` clears the complete image and applies source-over
blending. Each call independently owns its returned `MetalImage`; earlier images
remain stable across later frames and rejection.

Portable f64 rectangles are clipped to the viewport, converted to f32 endpoints,
and rasterized without antialiasing. Empty, fully clipped or quantized-degenerate
rectangles consume input capacity but generate no draw instance. Conservative
clear plus covered-pixel work is capped at67,108,864 before GPU submission. This
is a work estimate, not a driver-memory or execution-time guarantee. Creation,
shader compilation, submission, completion waiting, readback and destruction may
allocate/block and never belong in an audio callback.

Pixels are packed top-to-bottom premultiplied RGBA8Unorm. Color components retain
their numeric convention; no gamma/color-space conversion is inferred. The system
selects the Metal device. Native failure returns no partial public image and no
software fallback. Paths, text, rounded rectangles, presentation windows,
GPU sharing and other platforms remain separate APIs.

Local actual-GPU checks compare clipping, draw order, source-over blending and
orientation against an independent CPU pixel oracle, including a width whose row
stride is not256-byte-aligned. They also cover bounds/work rejection, owned-frame
stability, repeated reuse and worker-thread creation/destruction. These checks do
not establish window integration, cross-GPU conformance or a frame deadline.


## Explicit bitmap scenes

`MetalBitmap::from_premultiplied_rgba(width, height, pixels)` adopts an owned exact
packed top-to-bottom RGBA8 allocation. Dimensions are1..4096 and each RGB byte
must be<=alpha. Preparation scans all pixels on a UI/worker; invalid input consumes
the allocation. Immutable assets contain no native handles and can be transferred
between workers. `MetalImage::into_bitmap()` validates/adopts public readback fields
without copying; any caller mutation is checked again.

`render_scene(background, paints)` consumes an ordered borrowed sequence of
`MetalPaint::Rectangle` and `MetalPaint::Image { destination, source }`. The complete
image source covers the destination, proportionally clipped to the viewport,
with nearest normalized sampling and edge clamping. Destination and UV endpoints
quantize to f32; tiny UV intervals may become constant samples. Images already
contain premultiplied components; source-over blends in caller order with no tint,
gamma/color-space transform or high-quality resampling inference.

Raw scene commands consume the prepared capacity. At most16 distinct referenced
bitmap objects and64 MiB of their distinct pixel bytes are accepted, including
fully clipped images. Repeated references to one object share upload budget;
separately owned equal-pixel objects remain distinct. Clear/covered-pixel work
retains the67,108,864 limit. Complete scene/resource preflight precedes native
upload/submission. Sources stay borrowed through synchronous rendering only;
each call uploads fresh shared textures and returns independent owned readback.
Native upload/driver allocations/time are not globally bounded.

A consumer may explicitly adopt `VideoReader::frame_at(...).pixels` into a bitmap.
The media API labels its decoded pixels sRGB; the GPU path treats supplied bytes
as numeric components and performs no automatic conversion. The local integration
checks exact nearest2x upload of the public video fixture, alongside independent
mixed-image/rectangle clipping/blending and budget/reuse checks. It creates no
implicit reader, playback clock or presentation window.

## Explicit CPU raster adoption

The separate default-disabled [CPU raster](cpu-raster.md) feature prepares typed
paths/strokes and local TrueType glyphs into owned premultiplied RGBA8 bytes.
`MetalBitmap::from_premultiplied_rgba` validates those dimensions and pixels before
normal scene composition. An actual local macOS test renders a translucent curved
path and Arabic glyphs, transfers that pixel allocation and compares every GPU
readback component within one byte. This is explicit CPU rendering followed by
bitmap upload; it supplies no GPU path/text engine or native window integration.

## Explicit decoded image adoption

The separate default-disabled [image codecs](image-assets.md) return straight
RGBA8. Read dimensions, consume `into_premultiplied_rgba()` and explicitly construct
a `MetalBitmap`; Metal adds no image loading or color-management policy. The
local decoded-PNG upload/readback test checks every PMA component within1 byte,
without certifying other hardware/platforms or native window presentation.
