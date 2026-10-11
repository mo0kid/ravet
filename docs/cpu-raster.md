# CPU path and font rasterization

Enable default-disabled `ravet-surface/raster` (or `ravet/raster`) for owned CPU
pixel canvases. It pins tiny-skia 0.12.0 with defaults disabled and `std` only.
The independent [SVG image importer](svg-images.md) unifies tiny-skia std/SIMD/PNG
when both features are selected; the raster-only configuration remains std only.
Geometry is prepared from typed `PathCommand` values, with an explicit finite
affine transform applied before f32 quantization. Device-space control coordinates
must remain within 65536 magnitude; each path accepts 1..4096 commands. New
subpaths follow `MoveTo`; a closed subpath requires another `MoveTo` before further
segments. Empty and quantized-degenerate geometry consumes its input budgets
without guaranteeing visible ink.

`RasterCanvas` starts transparent and owns packed top-to-bottom premultiplied
RGBA8. Dimensions are 1..4096 pixels, at most 64 MiB. Each clear resets per-frame
budgets: 0..4096 configured draw operations, 65536 cumulative path commands and
67108864 conservative clear/covered pixels. Clear charges the complete canvas;
each path charges its clipped control-point bounds with coverage padding. Stroke
accounting uses quantized width/miter, square-cap expansion and an extra pixel.
These limits govern input, output and accounted work; backend allocations,
flattening and execution time require separate measurement. Use these allocating
APIs on UI or worker threads.

`fill_path` selects `NonZero`/`EvenOdd` and an explicit antialias flag.
`stroke_path` uses device-pixel width (0,256], miter limit [1,16], butt/round/square
caps and miter/round/bevel joins. Positive subpixel antialiased strokes use the
backend's modulated hairline coverage. Zero-width hairlines and dashes have no API.
Stroke width remains in device pixels after geometry transformation. Whole budget
and style rejection preserves every pixel and counter. Source-over operates on
numeric RGBA components without a gamma or color-space transform.

With both `raster` and `text` enabled, `RasterText` prepares every cached run's
TrueType outline before canvas mutation. Supply the same `TextFont` and size used
for shaping, and an explicit baseline. It owns geometry without font/source
borrows: up to 16384 glyphs and 65536 commands per run, each path at most 4096
commands. Missing glyph zero uses the supplied font's replacement outline.
CFF/Type1/color/bitmap fonts and system fallback are outside this API.
`draw_text` validates whole-run budgets before any glyph changes the canvas and
consumes one operation including space-only runs. Browser input and
[paragraph/caret layout](surface-drawing.md) remain separate preparations.

`snapshot` copies an independent owned readback; `into_image` transfers the pixel
allocation. A consumer can explicitly construct a `MetalBitmap` from these
dimensions and bytes or provide another presenter. The raster feature creates no
window, audio device or clock. [Metal composition](metal-drawing.md) validates its
own asset and scene limits; CPU path preparation does not supply GPU path rendering.

The public example writes a 480×240 packed RGBA8 frame to standard output:

```sh
cargo run -p ravet-surface --features raster,text --example raster-preview --locked > target/raster.rgba
cargo test -p ravet-surface --features raster,text --test raster --locked
cargo test -p ravet-metal --test raster --locked
```

The final command exercises an actual local macOS Metal device. Other-platform
compiler checks establish compilation only. Focused pixel tests compare integer
rectangle clipping/source-over against independent equations and a synthetic font
against independently extracted glyph bounds; winding holes, curves, caps,
antialiasing, rejected budgets and prepared font ownership are covered separately.
Backend notices are retained in [the raster notice file](../crates/ravet-surface/third-party/raster.md)
and [licensing](../LICENSING.md). See [decision 0187](decisions/0187-optional-cpu-raster.md).

`RasterText::from_fallback` prepares whole matched-face geometry from the optional
[local font stack](font-fallback.md), with the same finite/aggregate path limits.
The output retains no inferred system font or paragraph layout policy.
