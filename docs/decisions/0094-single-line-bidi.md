# 0094: Optional single-line Unicode bidi ordering

Status: implemented scoped run-ordering profile; normative fixture evidence.

## Decision and scope

Use optional pinned unicode-bidi 0.3.18/Unicode 16.0 data under `text` to resolve
one complete line before shaping. Retain borrowed source, line-adjusted byte
levels and logical byte ranges ordered visually. Base direction is explicit or
first-strong/LTR fallback. Each original logical slice goes to the shaper with
resolved direction; no character reversal is applied before contextual shaping.
Font/script segmentation, glyph mirroring/marks and placement remain downstream.

Input <=4096 UTF8 bytes/output <=512 runs after backend work. Controls, Unicode
line/paragraph separators and noncharacters reject, while format/isolate controls
remain in source ranges. Centralize this single-line check in TextFont as well.
No wrapping/multiline paragraphs, fallback/script selection, caret/text input,
native/GPU/accessibility or global memory/time/deadline guarantee. Empty lines
retain explicit base or LTR. Published Unicode data/licence remains separate.

## Evidence

Thirty-two unchanged official Unicode 16.0 BidiCharacterTest vectors compare base,
character embedding levels and visual order. Additional cases cover auto/forced
base, isolate/combining boundaries, byte coverage, empty lines, exact capacity
and invalid separators. The controlled Arabic/Latin/numbers outline example
illustrates both first-strong directions, without claiming arbitrary-script
segmentation or full conformance.

Actual controlled SVG contains 26 paths/23289 bytes without text/font URLs.
Collaborative browser geometry is within 640x180; independent librsvg PNG
inspection passes. This does not establish a native renderer or accessibility.
