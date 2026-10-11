# First-party static GIF fixtures

`generate.py` creates original numeric palette patterns with Python/Pillow and
independently decodes their complete RGBA oracle. No external image assets are
used. `provenance.json` records dimensions, transparency, interlace and SHA256 for
both encoded and decoded bytes. Tests also construct independent literal LZW
streams to verify partial logical-screen canvases, local palettes and faults.
Regenerate with `python3 crates/ravet-surface/tests/fixtures/gif/generate.py`.
Pillow is a fixture-generation tool, not a runtime dependency.
