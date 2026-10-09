# Unicode single-line reference vectors

Thirty-two unchanged vectors selected from
[Unicode BidiCharacterTest 16.0.0](https://www.unicode.org/Public/16.0.0/ucd/BidiCharacterTest.txt)
(date 2024-02-02, copyright 2024 Unicode, Inc.). They use explicit or first-strong
base direction, expected character levels and visual order, with no unspecified
`x` levels or prohibited single-line controls. Selection keeps the first 32 such
vectors; this is scoped regression data rather than the complete conformance suite.
The complete upstream file is 6880649 bytes, SHA256
`d04a51a90052dcd71c4e91ee5b3a9d973ee35c12406b5a99875ac8163c8f2804`.

The complete unmodified [Unicode License v3](Unicode-LICENSE.txt) is retained;
it was fetched from https://www.unicode.org/license.txt on 9 October 2026,
1995 bytes, SHA256
`e7a93b009565cfce55919a381437ac4db883e9da2126fa28b91d12732bc53d96`.
RAVET's first-party licence does not replace the data's terms. No proprietary
consumer text/font/session is included.

Tests compare base/byte levels and visual character ordering before shaping,
plus independent isolate/combining/UTF8-boundary/run-capacity cases. Glyph
mirroring/mark ordering remains the downstream shaper's responsibility.
