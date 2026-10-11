# Explicit local font fallback

Enable `ravet-surface/text` (or the umbrella's `text` feature) to use
`fallback::TextFontStack`. Supply 1..16 ordered references to already loaded
`TextFont` objects. The stack copies the references; it neither discovers nor
registers fonts, reads files, nor loads network resources. Each font retains
its existing 16 MiB source bound; the declared source sum can reach 256 MiB.
Backend caches, temporary allocations and elapsed time have no global bound.
All loading, shaping and outline preparation belong on UI or worker threads.

`stack.shape(source, options)` accepts at most 4096 UTF8 bytes and requires an
explicit four-ASCII-letter script tag and horizontal direction. Existing finite
size (0,1024], language (1..63 restricted ASCII bytes), control-character and
32-feature bounds apply. Script tags are structurally checked, rather than
validated against a script registry. The consumer segments scripts, directional
runs and paragraphs. This API does not supply mixed-bidi or paragraph fallback.

Selection probes each whole Unicode 17 extended grapheme using the existing
pinned `unicode-segmentation` 1.13.3 tables. It chooses the first supplied face
whose actual shaping produces no glyph zero. This permits canonical composition
or decomposition and keeps combining/ZWJ units together. Probes and final shaping
retain the original surrounding source; the backend stores the nearest five
codepoints on each side. Beginning/end flags refer to the original source bounds.
Default ignorables remain shaping input, then disappear from glyph output. A
zero-glyph ignorable unit therefore selects the primary face. Variation selectors
and emoji follow backend behavior; absence of glyph zero does not certify their
presentation or sequence conformance.

Adjacent units assigned the same face merge before final shaping, preserving
same-face ligatures and kerning. Font units reverse in visual order for RTL while
the original characters and byte ranges remain unchanged. Cross-font ligatures
are not promised. Units unresolved by every face use the primary replacement
and retain explicit `missing_ranges()`. Unexpected final missing glyphs for
source previously declared covered reject the entire result with `Shaping`.
A complete result caps at 256 font runs, 8192 probes and 16384 aggregate glyphs;
failures publish no partial output. Output-count bounds do not bound backend work.

`FallbackText` owns the original source and copied script/language/features, and
borrows the matching immutable faces. It can outlive the stack object, but cannot
outlive any supplied face. Each `FallbackRun` exposes its exact face and original
stack index, logical byte range, local `ShapedText`, visual pen origin and
`source_cluster(glyph_index)`. These clusters are original UTF8 byte offsets,
rather than caret, grapheme or selection positions. Ignorable source can have no
glyph. The existing text-editing/caret APIs do not implicitly accept this new
multi-font output.

```rust
use ravet_surface::{fallback::TextFontStack, text::{TextFont, TextOptions}};
# fn prepare(primary: &TextFont, complement: &TextFont) -> Result<(), ravet_surface::text::TextError> {
let stack = TextFontStack::new(&[primary, complement])?;
let prepared = stack.shape("abcffi", TextOptions {
    script: Some(*b"Latn"), ..TextOptions::default()
})?;
for run in prepared.runs() {
    let _matching_font = run.font();
    let _source_range = run.range();
}
# Ok(()) }
```

`SvgDrawing::fallback_text` consumes those matching faces, converts upward font Y
to the requested SVG baseline, and rolls back document bytes and command count
on any failure. With `raster` enabled, `RasterText::from_fallback` prepares the
entire matching glyph geometry before canvas publication. Existing aggregate
65536-path-command, finite coordinate, SVG document and raster bounds apply.
Both renderers retain the selected-instance TrueType `glyf` outline
profile; CFF, bitmap/color fonts, variable-axis selection, hinting and ICC/display
color management are separate capabilities. Outline geometry carries no text
input, selection or assistive-technology certification.

## Complete standalone consumer

The `font_fallback` example uses the reusable Rust stack for five explicit
samples, two font orders, three sizes, and vector/pixel rendering. The same model
prepares all 60 outputs for headless export and the actual macOS window. Native
select controls send bounded single-field messages, Rust preflights the full
revision/source/shape/payload transition, and the frontend reconciles the prepared
output. The ordinary standalone window retains its default title-bar Audio/MIDI
settings popup; the visual-only model advertises no audio/MIDI devices. Native
resize preserves the prepared source/face identity and focused control.

```sh
cargo test -p ravet-surface --features text,raster --test fallback --locked
cargo test -p ravet-examples --bin font_fallback --no-default-features \
  --features font-fallback --locked
# Native consumer on macOS; default live-ui and settings popup remain enabled:
cargo run -p ravet-examples --bin font_fallback --features font-fallback --locked
# Portable Rust-produced data, without creating a window:
cargo run -p ravet-examples --bin font_fallback --no-default-features \
  --features font-fallback --locked -- --export > target/font-models.json
# Owned hidden-window validation, including actual resize/popup/cleanup:
cargo run -p ravet-examples --bin font_fallback --features font-fallback --locked -- --fixture
```

The two tiny [first-party fixtures](../crates/ravet-surface/tests/fixtures/fallback/README.md)
use rectangular outlines to make exact advances/coverage/ligatures observable.
Arabic uses the existing unmodified [Noto/OFL fixture](../crates/ravet-surface/tests/fixtures/text/README.md).
These are reusable test assets, not mandatory application fonts.

## Independent acceptance and limits

Optional `scripts/check-font-fallback.py` compiles a validation-only helper using
an independently installed HarfBuzz C library and `pkg-config`; ordinary Cargo
tests do not need them. Its `hb_buffer_add_utf8` uses the full original source and
selected item range with matching BOT/EOT/remove-ignorable flags. Ten profiles,
19 selected-face runs match exact glyph IDs and source clusters and scaled
positions/advances (1e-10 tolerance), including contextual Arabic and marks.
The current `hb-shape` CLI does not expose the needed item-context parameters.

```sh
python3 scripts/check-font-fallback.py
```

The owned browser oracle independently loads the tiny fonts through `FontFace`.
Six analytical CSS advances, including `ffi` and decomposed accents, match in
Chromium and WebKit within .02 logical points. Composed `á` matches in Chromium
(12 at size20), but the observed WebKit CSS policy chooses 11.123046875; the
explicit Rust stack and independent HarfBuzz both produce 12. That negative
observation is retained rather than claiming equivalent browser font selection.
Thirty browser SVG-versus-Rust CPU pixel pairs per engine pass fixture limits of
mean RGBA-byte difference <=1, maximum <=80 and <=512 changed pixels in 128x64.
These allow measured antialiasing differences; they do not certify every font,
size, script or platform. Exact analytical outline tests and independent shaping
cover the selected profile separately.

The actual hidden macOS consumer verifies 60 Rust/WebKit output combinations,
these 37 geometry checks/observations, invalid input, three measured resizes,
default settings-popup composition and callback cleanup. Chromium trusted native
select changes are recorded separately from the synthetic native bridge checks.
An earlier native-picker/Tab pass reports `hasFocus=false`; the final exact-source
pass reports true and the trusted Tab sequence visits sample, order, size and
rendering selectors. Both receipts are retained. This is browser document focus
evidence rather than global/native focus proof. DOM labels and source text are separate from native AX/VoiceOver or
screen-reader validation. No audio callback deadline or physical device claim.

See [decision 0241](decisions/0241-contextual-local-font-fallback.md),
[existing local text](surface-drawing.md) and [CPU raster](cpu-raster.md).

Immutable explicit [variation instances](system-fonts.md) remain caller-selected
faces in the stack; fallback does not change their coordinates or discover fonts.

## Wrapped mixed-direction composition

The separate [font-stack paragraph engine](fallback-paragraphs.md) accepts caller-authored
complete script spans and explicit base direction, reuses the same exact selected
faces and contextual coverage policy, and adds bounded wrapping/visual order plus
whole SVG/CPU consumption. The single-run API above retains its original contract.
