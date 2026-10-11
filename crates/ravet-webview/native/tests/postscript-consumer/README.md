# Owned PostScript native qualification

`src/main.rs` uses the normal tutorial's exact model and HTML, with authored CFF2
source registered solely for this fixture at process scope. It checks native
copied data at four explicit user coordinates in both SVG and CPU modes, resize,
the default Audio/MIDI popup and child-before-window teardown. Cleanup returns
and logs native unregistration; a failed cleanup refuses qualification.

The script compiles these source-only fixtures against the exact rlibs emitted
by the normal tutorial Cargo build; there is no separate dependency manifest or
additional package. `src/bin/authored.rs` extracts the same explicit selected
outlines for comparison with independently configured HarfBuzz, FreeType and
CoreText. All authored source is GPL-3.0-only. No installed font asset is included.
Use `python3 scripts/check-postscript-outlines.py`; its native admission and direct
protocol observations are separate from system AX/VoiceOver or live-device proof.
