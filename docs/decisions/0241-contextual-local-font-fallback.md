# 0241 — Caller-owned contextual local font fallback

Status: accepted within the explicit bounded profile described below.

Choose fonts by shaping complete extended graphemes against an ordered immutable
caller-owned stack. Cmap-only coverage would incorrectly reject decomposition or
marks that the shaper can resolve. Keep full original source context for probes
and merged final font runs, with BOT/EOT only at source edges and default
ignorables removed after participating in shaping. Preserve selected face
references in the output so renderers cannot accidentally use unrelated glyph IDs.

Require explicit script and direction; font fallback does not replace consumer
script/bidi/paragraph segmentation. Unicode 17 extended graphemes use the existing
locked segmentation dependency, while the text backend retains its own Unicode
behavior. No new dependency, registration, file/network lookup or implicit system
fallback. RTL reverses visual font units and retains original source bytes.
Unresolved units use the primary replacement and remain explicit. A final missing
glyph outside unresolved source causes whole failure rather than false coverage.

The copied source/style and borrowed face ownership are immutable. Source/probe/
run/glyph bounds are 4096 bytes/8192/256/16384, stack 1..16 faces; backend transient
memory/time is not globally bounded. SVG failures roll back the existing document;
CPU geometry prepares completely before publication. Default-instance TrueType
outline scope remains explicit. System discovery, style/variable axes, mixed bidi,
paragraph fallback and color font rendering remain separate work.

[HarfBuzz buffer context](https://harfbuzz.github.io/adding-text-to-the-buffer.html)
requires homogeneous font/script/direction spans with original context; the
[scope documentation](https://harfbuzz.github.io/what-harfbuzz-doesnt-do.html)
leaves font-run and bidi/line decisions to consumers. The declared segmentation
profile is [Unicode 17 UAX29](https://www.unicode.org/reports/tr29/tr29-47.html).
The [buffer flags reference](https://harfbuzz.github.io/harfbuzz-hb-buffer.html)
defines the beginning/end and remove-default-ignorable behavior. Backend context
storage retains the nearest five codepoints on each side.

Acceptance uses first-party deterministic complement/mark/ligature fixtures,
existing unmodified OFL Arabic data, actual Rust SVG/CPU rendering and an ordinary
standalone consumer with the default settings popup. The independent installed
HarfBuzz C oracle matches selected-face/context glyphs and geometry. Actual
browser font-stack and SVG/pixel oracles retain the WebKit NFC selection mismatch;
browser font policy is not treated as an implementation contract. Native/window
and trusted-browser evidence remain distinct from system AX/screen-reader or
physical device validation. See [the guide](../font-fallback.md) for limits,
commands and the exact scope of observations.
