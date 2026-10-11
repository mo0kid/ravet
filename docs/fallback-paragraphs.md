# Explicit font-stack paragraphs

Enable `ravet-surface/text` to prepare a wrapped mixed-direction paragraph with
`FallbackParagraphLayout`. Supply an immutable `TextFontStack` containing 1..16
already selected faces. The layout copies the complete original UTF-8 source,
script spans, language and features, and borrows those exact faces, including
selected variation instances. The stack object may retire first; the faces may
not. No implicit family/style substitution, font discovery or script guessing is
performed by this layout engine.

`FallbackParagraphTextOptions` supplies finite size(0,1024], an explicit LTR/RTL
base direction, existing optional language and 32-feature profile, and at most 128
ordered nonempty `ParagraphScriptSpan`s. Spans cover the entire original source
without gaps/overlap, including separators, and end on complete Unicode 17
extended-grapheme boundaries. Tags are four ASCII letters, structurally checked
and must also be accepted by the selected shaping backend's ISO15924-tag parser; there is no independent script registry lookup. Common/Inherited assignments are
caller choices. Empty source requires no spans. The original source caps at 4096
UTF-8 bytes; existing paragraph controls admit LF/CRLF/CR/U+2028/U+2029 while tabs,
other forbidden controls and noncharacters reject.

## Separate bidi and shaping boundaries

Full-original `BidiInfo` resolves the explicitly based Unicode 16 paragraph levels.
Each measured line receives original-scalar UAX #9 L1 adjustment. RAVET then
applies an explicit atomic-grapheme rendering policy: the final non-X9-removed
scalar's post-L1 level represents the whole grapheme, while an all-X9 grapheme
uses its own post-L1 level. L2 reorders these complete units. This prevents
trailing BN joiners/tags and Prepend characters from splitting a grapheme.
It is RAVET's rendering refinement, not a claim of scalar-exact UAX #9 L2
conformance. Original bidi classes/levels/source and shaping context stay intact.
U+2028 ends a line without inventing a bidi paragraph reset; U+2029 is a paragraph
separator. Embeddings/isolates survive applicable soft wraps and U+2028. The
caller script pieces and selected font units traverse visual run order; RTL
reverses units while retaining logical characters and byte ranges.

Shaping context is the complete original content between admitted hard
separators. It excludes separator bytes, and supplies the existing backend's
nearest five surrounding codepoints for each item. Soft wrap, font, script and
bidi item boundaries do not become artificial BOT/EOT boundaries. Each run
exposes `context_range()` and original `range()`, plus absolute original-byte
`source_cluster(glyph_index)`. A default-ignorable source unit may have no glyph;
a glyph cluster is neither a caret nor an editing selection boundary. Existing
single-font caret/edit caches do not accept this distinct layout automatically.

Each complete grapheme selects the first supplied face whose actual contextual
shape contains no glyph zero. Adjacent same-face units in a homogeneous
script/direction merge and shape together, preserving same-face ligatures and
kerning. Unresolved units use primary replacements and explicit sorted source
missing ranges. Unexpected final glyph zero for previously covered source rejects
the whole layout. Glyph-zero absence does not prove emoji/variation-selector
presentation conformance; cross-font ligatures are not promised.

## Wrapping, work and publication

`ParagraphOptions` supplies finite positive width/line height and physical
left/center/right alignment. Unicode 15 line opportunities are filtered through
Unicode 17 grapheme boundaries. Greedy fitting stops at the first overflowing
candidate, retains whitespace and its advance, and preserves an oversized first
unbreakable unit with `overflows() == true`. Width, alignment and overflow include
retained trailing whitespace advance, even when visible ink would fit. There is no forced split, clipping,
dictionary break, discretionary hyphen insertion or justification. Empty input
and adjacent/trailing hard separators retain empty lines and original separator
ranges. Lines expose logical source range, separator range, width, aligned
physical X/downward-Y baseline and visual runs. Runs expose script/level/direction,
actual face and stack index, shape and line-local X.

One construction shares at most 8192 coverage probes,16384 total backend shape
calls and 1024 measured candidate lines, including validations, discarded
candidates and empty lines. A bounded per-grapheme/direction choice cache avoids
repeated coverage probes. Successful `work()` reports exact admitted counts;
these counters do not measure CPU time. Candidate search can revisit a growing
prefix, so admitted work is width-dependent and not monotonic: wider geometry
can exhaust the whole-call budget even when narrower geometry succeeds. A
resize failure therefore preserves the consumer's prior image and publishes a
diagnostic; no automatic complexity-reducing fallback is applied. Each candidate and complete retained
output caps at 512 runs/16384 glyphs; output caps at 256 lines. Source/style/finite
arithmetic and caps validate before a complete immutable output is published.
Errors publish no partial layout. Font parser/shaper/bidi/line-break temporary
memory, allocation failure and latency are outside these counters; prepare on
UI/worker threads, never the audio callback.

`SvgDrawing::fallback_paragraph` consumes the exact faces and line/run origins,
with baseline/color, rolling back both document bytes and path-command counts
on any failure. With `raster`, `RasterText::from_fallback_paragraph` preflights
one owned geometry object under the aggregate 65536-command/device bounds before
publication. It can outlive fonts/layout. Existing `draw_text` then preflights
whole pixel work before touching the canvas. Selected TrueType glyf instances
remain exact; unsupported CFF/bitmap/color/hinting/ICC/display profiles are
separate. Existing renderer geometry limits can reject a valid layout.

## Ordinary standalone workspace

The `paragraph_workspace` example shares one pure Rust model between export and
the macOS window. It loads exact available Arial Regular through the native
catalogue, then caller-owned explicit local alternatives and the public Noto
Arabic fixture. An unavailable/unsupported exact native selection fails without
silently choosing another face. System bytes remain memory-only. Authored export
uses the existing tiny rectangular fixtures instead, for reproducible source,
geometry and pixel evidence.

Labelled native HTML controls select nine samples, width 80..640, size 14/20/28,
base direction, alignment and SVG/pixel mode. Samples cover mixed scripts,
Arabic marks, isolates, accents/ligatures, missing characters, empty input,
hard separators, an unbreakable word and a deliberately oversized document.
Rust prepares the entire source/layout/geometry/image/JSON transition before
committing state. Commands have exact known single-key plus current u32 revision
and at most 128 bytes; stale/malformed/out-of-range commands are ignored. During
pending frontend acknowledgment controls are disabled and edits are refused.
The separate Refresh current view button remains usable for explicit recovery
from ignored stale commands or rejected image publication. The exact bounded
`{"refresh":1}` request reads the actual model revision without a stale client
revision and increments a distinct u32 delivery counter. It republishes the
current source/options/image without modifying the state revision. Equal-revision
updates are admitted only while an explicit refresh is pending and their delivery
counter is newer; all image, geometry and pixel-hash checks still apply. Invalid
publication releases busy ownership, retains the prior image and shows a visible
recovery diagnostic. Stale/replayed deliveries cannot overwrite it. Delivery
exhaustion refuses refresh visibly; reopening creates a new bounded model. No
response timeout or automatic retry is assumed.
Noops preserve the revision; exhaustion refuses a new transition. Layout/render
or snapshot-envelope capacity failure publishes a diagnostic revision while retaining the previously accepted
source/options/image. Programmatic native resize uses measured content width
minus 40, floored/clamped 80..640; widths below 120 or invalid dimensions preserve
the prior image. Changes only reflow when measured dimensions change.

The consumer caps images at 704×384, encoded output 45000 bytes/base64 payload 60000,
and full snapshot 65536 bytes. PNG is only bounded image transport, using the
existing optional `image` feature. Opaque-white CPU preparation makes the PNG
straight-byte conversion exact; browser canvas pixels must match the Rust pixel
hash before publication. SVG transport supplies explicit intrinsic width/height
in addition to the generated viewBox; browser font matching is never used for
the rendered image. Readable original source supplies the image label.

The standard title-bar Audio/MIDI settings popup remains enabled. This visual
model advertises no devices and opens none. Owned native fixtures validate
measured resize, focused control retention, settings composition and child-before-
parent cleanup. The normal snapshot path observes actual image publication;
the settings wrapper invokes updates without forwarding their Promise. The
separate authored-state fixture captures the consumer's async update before that
wrapper solely to await its full image publication. Browser data is prerecorded
Rust output, not a dynamic Rust runtime or an independent layout oracle.

```sh
cargo test -p ravet-surface --features text,raster --test fallback_paragraph --locked
cargo test -p ravet-examples --bin paragraph_workspace --no-default-features \
  --features paragraph-workspace --locked
# Native macOS consumer, default live-ui and settings popup:
cargo run -p ravet-examples --bin paragraph_workspace --features paragraph-workspace --locked
# Portable preparation without a window:
cargo run -p ravet-examples --bin paragraph_workspace --no-default-features \
  --features paragraph-workspace --locked -- --export > target/paragraph-models.json
python3 scripts/check-fallback-paragraphs.py
```

## Independent and actual evidence

Ordinary Cargo tests need no native validation library. The optional script needs
`pkg-config` and an independently installed HarfBuzz C library for a validation-
only helper. Twenty-two profiles/95 runs/121 glyphs match exact glyph IDs/original
clusters and scaled positions/advances to 1e-9 using full hard-domain item context,
matching direction/script/variation and BOT/EOT/remove-ignorable flags. Four fixed
analytic wrap vectors, five original visual-order vectors, ten atomic-grapheme
visual vectors and four further wrap vectors independently check
segmentation/order, including U+2028 versus U+2029 and a selected heavy instance;
passing HarfBuzz on Rust-selected items alone would not prove segmentation.

Fourteen integration groups plus three shared-library units cover contextual Arabic,
whole graphemes, strict spans/source/finite limits, shared candidate/probe/shape
and output caps, original clusters, face lifetimes and late SVG/CPU preservation.
The pure consumer has seven test groups. Its own native dynamic Rust window passes
41 groups, and the authored-state HTML transport passes 65 groups in Chromium and
WKWebView. Chromium trusted click/Enter/Space activate the width button and retain
focus. Initial browser `document.hasFocus()` was false; trusted interactions made
it true. Those are specific fixture receipts, not all-Unicode conformance,
vertical/rich-text layout, text editing/IME, assistive-technology certification,
platform/device/deadline support or browser automatic-font equivalence. The
historical WebKit NFC fallback discrepancy remains qualified in the local font
guide.

The backend tables are pinned independently: `unicode-bidi` 0.3.18 Unicode 16,
`unicode-linebreak` 0.1.5 Unicode 15 and `unicode-segmentation` 1.13.3 Unicode 17.
Primary reference boundaries are [Unicode bidi](https://www.unicode.org/reports/tr9/),
[Unicode 15 line breaking](https://www.unicode.org/reports/tr14/tr14-49.html),
[Unicode 17 graphemes](https://www.unicode.org/reports/tr29/tr29-47.html) and
[HarfBuzz item context](https://harfbuzz.github.io/adding-text-to-the-buffer.html),
[flags](https://harfbuzz.github.io/harfbuzz-hb-buffer.html) and
[clusters](https://harfbuzz.github.io/working-with-harfbuzz-clusters.html).
Latest bidi documentation is explanatory reference; it does not upgrade the
pinned Unicode 16 implementation. Fixture/oracle details are in the
[reproducibility README](../crates/ravet-surface/tests/fixtures/fallback-paragraph/README.md).

Delivery exhaustion is terminal. Native model command admission and programmatic
transitions refuse edits before mutation at delivery u32::MAX. If the MAX delivery
itself was rejected, a pending explicit refresh receives one terminal envelope
acknowledgment (`refreshExhausted: true`), without modifying state revision or
delivery. It is admitted only for a pending refresh at MAX after all envelope,
image, geometry and pixel validation; it retains the previous verified image,
releases busy ownership, disables further commands and shows reopen guidance.
Unsolicited/replayed terminal acknowledgments have no effect. Own-window fixture
counter injection reaches this boundary without iterating four billion requests.
