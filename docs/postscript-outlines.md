# Explicit OpenType PostScript outlines

`ravet-surface`'s optional `postscript-outlines` feature adds unhinted monochrome
CFF1 Type2 and CFF2 geometry to the existing text, SVG and CPU raster APIs. It
uses the exact caller-owned `TextFont` and its actual normalized variation
coordinates; it never discovers, substitutes or activates a font.

`TextFont::outline_format()` returns `TrueType`, `Cff` or `Cff2` after bounded
source-metadata admission. It is available with `text`, even when rendering is
not enabled. A present glyf table retains the original TrueType path; a failed
glyf face never falls back to CFF. Otherwise precisely one CFF/CFF2 table is
required. With `postscript-outlines` disabled, CFF rendering is UnsupportedFont.

Metadata admission requires one CFF1 font/name/top dictionary, the declared
headers, a CharStrings count matching the selected face, finite source numbers,
role-correct known Top/Font/Private operators, proper arity, no duplicate keys,
complete FDSelect coverage and in-bounds private/subroutine storage. Each DICT
is at most 4096 bytes, a numeric decimal spelling at most 64 ASCII characters,
the metadata operand stack at most 48 and FDArray at most 256 entries. CFF2
private hint blends are admitted at the exact selected variation coordinates.
These are implementation profile bounds, distinct from the format's broader
limits. They do not constitute validation of every glyph or semantic hint
combination. Unknown or misplaced operators and contradictory metadata reject;
the backend's ignored matrices/default font-dictionary fallback cannot silently
admit them.

Font bytes are untrusted input. RAVET's own admission code checks every index,
slice and offset access and refuses malformed data with a `TextError` rather than
panicking. Complete absence of panics also depends on the pinned read-fonts and
skrifa backend, which RAVET does not control or independently guarantee. The
seeded mutation fuzz tests check only for panics, over two first-party seed fonts
(analytic CFF1 and CFF2), with no time or memory budget. Each DICT is decoded twice, by RAVET's own source-number decoder and by the
pinned read-fonts tokenizer. Both must report the same operators, the same operand
count per operator and the same integer/real kind at each position, or the DICT
refuses before either result is used. read-fonts 0.45.0 stops reading a real
number once its exponent exceeds 1000 and then reads the remaining bytes as new
operands and operators. RAVET therefore refuses every real-number exponent above
1000, even spellings that read-fonts would not cut short.

Two further refusals are deliberately stricter than the CFF and CFF2
specifications:

- A real-number exponent with a leading zero, such as `1E0`, `1E05` or `1E-05`,
  refuses.
- A CFF2 FDSelect that maps every glyph to a single font dictionary refuses.
  Omit FDSelect when the FDArray has one entry.

A font that relies on either spelling reports UnsupportedFont.

`outline_glyph` repeats the complete source-profile admission for every glyph it
draws, then rebuilds the backend outline collection. That admission covers up
to 256 Font/Private DICTs of 4096 bytes each, plus FDSelect and the variation
store. SVG and CPU text preparation therefore pay this metadata cost once per
glyph. It is proportional to the font's metadata, not to the glyph. Prepare text
off audio threads, and cache prepared drawings or images instead of repeating
preparation.

Known limits:

- The per-glyph full metadata re-admission cost described above is unmeasured.
- The leading-zero exponent refusal may reject some otherwise valid fonts.
- The backend's CFF2 blend cost at non-default variation instances is unmeasured.

CFF2 follows the current OpenType 1.9.1 Top/Font DICT profile. Legacy FD FontMatrix
and maxstack keys are unsupported. Its Top FontMatrix has equal positive scales,
exact zero off-diagonal/translation entries, and
`abs(scale * head.unitsPerEm - 1) <= 1e-12`. This tolerance only admits finite
source decimal spellings of the UPEM reciprocal; it never changes selected
coordinates, glyph geometry or advances. Missing matrix is admitted only at
UPEM 1000. CFF1 permits explicit nondegenerate backend-supported matrices.

The pinned skrifa 0.48.0/read-fonts 0.45.0 unscaled backend rounds source coordinates
through its documented integer font-unit pipeline: signed arithmetic-shift
flooring occurs before normalized FontMatrix multiplication. The normalized
matrix uses fixed-point rounded products and produces integer font-unit output.
For example, negative tiny controls first become (-1,-1); a 0.75 shear produces
(-2,-1). Applying a source affine transform before flooring would differ. Tiny
positive curves can become degenerate, but their cubic commands remain present.
This policy differs from fractional HarfBuzz/CoreText outlines and is separate
from HMTX/variation advance rounding. It does not claim exact mathematical
interpolation or general affine agreement across engines.

The existing 4096 commands per requested glyph and finite geometry bounds remain.
A malformed glyph/late overflow fails the complete requested outline. Existing
SVG and CPU admission publishes only after all requested geometry is prepared;
prior drawing state remains intact on error. Font loading, shaping and drawing
allocate and backend work/memory is not globally bounded by output limits. Run
these operations on UI/worker threads, never audio callbacks.

On macOS, opt-in `ravet-webview/postscript-fonts` includes system-fonts and adds
exact CoreText OpenType PostScript (format 1) catalogue loading. The original
glyf-only feature-off path remains. Copied table sources require unambiguous
CFF/CFF2, no glyf/loca, complete checked OTTO reconstruction and existing owner,
identity, unavailable/downloadable/auto-activation refusal policies. Plain
PostScript format 4 remains unsupported even if CoreText derives that descriptor
from an authored OTTO fixture. No implicit native format correction is performed.

The normal `postscript_fonts` standalone reference uses the same exact selection
model and vector/pixel controls, copied native data and existing renderers. It
requires the selected PostScript name to exist and begin with an admitted CFF
face; it never selects a replacement. The standard Audio/MIDI popup is enabled.
The hidden native fixture has measured eight installed CFF1 styles; separately,
a process-scoped first-party CFF2 fixture consumed native copied source at user
100/400/650/900 in both renderers and unregistered afterward. This is fixture
registration only, not a framework activation API or permanent font installation.

Independent HarfBuzz 14.4.0, FreeType 2.14.3 and CoreText probes preserve differences:
HB ignores the authored CFF1 shear, FT matches the integer policy, and HB/CT
retain fractional tiny/CFF2 points. At authored user 650 the actual normalized
coordinate is 13107/16384; CT advance 855.99609375 differs from the rounded
Rust/HB/FT 856. Engine differences are explicit, not hidden by loose tolerances.
Actual installed CFF1 proof compared 40 glyph advances/tight bounds against
CoreText; the scope is the tested faces, not every platform/font or screenreader.

References: [OpenType CFF2](https://learn.microsoft.com/en-us/typography/opentype/spec/cff2),
[Adobe CFF](https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf),
[skrifa outlines](https://docs.rs/skrifa/0.48.0/skrifa/outline/index.html), and the
pinned read-fonts `ps/cs.rs`/`ps/transform.rs` implementation. System font bytes
are held only in memory; checked-in authored fixture outlines are first-party.

Run the normal reference with an exact installed PostScript name:

```sh
cargo run -p ravet-examples --features postscript-fonts,live-ui --bin postscript_fonts -- --font D-DIN
```

Absence/unsupported initial metadata returns an error; the reference does not
choose another face. Different installed fonts remain consumer/licence owned.

Run the normal reference with an exact installed name:

```sh
cargo run -p ravet-examples --features postscript-fonts,live-ui --bin postscript_fonts -- --font Academico-Regular
```

The name must exist and be admitted on that machine. The default reference name
is D-DIN-Regular; absent or unsupported selection returns its diagnostic rather
than choosing a different face. Vector and pixel buttons consume the same copied
selected source and normalized coordinates. The normal reference contains no
fixture registration API.

Run the owned native qualification separately:

```sh
python3 scripts/check-postscript-outlines.py --target-dir target/postscript-proof-cargo
```

This requires macOS, the tested installed Academico/D-DIN/Bagnard styles, a C/C++
compiler, pkg-config and HarfBuzz/FreeType development packages. Missing sources
or disagreeing engines fail visibly. The script builds serially, records exact
commands/source hashes and uses only its own hidden windows. It registers solely
the checked-in first-party fonts at process scope and verifies unregistration.
Native CFF1 format 4 refusal, feature-off refusal and native CFF2 format 1 loading
are separate results. System bytes are never written; glyph/advance observations
are local evidence. Two authored browser batches each contain 30 prepared Rust
images plus three invalid-publication checks within the existing 1 MiB native
HTML limit. Chromium trusted controls are a separate product-tool check; no
native focus, system AX or screenreader certification is inferred.
