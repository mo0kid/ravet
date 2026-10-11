# ADR 0259: Explicit selected-instance PostScript outlines

Status: proposed implementation, awaiting review/adoption and workspace gates.

A CFF face could previously shape but could not supply the existing SVG/CPU text
renderers. Add default-disabled `postscript-outlines` using pinned skrifa 0.48.0,
defaults off/std on, the same read-fonts 0.45.0 instance and caller-selected
normalized coordinates. Preserve glyf precedence and reject alternate-table
ambiguity; Type1, color layers and bitmap rendering remain outside this scope.

Metadata admission owns finite numeric spelling, DICT role/arity/duplicates,
4096 bytes/64 decimal characters/48 effective stack operands, 256 font dicts,
complete FDSelect and private/subroutine bounds. The current CFF2 operator
profile excludes legacy FD FontMatrix/maxstack. CFF2 source matrix normalization
uses equal positive scales, exact zero off-diagonal/translation and absolute
`abs(scale*UPEM-1)<=1e-12`; this tolerance never applies to output geometry or
selected coordinates. Backend parsing/memory/time remain distinct from own
output bounds and no audio callback deadline is claimed.

Revision 2 (W246 R2) treats every DICT as untrusted input that must decode the
same way twice. The source decoder and the pinned read-fonts tokenizer must agree
on the operators, the operand count per operator and the integer/real kind of
each operand before either result is used. Because read-fonts 0.45.0 stops
reading a real number once its exponent exceeds 1000, every exponent above 1000
refuses. Leading-zero exponents, and a CFF2 FDSelect over a single font
dictionary, remain conservative refusals. All index, slice and offset arithmetic
in RAVET's admission path is checked, so RAVET's own code refuses malformed bytes
with `TextError` values rather than panicking. Complete absence of panics also
depends on the pinned read-fonts/skrifa backend. The fuzz evidence is panic-only,
over two first-party seed fonts, with no time or memory budget. Per-glyph outline
extraction still repeats the whole metadata admission; caching it remains a
possible later optimisation. Known limits: that per-glyph re-admission cost and
the backend's CFF2 blend cost at non-default instances are unmeasured, and the
leading-zero exponent refusal may reject some valid fonts.

Publish the backend's actual integer font-unit policy. Signed flooring precedes
normalized fixed-matrix rounded products, retaining explicit degenerate cubics.
The source tiny-negative/0.75-shear case gives (-2,-1), differing from applying
source affine geometry before flooring. Independent FreeType matches this path;
HarfBuzz ignores the authored CFF1 shear and HB/CoreText retain tiny/CFF2 fractions.
At authored 650, CoreText advance 855.99609375 differs from rounded 856. These are
measured differences, not unsupported legal-coordinate refusals or proof of
universal engine agreement.

The macOS `postscript-fonts=[system-fonts]` loader admits only native format 1
OpenType PostScript, unambiguous CFF/CFF2 and checked OTTO reconstruction with
existing identity/unavailable/downloadable/PreventAutoActivation restrictions.
Authored CFF1 classifies as native format 4 and is honestly refused. Authored
process-registered CFF2 classifies as 1 and loads all four selected coordinate
profiles. Fixture registration is reversible/process scoped and not a public
activation API. Eight actual installed format 1 CFF1 styles additionally supply
normal standalone vector/pixel proof; source bytes remain memory only.

Consumers preserve font/render on whole preparation failure and use the standard
Audio/MIDI popup. Browser DOM/trusted activation evidence is separate from native
visual focus, system AX, screenreader, platform and real-time support claims.

Primary semantics: [CFF2](https://learn.microsoft.com/en-us/typography/opentype/spec/cff2),
[Adobe CFF](https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf),
the published skrifa/read-fonts sources and public CoreText headers. See the
[guide](../postscript-outlines.md) for API/profile limits and qualification scope.
