# 0172: Explicit bounded local-font paragraph layout

Status: implemented within the single-font UI/worker profile; focused code and browser validation passed.

## Decision

Compose local-font shaping with pinned Unicode line opportunities, paragraph bidi
context and explicit width/baseline spacing/physical alignment. Resolve bidi over
the complete input before shaping candidates, rather than losing embeddings or
isolates by resolving each wrapped line independently. Keep original content and
hard-separator UTF8 byte ranges; glyph clusters map back through their logical run.

Use unicode-linebreak 0.1.5 (Apache-2.0, no runtime dependencies) as an optional
part of the existing default-disabled text feature. It uses Unicode 15.0 data,
independent of unicode-bidi's Unicode 16.0 data, and tailors complex-context SA
characters to alphabetic. No dictionary-breaking or Unicode-version-unification
claim is made. Both the downloaded manifest and complete upstream licence were
reviewed; their terms remain separate from RAVET's first-party licence.

Accept at most 4096 input bytes and 256 lines/512 visual runs/16384 output glyphs.
Greedy wrapping stops at the first overflowing opportunity. An oversized first
unbreakable unit overflows unchanged instead of splitting a cluster or inventing
hyphenation. Whitespace advance is retained. LF/CRLF/CR and Unicode separators
produce hard lines, with empty input/trailing separators represented explicitly.
Other controls/noncharacters reject. Font/style identity, coverage and script
segmentation remain consumer-owned. Line spacing is pen geometry, not ink bounds.

Render complete paragraphs through the existing local TrueType SVG outline
mechanism. Reuse the same private glyph renderer for independent shaped runs;
retain whole-run rollback and add whole-paragraph rollback after late failures.
No partial layout or drawing is published on failure.

## Evidence and limits

Six focused groups exercise numeric/source/embedding/capacity/rollback contracts.
The public synthetic local-font example produces two five-line paragraphs with
opposite explicit bases. The collaborative browser reports 192 outline paths,
no text nodes and ink within its 720×420 source viewport; visual inspection confirms
readable Latin and joined Arabic/marks. This is SVG browser rendering only.

The API allocates and may repeatedly run backend shaping; output bounds do not
bound all transient work. No audio callback/deadline, implicit font discovery or
fallback, dictionary hyphenation, discretionary hyphen rendering, justification,
rich text/caret/native/GPU/assistive-technology or complete Unicode conformance
is established. Reusable font fallback and broader rendering remain separate.

Sources: [layout](../../crates/ravet-surface/src/paragraph.rs),
[renderer](../../crates/ravet-surface/src/drawing.rs),
[tests](../../crates/ravet-surface/tests/paragraph.rs),
[example](../../crates/ravet-surface/examples/paragraph-outlines.rs),
[guide](../surface-drawing.md).
