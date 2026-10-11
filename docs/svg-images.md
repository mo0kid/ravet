# Owned SVG image import

Enable default-disabled `ravet-surface/svg-image` or umbrella `ravet/svg-image`
for `svg::{SvgDocument, SvgImage, SvgFit, SvgError}`. Supply UTF8 SVG bytes to
`SvgDocument::parse`; the result owns normalized geometry with no source, file,
font, editor or window borrow. Reuse it for multiple explicit pixel sizes.
`intrinsic_size()` returns the resolved root viewport, not painted bounds.
`render(width, height, fit)` creates a separate transparent image: `Contain`
centers the aspect-preserving viewport; `Stretch` scales it independently on each
axis. These outer fits follow the source's own viewBox/preserveAspectRatio mapping.

`SvgImage` has private dimensions/pixels, immutable accessors and consuming
`into_premultiplied_rgba()`. Its top-to-bottom packed RGBA8 is sRGB-encoded and
premultiplied in that encoded space. The renderer uses numeric encoded component
compositing. [Explicit color preparation](color-preparation.md) supplies separate
association/transfer/linear-light operations; no ICC or monitor profile is inferred.
Output is1..4096 pixels per dimension, at most64MiB, with RGB<=alpha verified before
publication. Whole failures publish no partial image and preserve previous assets.

Use these allocating APIs on UI/worker threads. Source/tree/path/output limits
bound framework admission, not all parser/converter/layer/flattening buffers,
process memory, allocator failure or execution time. They supply no real-time
callback guarantee, cache, clock, native presenter, window or audio device.

## Fixed input profile

Source is1..1MiB UTF8 XML, no SVGZ. XML admission allows8192 total nodes (including
text/comments),4096 elements,32 element levels,64 attributes/namespaces per element
and65536 bytes per attribute. At most1024 distinct ASCII IDs have1..128 bytes and
use letters/digits/underscore/hyphen/dot. The exact SVG namespace is required;
namespace-qualified attributes, nested SVG viewports, duplicate IDs, processing
instructions and the literal `<!DOCTYPE` sequence are rejected. XML declarations
and predefined/numeric character references remain legal. DTD parsing and external
entity resolution are disabled. Text content is permitted in `title`/`desc`; other
text must contain only ASCII XML whitespace. Metadata role/aria labels are inert
strings, with no accessibility certification implied.

Supported elements are `svg`, `g`, `defs`, `path`, `rect`, `circle`, `ellipse`,
`line`, `polyline`, `polygon`, `linearGradient`, `radialGradient`, `stop`, `clipPath`,
`title` and `desc`. Element-child profiles are checked; gradients contain stops,
clip paths contain only shapes/transformed groups, and shapes contain no elements.
Root width/height use finite positive unitless/px values, or each missing dimension
comes from a valid viewBox. Positive root/viewBox dimensions and all source numeric
coordinates are<=65536 in magnitude. Root viewBox, standard preserveAspectRatio
alignments/meet/slice/none, and matrix/translate/scale/rotate/skew transforms are
explicit. Physical/font units, shape percentages and environment-dependent root
fallback are rejected. Gradient coordinates and stop offsets may use percentages.

Path grammar includes absolute/relative move, line, horizontal/vertical, cubic,
quadratic, smooth cubic/quadratic, elliptical arcs and close. Each source and
normalized path accepts<=4096 segments, with<=65536 cumulative source and normalized
segments. Relative accumulation is checked in normalized geometry. Points accept
<=4096 coordinate pairs. Valid degenerate/quantized/invisible shapes do not promise
visible ink; an empty supplied path `d` is rejected. Each transform list has<=256
expanded tokens; values/composed transforms/normalized transformed path controls
must remain finite and within65536 magnitude. This is a bounded image profile,
not complete browser SVG support.

Presentation attributes and inline style support fill/stroke/color/currentColor,
fill/stroke/overall/group opacity, fill/clip winding rules, stroke width/caps/joins/
miter/dashes, transform, display and visibility, stop color/opacity and clipping.
Gradient coordinates/units/transform/spread and stop offsets use SVG attributes;
inline styles do not admit those geometry attributes. Stroke width is0..256, miter1..16, dash arrays<=256 unitless entries in
0..256 with positive total, and dash offset magnitude<=256. Opacity is a unitless
0..1 number or inherit; line caps are butt/round/square and joins miter/round/bevel.
Gradients select userSpaceOnUse/objectBoundingBox, finite gradientTransform and
pad/reflect/repeat. Stop offsets are0..1, <=256 stops per gradient/4096 total.

Inline style uses<=32 semicolon-separated declarations. Every nonempty declaration
must have exactly one colon and one matching parsed token for a supported
presentation property. Unknown/malformed declarations, important, escapes,
comments, custom variables, braces and at-rules are refused. This deliberately
narrow syntax does not permit semicolons/colons inside values. Inline declarations
override presentation attributes; ordinary supported inheritance remains explicit.
Selector stylesheets/style elements and injected CSS are outside this API.

Paint and clipping may reference only validated local IDs with typed `url(#id)`
syntax (including quoted local IDs); paints target linear/radial gradients and
clips target clipPath. Gradient href inheritance, clip references inside clip
subtrees and all resource-bearing href attributes are refused. Normalized traversal
counts<=8192 visits including repeated referenced clip subroots and<=32 levels.
Text/fonts, use/symbol instances, images/data images, patterns, masks, filters,
foreignObject, scripts/handlers, anchors and animation are rejected before backend
conversion. Both image resolver closures unconditionally return None, even though
the backend's defaults permit local-file loading. No consumer options expose a
resource directory, font database, external stylesheet or resolver.

## Consumer and validation

The complete tutorial imports the first-party outlined logo and authored gradient/
arc/clip/opacity and integer-rectangle images. Rust prepares every image and uses
`RgbaView` for explicit encoded premultiplied-to-straight conversion; JavaScript
only presents the resulting bytes. Native labelled source/fit/size controls send
bounded local messages, with complete model replacement and monotonic revision.
Its18 fixed snapshots are each<65536 bytes, using base64 with128×64,96×48 or160×48
pixels. Rejected transitions retain the admitted model/image. This teaching bridge
accepts only its exact compact one-field messages and is not a general JSON API.

```sh
cargo test -p ravet-surface --features svg-image --test svg_image --locked
cargo run -p ravet-examples --bin svg_workspace --no-default-features --features svg-image --locked -- --export
cargo run -p ravet-examples --bin svg_workspace --features svg-image --locked
cargo run -p ravet-examples --bin svg_workspace --features svg-image --locked -- --fixture
```

Native mode requires macOS/live-ui; export creates no window. Standalone mode keeps
the default title-bar Audio/MIDI settings popup; this image consumer reports no
owned devices. Embedded plug-in routing remains host-owned. The explicit owned
hidden WK fixture checks18 actual Rust image/control transitions against a separate
native SVG-image raster oracle, invalid transition retention, three measured
resizes/focus, popup and parent/editor/context cleanup. Synthetic native DOM events
prove transport rather than trusted human interaction.

The [first-party fixture provenance](../crates/ravet-surface/tests/fixtures/svg/README.md)
and source tests cover ownership, exact capacities, viewport fitting, all path
families, style inheritance, transforms, gradients, clipping, opacity and resource
rejections. Browser/WK oracle comparison does not call resvg or its export path.
Aligned integer rectangles compare exact bytes; other fixtures require mean
absolute component error<=3, <=5% pixels with component error>32, ink bounds<=2
pixels and nonblank ink ratio0.95..1.05. These are fixture tolerances, not a general
SVG fidelity guarantee. Trusted browser source/fit/size keys use a separately owned
frontend model table of Rust-prepared bytes, distinct from the actual Rust native
owner. Browser picker traversal may temporarily yield document.hasFocus=false;
DOM focus/image retention is distinct from native focus or screen-reader proof.

## Dependencies

Exact resvg/usvg0.48.1 uses the existing tiny-skia0.12.0. Direct exact roxmltree0.21.1,
svgtypes0.16.1 and simplecss0.2.2 validate the bounded profile. Direct defaults are
disabled, but transitive feature unification enables roxmltree positions/std and
tiny-skia std/SIMD/PNG. Combining `raster` with this feature therefore adds those
backend flags. Text/system-font/memmap/SVGZ/writer/raster-image features remain off;
SVG image support does not select RAVET's `image` feature. PNG dependencies are
selected internally by tiny-skia, without granting SVG image-element admission.
Complete exact [notices](../crates/ravet-surface/third-party/svg.md),
[licensing](../LICENSING.md) and [decision0232](decisions/0232-owned-svg-image-import.md)
record the backend boundary. See the primary [SVG viewport mapping](https://www.w3.org/TR/SVG2/coords.html#ComputingAViewportsTransform),
[paint grammar](https://www.w3.org/TR/SVG2/painting.html#SpecifyingPaint) and
[backend API](https://docs.rs/resvg/0.48.1/resvg/fn.render.html).

All workspace features additionally unify bytemuck derive/aarch64 SIMD,
numeric traits/euclid and zlib-rs through shared dependencies. This does not
enable usvg text/system-font/raster-image features or expand SVG admission.
The [retained notices](../crates/ravet-surface/third-party/svg.md) cover every
resolved normal SVG dependency in that configuration.
