# 0232: Owned bounded SVG image import

Status: accepted for the scoped optional implementation.

Typed SVG export and CPU prepared paths remain independent. Image consumers need
an imported SVG asset with reusable viewBox/shape/paint preparation, explicit
output pixels and no implicit font/file/window ownership. Default-disabled
`svg-image` uses exact resvg/usvg0.48.1 with current tiny-skia0.12.0; it does not
select text or RAVET image codecs.

RAVET preflights UTF8/namespace/structure/typed geometry/style/local references
before normalization. It parses one bounded DTD-disabled roxmltree document and
passes it directly to usvg; backend `from_str` enables DTD and is intentionally
not used. Both image resolvers return None. Unsupported text/images/use/filters/
masks/patterns/stylesheets/active/external features reject instead of disappearing
silently. This is a fixed static image profile, not a complete SVG/browser renderer.

Normalized immutable geometry owns no source. Every render validates its own
fitting/geometry/output and publishes only complete independent sRGB-encoded
premultiplied pixels. Numeric renderer compositing does not imply linear-light or
ICC/display management; consumers select separate color preparation. Input/output
limits do not globally bound backend working memory/flattening/time or allocator
failure. These APIs run on UI/worker threads.

Transitive features are recorded explicitly: tiny-skia std/SIMD/PNG and roxmltree
positions/std remain enabled even though direct defaults are disabled. Font/SVGZ/
raster-image backend defaults remain off. Complete independent licence notices
retain exact selected package versions, and lock-only optional entries are listed
separately from selected runtime dependencies.

The complete tutorial delivers imported Rust pixels through bounded source/fit/
size messages, explicit association conversion and the default standalone settings
popup. Independent Chromium and WK SVG image decoding compare authored geometry
and the outlined first-party logo. Exact interior/geometry tests and declared edge
fixture tolerances are distinct from general fidelity, native focus/accessibility,
platform or real-time claims. See [the guide](../svg-images.md) for exact profile,
limits, reproducible commands and remaining boundaries.
