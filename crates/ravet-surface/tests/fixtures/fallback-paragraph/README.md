# Font-stack paragraph validation

This directory contains validation helpers, not new font assets. The Rust tests
reuse the authored rectangular primary/complement fonts from
[local fallback](../fallback/README.md), the existing licensed unmodified
[Noto Arabic](../text/README.md), and the authored variable-axis fixture from
[variation](../variation/README.md). System fonts remain memory-only in the
normal native consumer and never become oracle/browser/repository assets.

`harfbuzz-paragraph.c` is a first-party validation-only C helper. It uses an
independently installed public HarfBuzz library to shape each full-original
hard-domain item with the declared font face, variation, script/direction,
monotone grapheme clusters, BOT/EOT and remove-default-ignorable flags. It does
not participate in framework runtime or ordinary Cargo tests.
`check-oracle.py` compares the Rust example export's complete glyph IDs/global
byte clusters/positions/advances to 1e-9, then checks four separately authored
analytic wrap vectors and five fixed logical-byte visual-order vectors.
Feeding Rust's run segmentation to HarfBuzz alone would not independently prove
that segmentation. Profiles include U+2028/U+2029 isolate behavior, Arabic marks,
hard/soft wraps and selected heavy instance rendering. No general Unicode
conformance or cross-platform deadline follows from these fixtures.

`browser-harness.js` consumes prepared authored Rust image/source/geometry states
through the exact ordinary consumer HTML. Its 37 groups cover eight samples,
SVG/PNG, LTR/RTL, actual image decoding and Rust pixel hashes, rejected stale/bad
payload/geometry, noops and command acknowledgement. In the default standalone
wrapper it captures the consumer's async update function before the settings
wrapper is appended; framework snapshot delivery is deliberately separate and
checked in the actual dynamic native consumer. Browser states are prerecorded
Rust data, not a dynamic Rust runtime or independent layout expected results.
Trusted browser keys need product-tool actions; native scripted button/focus
checks do not certify OS keyboard or assistive technology support.

```sh
cargo test -p ravet-surface --features text,raster --test fallback --test fallback_paragraph --locked
cargo run -p ravet-surface --features text --example paragraph-fallback-oracle --locked
python3 scripts/check-fallback-paragraphs.py
```

The script needs `pkg-config`, `clang`, HarfBuzz C development files, and macOS for
its own dynamic/transport window checks. It writes receipts and authored browser
fixtures under `target/fallback-paragraphs`, uses one sequential Cargo job, keeps
default Audio/MIDI popup and closes every created window. Optional output/cache/
manifest/runner arguments support isolated source review. On non-macOS it runs
the contextual oracle only and records native evidence as unexecuted. Ordinary
library and pure model tests require neither native oracle tools nor a window.
