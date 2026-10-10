# 0175: Prepared paragraph source carets and selection geometry

Status: implemented within the bounded local-font UI/worker profile; focused tests and browser geometry validation passed.

## Decision

Retain the complete original source, options and base direction in immutable
paragraph layouts. Borrow this identity when preparing grapheme-boundary caret
positions and selection cells from the original shaped clusters, directional
runs and line placement. Affinity chooses the preceding logical text's end or
following text's start; soft wraps and bidi boundaries may give different points.
Do not invent editable UTF8 offsets inside extended graphemes.

Pin optional unicode-segmentation 1.13.3 (MIT OR Apache-2.0, no runtime dependencies)
for Unicode 17.0 extended-grapheme boundaries. Its upstream manifest, copyright
and complete licences were reviewed. Line opportunities remain Unicode 15.0 and
bidi remains Unicode 16.0; this is no unified-version or complete conformance claim.
Reject non-monotone, cross-grapheme or missing-initial-boundary backend clusters.

Use cluster pen advances for geometry, ignoring glyph ink/mark offsets. Split
ligature advance equally among its graphemes; no font GDEF caret query is supplied.
Selection boxes use baseline ±half configured line height. CRLF is one grapheme;
hard separators retain their source bytes but have no selection pen width.
Merge selected cells in visual line/X order, allowing four relative machine
epsilons for accumulated edge rounding. No whole-line newline fill is invented.

Preparation allocates at most 8194 affinity positions and 4096 selection cells
within the existing 4096-byte input limit, outside audio callbacks. Caret lookup,
hit tests and caller-buffer selections allocate nothing after preparation.
Hit tests select the nearest baseline (a midpoint chooses the later line), clamp
outside finite coordinates, then choose the nearest pen position. X ties prefer
the requested affinity and earlier source bytes. Invisible/bidi points can share
coordinates, so source/point round trips are not guaranteed. Nonfinite hit
coordinates return None. Selection validates endpoints and total destination
capacity before writing; errors preserve the complete caller buffer.

## Evidence and limits

Portable contracts cover Arabic combining graphemes and RTL edges, bidi/wrap
affinities, CRLF/empty lines, extreme/nonfinite coordinates, selection capacity
rollback, maximum source boundaries and allocation-free prepared operations.
A tiny first-party rectangular GSUB font actually shapes ffi into one glyph;
its known advance supplies analytic interpolation and source-selection expectations.
No external font outlines or proprietary consumer assets are copied. A browser SVG
fixture shows two disjoint boxes for a logical bidi selection, two affinity
carets for byte 3, and four equally spaced ffi carets with its middle cell selected.
It contains 73 outline paths, no text nodes and finite geometry inside 720×320.
This is local browser SVG evidence, not interactive editing or native input.

This is editing geometry, not an interactive text editor, input method,
clipboard, undo manager, native caret/selection, accessibility or screenreader
proof. No GDEF measurement, automatic font/script fallback, shaping-backend global
memory/time bound, audio deadline or complete Unicode/script support is claimed.

Sources: [caret map](../../crates/ravet-surface/src/caret.rs),
[paragraph identity](../../crates/ravet-surface/src/paragraph.rs),
[tests](../../crates/ravet-surface/tests/caret.rs),
[fixture provenance](../../crates/ravet-surface/tests/fixtures/caret/README.md),
[guide](../surface-drawing.md).
