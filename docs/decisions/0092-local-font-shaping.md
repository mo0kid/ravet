# 0092: Optional local-font horizontal run shaping

Status: implemented scoped UI/worker profile; portable fixture/independent CLI evidence.

## Decision and scope

Use optional pinned HarfRust 0.14.0, exposed through `ravet-surface/text` and
umbrella `ravet/text`; default builds omit it. Retain owned local font bytes and
backend default variation instance. Shape one complete independent horizontal
run with caller direction, optional guessed/explicit script/language and global
feature overrides. Return immutable font-specific glyph IDs, visual-order UTF8
clusters, scaled positions/advances and total pen movement, with upward Y.
Missing glyphs remain explicit; caller owns matching renderer/font/text identity.

Input bounds: 16 MiB font, 4096 UTF8 text bytes, size (0,1024], 32 printable-tag
features and 1..63 ASCII language tags. Reject controls/newlines/noncharacters.
Output caps at 16384 glyphs after backend work; neither backend transient memory
nor time is globally bounded by that cap. Font parsing/cache/shaping allocate and
belong on UI/worker threads. No automatic font discovery/files/network/global
registration/fallback/bidi paragraph resolution/wrapping/partial-run context,
raster/native/GPU/accessibility or deadline integration.

The earlier RustyBuzz candidate is archived; choose the current HarfRust backend
and record its locked dependency notices. Public Noto font fixture retains its
unmodified OFL and exact upstream commit/hash; proprietary assets stay private.

## Evidence

Three groups exercise contextual Arabic/marks/visual byte clusters/scaling,
kerning override, empty/missing coverage/repeat identity and invalid/bounded
font/face/options. Eight runs compare exact glyph IDs/clusters and numeric
positions/advances to installed HarfBuzz 14.4.0, including kerning disable.
This does not establish all fonts/scripts, paragraph or rendering correctness.
