# Portable layout, SVG drawing and animation

`ravet-surface` supplies independent UI mechanisms. Its default build has no
native window/GPU or font dependencies; optional `text` adds local fonts below. These APIs are provisional and do not establish a native
widget toolkit or assistive-technology conformance.

`layout::Rect` validates finite nonnegative extents/edges, half-open containment,
uniform padding and intersection. `layout_axis` lays out at most 256 children in
an explicit row/column, stretching the other axis. Each item supplies minimum,
preferred, maximum and growth weight. Shrinking distributes available capacity
above minima; growth redistributes weighted space after maximum caps. Unclaimed
space remains at the end. Fixed gaps, explicit nesting and caller-owned output
make resizing deterministic. Invalid constraints, overflow, insufficient space
or capacity reject before destination mutation. Layout does not wrap, round,
clip or infer pixel scale, and performs no heap operations.

`animation::Tween` samples a finite scalar with an explicit u64 clock, duration
and linear/smooth-step/cubic easing. Units belong to the consumer; before-start
and exact completion return exact endpoints. Retargeting starts at the current
sample without a jump. Zero duration snaps at start for a consumer's reduced-motion
policy. Elapsed subtraction avoids timestamp-addition overflow. Sampling may
rewind explicitly; there is no clock query, thread, scheduler or display callback.

`drawing::SvgDrawing` serializes rectangles, rounded polylines and escaped text
into a local SVG document. It embeds no script, arbitrary markup, URL or external
resource. Command/document limits are 4096/1 MiB; rejected commands preserve the
document. `Theme` supplies replaceable light/dark semantic RGBA palettes. The
selected Amber dark default uses warm charcoal/plum surfaces and amber
emphasis, with a warm paper light companion. The default web palette uses the
same semantic colors; see [web controls](web-controls.md). `Font`
explicitly selects family, size and weight; the actual SVG renderer owns installed
font fallback, shaping and measurement. The escaped title labels role=img; shapes
are visual and supply no interaction semantics. Use existing native DOM controls
for interactive keyboard/focus/label behavior. Text/title/control validation and
capacity enforcement operate before append; allocations belong on UI threads.

Generate standalone synthetic displays at different widths/times/themes:

```sh
cargo run -p ravet-surface --example drawing --locked -- 640 500 > target/dark.svg
cargo run -p ravet-surface --example drawing --locked -- 360 1000 light > target/light.svg
```

Time is milliseconds in this example; it samples an explicit one-second tween.
Both documents were inspected in the local collaborative browser: valid SVG,
expected theme/layout and text bounds inside the viewport. This is browser SVG
rendering evidence, not GPU acceleration, embedded plug-in drawing integration,
font/asset loading, video, interactive animation scheduling or accessibility audit.

## Optional local-font run shaping

Enable `ravet-surface/text` (or `ravet/text`) for `surface::text`. `TextFont`
copies caller-supplied local font bytes, at most 16 MiB, selecting a backend face
index and its default variation instance. No system font discovery, global font
registration, file/network loading or fallback is implicit. Font parsing/cache
work belongs on UI/worker threads and is not complete corruption validation.

`shape` accepts one independent complete horizontal run, at most 4096 UTF8 bytes,
with finite size (0,1024], explicit left/right direction, optional script/language
and at most 32 global printable-tag feature overrides. Default font features
apply unless overridden. Script may be guessed; language tags are restricted to
1..63 ASCII alphanumeric/hyphen bytes, without full BCP47 validation. Controls,
newlines and Unicode noncharacters reject; spaces, combining marks and joiners
are retained. The caller segments scripts/bidi text before shaping; partial-run
surrounding context, paragraph bidi resolution, wrapping and fallback are not
supplied by this independent-run API.

Returned glyphs are in visual order with font-specific IDs, UTF8 byte clusters,
scaled positions/advances and upward Y. Glyph zero reports missing coverage.
The run retains total pen advance, rather than ink bounds; caller keeps matching
font/text identity and applies its rendering/baseline transform. Returned glyphs
cap at 16384 after shaping. Backend transient memory/time is not globally bounded
by that output cap. Font loading/shaping allocate; cache runs before callbacks.
There is no raster/native/GPU/accessibility integration or deadline claim.

The default build excludes the shaper dependency. Optional HarfRust 0.14.0 (MIT),
read-fonts/font-types and their locked transitive notices remain separate from
RAVET's licence; see [licensing](../LICENSING.md). A public unmodified Noto Sans
Arabic fixture retains [OFL/provenance](../crates/ravet-surface/tests/fixtures/text/README.md).
Portable groups exercise contextual Arabic/marks/clusters/scaling, actual kerning
override, repeat/empty/missing coverage and invalid/capacity/face rejection.
Eight actual runs match installed HarfBuzz 14.4.0 numeric glyphs, UTF8 clusters,
positions and advances, including explicit kerning disable.

```sh
CARGO_BUILD_JOBS=2 cargo test -p ravet-surface --features text --locked
CARGO_BUILD_JOBS=2 cargo run -p ravet-surface --features text \
  --example shape-text --locked -- /path/to/local.ttf 'سلام' rtl
# Optional independent installed CLI (not needed by workspace tests):
python3 scripts/check-text-shaping.py --hb-shape /path/to/hb-shape
```

This is the tested local-font/single-run profile, not every script/font/paragraph
or render backend. See [decision 0092](decisions/0092-local-font-shaping.md).

## Typed paths and local glyph outlines

`PathCommand` supports move/line/quadratic/cubic/close geometry. `SvgDrawing::path`
accepts 1..4096 finite typed commands: start with MoveTo, draw/close only in an
open subpath, and start another subpath after Close. SVG's normal implicit fill
closure applies. No arbitrary markup/URL/transform string is accepted. Existing
4096 drawing-command/1 MiB document limits and atomic rejection remain.

With `text` enabled, `TextFont::outline_glyph` extracts default-instance TrueType
`glyf` outlines into at most 4096 finite font-unit path commands. Empty outlines
(e.g. spaces) yield nothing; invalid IDs reject. CFF/Type1/color/bitmap rendering
is unsupported. Outline loading can allocate before output limits, so it belongs
on UI/worker threads. `SvgDrawing::shaped_text` shapes a complete independent run
and renders its local outlines at a caller-selected baseline, converting upward
font Y to downward SVG Y. It emits paths without text nodes/font URLs. Glyph zero
draws that font's replacement outline. Whole-run failure restores document bytes
and command count, even if earlier glyph paths succeeded.

Consumers own headroom/layout, font permissions and meaningful accessible titles;
outline paths themselves do not supply text semantics, selection or text input.
There is no hinted/native/GPU/bitmap/variable-instance or generic raster backend.
The example uses public licensed local font bytes and needs no system fallback:

```sh
CARGO_BUILD_JOBS=2 cargo run -p ravet-surface --features text \
  --example font-outlines --locked -- /path/to/local.ttf > target/font-outlines.svg
```

Tests cover contour/curve serialization, invalid topology/nonfinite/capacity
preservation, actual local glyph/space/invalid-ID outlines, outline-only output
and whole-run capacity rollback. A 640×240 fixture contains 25 paths/18529 bytes;
the local collaborative browser reports all ink bounds inside the viewport and
no text nodes. Preview snapshots fail on its client, so pixel inspection used an
independent librsvg export: readable Latin, joined Arabic and combining marks.
That export tool is validation evidence, not a RAVET raster API. See
[decision 0093](decisions/0093-svg-local-glyph-outlines.md).

## Single-line bidirectional run ordering

Optional `bidi::BidiLine` resolves one complete line before shaping with the
pinned unicode-bidi 0.3.18 Unicode 16.0 data. Supply an explicit base direction,
or use first-strong detection (LTR for no strong text). It retains borrowed text,
line-adjusted embedding levels per UTF8 byte and logical byte ranges in visual
run order. Feed each original run slice to the shaper with its resolved direction;
reversing characters first would break combining/contextual shaping. The shaper
handles mirroring/marks. Runs are directional boundaries, rather than font/script
boundaries; caller still segments scripts, chooses fallback/fonts/styles, maps
clusters and places/aligns complete lines.

Input is at most 4096 UTF8 bytes, output at most 512 directional runs after
backend work. Controls, Unicode line/paragraph separators and noncharacters
reject; directional formatting/isolate characters remain in logical source
ranges. `TextFont::shape` now also explicitly rejects Unicode separators, matching
its single-line contract. Empty lines retain explicit direction (default LTR).
No wrapping, paragraph/multiline layout, script/font fallback, caret/text input,
native/GPU/accessibility or global backend memory/time/deadline claim follows.

Thirty-two unchanged Unicode 16.0 reference vectors pass resolved base levels,
character levels and visual order. Additional cases check isolates/combining/
byte-boundary coverage, auto/explicit base, empty lines, exact 512-run capacity
and invalid separators. Reference data retains
[Unicode licence/provenance](../crates/ravet-surface/tests/fixtures/bidi/README.md).
The controlled example combines Arabic/Latin/numbers using local outlines:

```sh
CARGO_BUILD_JOBS=2 cargo run -p ravet-surface --features text \
  --example bidi-outlines --locked -- /path/to/local.ttf > target/bidi-outlines.svg
```

See [decision 0094](decisions/0094-single-line-bidi.md). This is a scoped
single-line fixture profile, rather than complete Unicode/UI conformance.
