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

`layout_grid` resolves explicit row/column tracks and checked nonzero area spans
into atomic caller-owned rectangles. Each axis has 1..32 tracks and at most 256
areas are accepted; overlapping areas are explicit. Both axes reuse the same
shrink/growth rules above. The [grid and workspace guide](grid-layout.md) shows
portable geometry and actual measured standalone-window consumption.

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
index and its default variation instance; explicit `with_variations` selects an
immutable instance from checked source fvar axes. No system font discovery, global font
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
Shaping itself does not select a presenter. The separate [CPU raster](cpu-raster.md)
feature can render explicitly prepared local-font outlines; native/GPU/accessibility
integration and callback deadlines require separate evidence.

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

Optional [explicit local font fallback](font-fallback.md) supplies an ordered
caller-owned stack, whole-grapheme contextual selection, source/matching-face
ownership and complete SVG/CPU consumption. It does not change the independent
single-font APIs or infer system/script/paragraph policy.

## Typed paths and local glyph outlines

`PathCommand` supports move/line/quadratic/cubic/close geometry. `SvgDrawing::path`
accepts 1..4096 finite typed commands: start with MoveTo, draw/close only in an
open subpath, and start another subpath after Close. SVG's normal implicit fill
closure applies. No arbitrary markup/URL/transform string is accepted. Existing
4096 drawing-command/1 MiB document limits and atomic rejection remain.

With `text` enabled, `TextFont::outline_glyph` extracts selected-instance TrueType
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
These outline APIs do not provide hinted/native/GPU/bitmap
rendering. The independent [CPU raster](cpu-raster.md) feature accepts prepared
paths and local TrueType glyph outlines.
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

## Bounded local-font paragraph layout

With `text` enabled, `paragraph::ParagraphLayout::new` composes the existing
local font shaper with explicit finite positive width, baseline spacing and
physical Left/Center/Right alignment. The input remains at most 4096 UTF8 bytes;
complete output is limited to 256 lines, 512 directional runs and 16384 glyphs.
Preparation allocates and repeatedly shapes candidate lines on UI/worker threads;
output caps do not globally bound backend work or memory. Cache the result before
callbacks; it owns the original source and paragraph options/base direction.
Retain the matching font and shaping style identity.

Line opportunities come from pinned unicode-linebreak 0.1.5 with Unicode 15.0
data; bidi retains the separate Unicode 16.0 backend. Its default line-break
profile treats complex-context (SA) characters as alphabetic, so dictionary
breaking for languages such as Thai is not supplied. The greedy policy accepts
consecutive fitting break opportunities until the first overflowing candidate.
An oversized first unbreakable unit is retained as one overflowing line, flagged
by `overflows()`; no character is forcibly split, clipped, dropped or replaced.
Spaces remain in their source lines and their advances participate in fitting.
There is no hyphen insertion, discretionary soft-hyphen substitution or
justification. Font coverage still appears as missing glyph zero, without fallback.

Resolve bidi embedding/isolate context for the whole input before soft wrapping,
then apply line reordering to each candidate and shape each original directional
slice. Base direction is explicit in `TextOptions`; script/language/features apply
to each run, rather than providing automatic script segmentation. LF, CRLF, CR and
Unicode line/paragraph separators create hard lines. Original separator byte ranges
remain available separately from content ranges; empty input and trailing hard
separators produce empty lines. Other controls/noncharacters reject. This does
not supply interactive editing, rich text or native font registration. The
optional prepared source mapping below consumes this immutable layout.

Each line exposes its original UTF8 content/separator ranges, width, visual runs
and alignment/baseline offset. Run X positions accumulate left-to-right in visual
order; glyph Y remains upward from the existing shaper. `source_cluster(i)` maps
one glyph's local cluster back to the original paragraph byte offset. Spacing
height is `line_count * line_height`, with the first baseline at zero; it is not
measured ascender/descent or ink bounds. Overlong lines start at X zero for every
alignment. Invalid styles, numeric overflow or capacity failure return no partial
layout.

`SvgDrawing::paragraph_text` renders the selected local TrueType outlines and
returns the layout. All paragraph/drawing/path limits apply, and a late outline
or document failure restores the whole existing document and command count.
The paragraph emits no text nodes, scripts, font URLs or implicit resources;
consumers supply accessible meaning through their title/markup.

```sh
cargo run -p ravet-surface --features text --example paragraph-outlines --locked -- \
  crates/ravet-surface/tests/fixtures/text/NotoSansArabic.ttf > target/paragraph.svg
```

Six portable groups cover exact wrapping/whitespace/glyph identity, global byte
clusters, physical alignment, all hard separators/empty lines, nonbreaking spaces,
combining marks/zero-width breaks, preserved embedding levels across soft wraps,
unbreakable overflow, maximum bytes/lines/runs, invalid styles and whole-drawing
rollback. The synthetic 720×420 example renders two five-line paragraphs with
opposite explicit base directions. The collaborative browser reports 192 paths,
no text nodes and finite ink inside the viewport; visual inspection shows joined
Arabic/marks and readable Latin without clipping. This is local SVG browser
rendering, with no native/GPU/bitmap, assistive-technology or other-font/script
conformance claim. See [decision 0172](decisions/0172-bounded-paragraph-layout.md)
and the dependency notices in [licensing](../LICENSING.md).


## Prepared source carets, hit tests and selections

`caret::CaretMap::new(&layout)` derives original UTF8 extended-grapheme boundaries
and pen geometry from the immutable paragraph source, shaped clusters, visual
runs and line baselines. `TextPosition` includes Upstream/Downstream affinity:
the same byte at a soft wrap can resolve to the preceding line's end or following
line's start; a bidi boundary can resolve to two different X positions. Invalid
byte offsets, including positions inside combining graphemes/CRLF, return None.

The optional pinned unicode-segmentation 1.13.3 uses Unicode 17.0 data, separately
from line-break Unicode 15.0 and bidi Unicode 16.0. Unsupported non-monotone,
cross-grapheme or missing-initial-boundary backend clusters reject with Shaping.
Ligature interior positions divide the cluster advance equally among graphemes;
this is an explicit interpolation policy, not GDEF font caret measurement.
Mark offsets/ink bounds do not alter pen carets or selection cells.

Preparation allocates at most 8194 affinity positions and 4096 grapheme cells
within the 4096-byte source cap. Prepared `caret`, `hit_test` and `selection`
operations allocate nothing. Hit tests choose the nearest baseline, with vertical
midpoints choosing the later line. Finite coordinates outside clamp to available
lines/pen positions; nonfinite coordinates return None. X ties prefer the supplied
affinity, then earlier source bytes. Invisible/bidi source positions may coincide,
so source/point/source round trips are not guaranteed.

`selection(range, &mut output)` accepts a half-open logical range at grapheme
boundaries and writes only the returned prefix of visual line/X-ordered boxes.
Each pen box spans baseline ±half the configured line height, rather than font
ascender/descent or glyph ink. Touching selected cells merge with four relative
machine epsilons for floating-point edge rounding; bidi logical selections can
remain visually disjoint. Hard separators have no pen width, so selecting only
one produces no box. Invalid endpoints or insufficient output capacity preserve
every caller slot; unused output slots also remain untouched after success.

Six portable groups check numeric/source/affinity contracts, a real one-glyph
synthetic ffi substitution with analytic equal-grapheme carets, selection rollback
and maximum source boundaries. A 3000-iteration prepared lookup/hit/selection
loop records zero heap allocations. The first-party rectangular fixture retains
[generator/provenance](../crates/ravet-surface/tests/fixtures/caret/README.md).
This is reusable editing geometry; interactive input adapters, IME/clipboard/native
caret, assistive technology, global backend work/deadline and complete Unicode
conformance remain separate. See [decision 0175](decisions/0175-grapheme-caret-selection.md).


```sh
cargo run -p ravet-surface --features text --example paragraph-carets --locked -- \
  crates/ravet-surface/tests/fixtures/text/NotoSansArabic.ttf > target/paragraph-carets.svg
```

The 720×320 browser fixture contains 73 outline paths and no text nodes. It shows
one mixed bidi byte with two visual affinities, a logical selection with two
disjoint boxes, and four equally spaced carets within the actual ffi substitution.
The middle ligature grapheme has one 24-unit pen selection at size 72. Visual
inspection confirms separate highlight boxes/carets and unclipped source geometry;
this remains local SVG evidence, rather than native text interaction.


## Source editing and cache ownership

The optional [bounded text edit model](text-editing.md) owns grapheme-safe source
replacement/deletion, directional selection and undo/redo snapshots on UI/control
threads. Process-local owner/revision tokens reject stale or foreign mutations.
`prepare_layout` captures source and token together; check the returned
`PreparedTextLayout::matches` before treating cached paragraph/caret geometry as
current. Undo restores text and selection with a new revision, so matching old
source bytes does not revive a stale token. This remains source state and geometry;
input adapters, IME, clipboard and native editor/accessibility validation are
separate facilities.

## Optional owned image codecs

The independent default-disabled [image feature](image-assets.md) decodes static
PNG/selected JPEG bytes into owned straight RGBA8 and encodes bounded PNG output.
Consumers explicitly convert/adopt pixels for raster or Metal presentation;
geometry/layout alone adds no codecs, files or color-management policy.

Optional [explicit system fonts and immutable variations](system-fonts.md) keep
main-thread macOS discovery separate from local Surface shaping. Selected instances
retain exact caller coordinates through outlines, SVG, CPU and fallback consumption.

## Exact-face font-stack paragraphs

With `text`, `fallback_paragraph::FallbackParagraphLayout` composes original-source
bidi, grapheme-safe caller script spans, wrapping and explicit font-stack coverage.
`SvgDrawing::fallback_paragraph` and optional `RasterText::from_fallback_paragraph`
consume exact selected faces transactionally. This is distinct from single-font
paragraph caret/edit caches. See [font-stack paragraphs](fallback-paragraphs.md) for
source/context policies, whole preparation bounds, renderer limits and the normal
standalone workspace with measured width reflow.
