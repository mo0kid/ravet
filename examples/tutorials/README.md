# Interactive RAVET examples

Run from the repository root on macOS with Rust and Xcode command-line tools:

```sh
cargo run -p ravet-examples --bin midi-synth --locked
cargo run -p ravet-examples --bin effect --locked
cargo run -p ravet-examples --bin mpe --locked
cargo run -p ravet-examples --bin midi2 --locked
cargo run -p ravet-examples --bin layout_workspace --locked
```

Use the title bar's **Audio / MIDI…** button to choose devices, channel pairs,
sample rate, buffer size and active MIDI inputs. Press **Start audio** there.
These Audio/MIDI selections are remembered between launches. The main Start/Stop
button restarts using the selected settings; restore itself does not start audio.
The synth has waveform selection, VCA/VCF knobs and a shared ADSR. The effect is
a stereo delay with time, feedback and mix knobs, ping-pong and bypass; demo
bursts repeat until stopped. Its Demo loop switch is in the effect panel; turn
it off to hear the selected device input. MPE also plays live; MIDI 2.0 is an interactive packet inspector without audio/device
output. The standard popup is enabled by default in RAVET standalone windows and
can be explicitly disabled. Plug-in devices remain host-owned.

The [layout workspace](../../docs/grid-layout.md) is an independent resizable
application example: measured standalone content dimensions feed explicit Rust
grid tracks/spans and five labelled HTML panels. It retains the default settings
popup with truthful no-device state. `--fixture` validates only owned hidden
windows, actual WebKit geometry, native resizing and popup composition; it opens
no audio/MIDI devices.

Explicit `--render [new-path.wav]` (synth/effect) and `--offline` (MPE/MIDI 2.0)
retain hardware-free fixtures. WAV paths refuse overwrite. Portable tests build
with `--no-default-features`. `--ui-test <directory>` runs hidden WebKit fixtures
and writes screenshots without opening devices.

See the [guide](../../docs/examples.md) for use cases, plug-in builds, controls,
source map and current limits.

The `color_workspace` binary prepares explicit linear-light image composites in
Rust and presents the resulting sRGB bytes through native ordinary controls.
`--fixture` checks only an owned hidden window with no devices; `--export 128 amber`
prepares JSON without a window. See [color preparation](../../docs/color-preparation.md).

`svg_workspace` selects the optional `svg-image` feature. It imports first-party
SVG bytes and displays explicit Rust-prepared pixels with source/fit/size controls;
headless export and owned native fixtures remain separate. See the
[SVG image guide](../../docs/svg-images.md).

`cargo run -p ravet-examples --bin native_semantics` opens the macOS custom-control
reference with one Rust model for visual and explicit native semantic content.
The default title-bar Audio/MIDI popup is retained. See the framework
[native semantic guide](../../docs/native-semantics.md) for bounds and validation limits.

`font_fallback` prepares caller-owned contextual local font runs through reusable
Surface APIs and shows matching SVG/CPU output with labelled native selectors.
Run `cargo run -p ravet-examples --bin font_fallback --features font-fallback --locked`.
Use `--export` for portable Rust-produced data or `--fixture` for the owned hidden
macOS window checks; see [the scope and font notices](../../docs/font-fallback.md).

`system_fonts` selects `system-fonts,live-ui` to discover exact macOS families/styles
and show Rust-prepared selected glyph SVG/CPU images. The ordinary window retains
the default title-bar settings popup; `--fixture` owns hidden validation windows
and opens no devices. `--glyph-oracle` emits numeric glyph geometry for independent
CoreText comparison; `--export-first-party` emits authored synthetic image states
without system font assets. See [system fonts](../../docs/system-fonts.md).

### Exact-face paragraph workspace

`paragraph_workspace` uses the reusable Rust font-stack paragraph engine for
wrapped mixed Latin/Arabic, original-source isolates/separators, complete graphemes
and vector/PNG rendering. The normal macOS window loads exact Arial Regular with
explicit local alternatives, retains the default Audio/MIDI popup, and reflows from
measured window width. The same pure model supplies portable authored-font exports,
revision/command bounds and complete failure preservation. See the
[paragraph guide](../../docs/fallback-paragraphs.md) and optional reproducible
`scripts/check-fallback-paragraphs.py` native/oracle proof.

```sh
cargo run -p ravet-examples --bin paragraph_workspace --features paragraph-workspace --locked
cargo test -p ravet-examples --bin paragraph_workspace --no-default-features \
  --features paragraph-workspace --locked
```

### Selected PostScript fonts

`postscript_fonts` selects the default-disabled `postscript-fonts` feature, which
adds `system-fonts`, `ravet-webview/postscript-fonts` and
`ravet-surface/postscript-outlines`. It loads one exact installed OpenType
PostScript (CFF/CFF2) face named by `--font <PostScript name>`, with no activation
API or implicit replacement, and feeds the shared Rust selection model, SVG/CPU
renderers and explicit diagnostics. The normal window retains the default
title-bar Audio/MIDI settings popup. `--glyph-oracle`, `--export-first-party` and
`--browser-fixture <html>` support the reproducible
`scripts/check-postscript-outlines.py` proof. See the
[PostScript outline guide](../../docs/postscript-outlines.md).

```sh
cargo run -p ravet-examples --bin postscript_fonts --features postscript-fonts,live-ui --locked -- --font <PostScript name>
```
