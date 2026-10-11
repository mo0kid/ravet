# First-party fallback fixtures

These fonts contain only new primitive rectangular outlines created for RAVET's
reusable tests. The complete generator is [generate.py](generate.py); the files
and source follow the repository GPL-3.0-only/source commercial licensing policy.
They are not modified or renamed third-party fonts. The separate unmodified
[Noto Sans Arabic fixture](../text/README.md) retains its own OFL licence.

Primary covers `a`, `f`, `i`, Arabic beh and space, and substitutes `ffi` through
GSUB. Complement covers `a`, `b`, `c`, combining acute and space, intentionally
omitting precomposed `á` so decomposition and whole-grapheme fallback are tested.
Units-per-em is 1000, normal outlines rise 700 units, mark outlines rise 750..850;
advances are declared in the generator. Empty space and replacement outlines are
explicit. There is no copied artwork, system-font registration or application asset.

| File | Bytes | SHA256 |
|---|---:|---|
| RavetFallbackPrimary.ttf | 1328 | ab5cc41dd42f662c77085b1fa7bbe1bd1be3ffc9809cdd1fed145751891faf58 |
| RavetFallbackComplement.ttf | 1208 | e7b26b9a289fac11b25fda4d72659d6087fb641cbe28ff6004cca454afbaf6aa |

Optional development-only regeneration used fontTools 4.62.1 (MIT), with fixed
creation/modification timestamps and deterministic source. It adds no runtime
Cargo dependency. From this directory, run `python3 generate.py` in an environment
with that version installed and verify both hashes before accepting generated
changes. This fixture only supplies declared synthetic geometry/coverage; it is
not an OpenType conformance corpus or typography suitable for a product.
