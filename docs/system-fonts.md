# Explicit font discovery and styled rendering

`ravet-webview/system-fonts` adds copied font catalogue contracts and an opt-in
macOS `SystemFontCatalog`. Discovery and loading run on the main thread. They do
not create windows, register fonts, download resources, or change device routing.
The ordinary native build has no CoreText service dependency without this feature.

```rust,ignore
use ravet_webview::fonts::SystemFontCatalog;
use ravet_surface::text::{TextFont, TextVariation};
let owner = SystemFontCatalog::discover()?;
let selection = owner.catalog().styles("Arial")[0]; // choose exact metadata explicitly
let loaded = owner.load(selection)?;
let settings = loaded.variations().iter().map(|v| TextVariation {
    tag: v.tag, value: v.value,
}).collect::<Vec<_>>();
let font = TextFont::from_bytes(loaded.bytes(), 0)?.with_variations(&settings)?;
```

Selections are private process-local owner/index tokens belonging to one immutable
snapshot. Exact case-sensitive family/style/PostScript names are metadata, not a
fuzzy matching policy. Duplicate labels remain separate selections. A foreign
selection rejects before native loading. The native owner is neither Send nor Sync;
loaded copied bytes and metadata no longer borrow it. Font licensing and any later
redistribution remain the consumer's responsibility. System bytes are never sent
to the browser or saved by this package.

The catalogue admits at most 4096 descriptors, each name at most 1024 UTF8 bytes,
and 4MiB total names. Family/PostScript names must be nonempty; optional empty style
and display names are preserved. Control characters and malformed names reject the
whole snapshot. Normal portable construction requires finite normalized trait
values within [-1,1]. A native descriptor with valid names but malformed traits
remains discoverable with `traits() == None` and `UnsupportedFont`; its load rejects.
Native unsupported formats or malformed availability also have typed diagnostics.
Unavailable/downloadable resources are retained as metadata and refuse loading.
No missing or unsupported requested style silently selects an alternative face.

Discovery uses a bounded available-font collection with disabled entries excluded
and automatic activation disallowed. CoreText descriptor registration fields may
be absent; snapshot membership is the bounded discovery evidence in that case.
Explicit disabled metadata is always honored. Documented Boolean fields observed
as numeric CoreText 0/1 are admitted only as exact finite 0/1; unknown types and other
values are unavailable/unsupported, never truthy coercions. Loading uses
`kCTFontOptionsPreventAutoActivation` and revalidates exact name/family/style/source
identity. This does not promise general system font activation or availability.
CoreText's internal allocation and elapsed work are not bounded by output limits.

Loading supports renderer-compatible static or variable TrueType glyf resources.
CFF/CFF2, bitmap-only, synthetic matrix/fixed-advance/features and unrepresentable
variations reject. There are at most 256 copied tables, 16MiB reconstructed bytes
and 32 axes. The complete SFNT is checked before publication: sorted unique valid
OpenType tags, required TrueType tables, checked padded offsets, head magic/units,
per-table checksums with head adjustment zero, then the complete SFNT checksum.
DSIG is omitted because this is a new container, not the original signed file.
Tables must be byte-identical to a same-face explicit-default native instance;
selected coordinates are applied once by the existing Rust font backend.

`LoadedSystemFont::axes()` preserves authoritative exact fvar bounds and original
native bounds/current values. CoreText min/max metadata may differ by at most1e-4
user units, matching measured four-decimal truncation. Default, hidden flag, unique
tag and selected coordinates remain strict; coordinates outside source fvar bounds
reject. No clamping occurs. The min/max tolerance never applies to coordinates,
glyph metrics, source ranges, or ordinary portable descriptors. Native f64 selected
values must round to in-range f32 within1/65536 user units. `TextFont` exposes both
requested f32 user coordinates and the actual backend normalized 2.14 coordinates.
The exact font instance is retained through shaping, outlines, SVG, CPU drawing
and contextual caller-owned fallback.

`TextFont::variation_axes()`, `with_variations()` and `variation_coordinates()`
are available under Surface's existing `text` feature, without native discovery.
Only the exact fvar1.0 header, reserved value2, twenty-byte axis records and
checked axes offset are admitted; this is not whole-font validation.
Settings admit at most 32 unique valid known axis tags with finite values inside
source ranges. Omitted axes reset to source defaults on each new immutable instance;
unknown/duplicate/out-of-range settings reject without changing the original.
The font backend may perform internal work before bounded outputs. This profile
keeps explicit script/direction and the existing shaping/outline limits; it adds
no implicit system fallback, mixed-bidi discovery, color glyphs or native hinting.

## Normal standalone consumer

```sh
cargo run -p ravet-examples --features system-fonts,live-ui --bin system_fonts
cargo run -p ravet-examples --features system-fonts,live-ui --bin system_fonts -- --fixture
```

The macOS consumer begins with exact Arial Regular and reports an error if absent,
then keeps the standard Audio/MIDI settings popup. Families and styles are paged
four entries at a time with full counts; all snapshot entries remain reachable.
Native labelled selectors and previous/next shown-style buttons send bounded exact
indices plus the displayed revision. Commands are at most 128 bytes; stale, foreign,
malformed or off-page indices reject. JSON object key order is not assumed.
Presentation page changes do not reload or alter the admitted selected font.

A complete native load, Rust shaping, image preparation and JSON-size admission
precedes each font commit. Refused requests preserve the previous font/image and
publish a typed diagnostic with a new revision; the selected label still names
the admitted font. Rendering/sample/size transitions also publish atomically.
The fixed samples are at most 64 UTF8 bytes, preview 128x48 RGBA8 or 32768 SVG bytes,
and snapshots at most 65536 bytes after escaping/base64. Font bytes never enter
messages. Native loading/shaping is UI work, with no audio deadline guarantee.

## Evidence and qualifications

The first-party two-axis rectangle fixture, analytic expected advances/outlines,
independent HarfBuzz and fontTools profiles prove selected instance consumption.
The actual CoreText snapshot on the measured machine returned 1139 descriptors and
308 families. Eurostile supplied slant=-97.55555555555556 and remained explicit
unsupported metadata. Helvetica/Arial styles loaded. Skia native axis minima and
maxima truncate exact fvar values; four Light/Condensed named styles have current
coordinates outside exact source bounds and remain unsupported. Six other Skia
styles load without clamping.

The independent native 80-glyph comparison uses five fixed Latin glyphs across 16
loaded styles. Glyph IDs and advances agree exactly. Upright static and default
Skia tight-path bounds agree exactly; varied/oblique bounds differ by at most
0.484375 font units in this receipt. This measured comparison does not promise
native CoreText rasterization, hinting, shaping or pixel equivalence for arbitrary
fonts/text. An original 7-unit Helvetica 'g' diagnostic was an oracle error: a final
empty move/close contour was wrongly included in ink bounds. Its negative receipt
is preserved separately; product outline code was unchanged.

Owned native consumer tests cover actual Rust/native selected styles, SVG/CPU
images, refused style preservation, revisions, resize/focus, settings and teardown.
Browser tests use first-party Rust-produced images only; their offline host shim
is separate from the real native Rust producer. Native browser selectors received
trusted keys without changing default options in the Chromium tool; no selector
keyboard-change proof is claimed. Trusted native style-button click/Enter/Space
activation did consume prepared Rust images with document focus. There is no
system AX, screen-reader, font redistribution or other-platform native certification.
