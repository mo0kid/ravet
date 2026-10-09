# Public typography fixture

Unmodified Noto Sans Arabic variable font (default instance), downloaded from
[google/fonts commit 2eb0b48d5f760f62e286216f0859a8c540dbc1bd](https://github.com/google/fonts/tree/2eb0b48d5f760f62e286216f0859a8c540dbc1bd/ofl/notosansarabic).
The upstream name is `NotoSansArabic[wdth,wght].ttf`; this local file is named
`NotoSansArabic.ttf` for fixture convenience. Font bytes are not modified.

Copyright 2022 The Noto Project Authors. This fixture retains the complete
unmodified [SIL Open Font License 1.1](OFL.txt). It is public reusable test data,
rather than a proprietary consumer asset or a mandatory product font.

| File | Bytes | SHA256 |
|---|---:|---|
| NotoSansArabic.ttf | 844676 | 63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e |
| OFL.txt | 4382 | 07fc70bfeb985cc1a87a8587d0a0c80bab11c86c9dc3fd95b6f0cb332f983e96 |

`tests/text.rs` exercises contextual Arabic/marks/visual byte clusters, scaled
positions, feature overrides, missing glyphs and bounds. The optional explicit
`scripts/check-text-shaping.py` compares eight actual runs with a supplied
`hb-shape` binary; no external shaper is required for ordinary workspace tests.
This fixture does not establish every script/font/paragraph or renderer profile.
