# First-party color preparation reference

`reference.rs` compiles actual portable RAVET drawing/layout/color source directly
without Cargo or native frameworks. It emits 256 scalar transfer samples, 4096
conversions covering all source/output sRGB/linear and straight/premultiplied
profiles, and 2048 source-over cases. A fixed first-party LCG creates local sample
channels; explicit white/black and hidden-alpha-zero cases supplement those values.
No proprietary image, font or external fixture/code is copied.

```sh
python3 scripts/check-color-preparation.py
```

The script independently implements normalized sRGB transfer and Porter-Duff
source-over with 70-digit Python Decimal arithmetic, exact rational same-encoding
alpha reassociation, nearest half-up byte rounding and full scalar/pixel checks.
It compiles with Rust warnings denied, caps reference JSON at 2 MiB, checks exact
case counts and writes owned output under `target/color-preparation` by default.
`--output` and argument-parsed `--runner` are explicit, with no shell.

Primary formula provenance: [standard sRGB specification](https://www.w3.org/Graphics/Color/srgb.pdf)
and [W3C compositing](https://www.w3.org/TR/compositing-1/#simplealphacompositing).
These formulas are independently implemented; source prose/code and runtime
profiles are not imported. Numeric agreement does not certify physical display
calibration, ICC, native/window behavior or callback deadlines.
