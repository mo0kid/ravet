# 0093: Typed SVG paths and local TrueType glyph outlines

Status: implemented scoped portable vector profile; browser geometry/static-render evidence.

## Decision and scope

Use typed move/line/quadratic/cubic/close commands, validating finite geometry,
subpath topology and 4096 command bounds before atomic SVG append. Local glyph
outlines reuse the already-locked read-fonts 0.45.0 backend through optional
`text`, extracting default-instance TrueType glyf paths. Add no font URL or
system fallback. Font-unit glyph positions scale into caller baseline geometry
with SVG's downward Y. Limit each outline to 4096 finite commands after loading.

Whole-run shaped drawing rolls back bytes/count on any later glyph/capacity
failure. Spaces/empty outlines draw nothing; missing glyph zero retains its
replacement outline. CFF/Type1/color/bitmap/variable-instance rendering is outside
scope. Backend parsing/loading may allocate before output limits; no global
memory/time/deadline claim. Accessible meaning belongs to drawing title/consumer;
outline paths do not supply text selection/input/assistive-tech semantics.

## Evidence

Path topology/curves/nonfinite/capacity preservation and actual glyph/space/ID
extraction, outline-only output and whole-run rollback pass. The example's
640×240 local-font SVG has 25 paths, 18529 bytes and no text/font URLs. Browser
getBBox inspection confirms all ink bounds within viewport. Its preview snapshot
client fails with image and text-only requests; independent librsvg artifact
inspection confirms readable Latin/joined Arabic/marks. No browser screenshot,
native font engine/GPU/raster API or accessibility claim follows. Initial compile
used OutlinePen's parent module; its public outline submodule fixes the import.
The first XML fixture expected 24 paths; actual 26 glyphs less one empty space
correctly yields 25, and the corrected structural check passes.
