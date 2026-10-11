# Explicit color preparation

`ravet_surface::color` prepares caller-declared sRGB or linear-sRGB pixels without
changing existing drawing, image, CPU raster or Metal numeric behavior. It is a
dependency-free module for UI/worker preparation; it performs no native, file,
profile discovery or network access.

```rust
use ravet_surface::{color::LinearRgba, drawing::Color};
let overlay = LinearRgba::from_srgba(Color {
    red: 255, green: 255, blue: 255, alpha: 128,
});
let black = LinearRgba::from_srgba(Color::rgb(0, 0, 0));
assert_eq!(overlay.source_over(black).to_srgba(), Color::rgb(188, 188, 188));
```

`LinearRgba::new` checks four finite normalized straight linear channels in
[0,1]. The value has private fields; `components` returns a copy. `from_srgba`
explicitly declares a `drawing::Color` to be straight encoded sRGB. `to_srgba`
returns quantized straight sRGB. Alpha stays linear. Hidden RGB at zero alpha
survives straight conversion; a completely transparent `source_over` result
canonicalizes RGB to zero.

`ColorEncoding::{Srgb,LinearSrgb}` selects the standard sRGB primaries and transfer.
`AlphaMode::{Straight,Premultiplied}` specifies RGB association **in the declared
encoded numeric space**. An encoded sRGB premultiplied pixel is unpremultiplied
before sRGB decoding, then output transfer precedes output premultiplication.
This distinction matters when a renderer accepts numeric premultiplied bytes.

## Borrowed assets and caller buffers

`RgbaView::new(width,height,pixels,encoding,alpha_mode)` borrows immutable exact
packed top-to-bottom RGBA8. Dimensions are 1..4096, bytes at most 64 MiB, with an
exact width*height*4 length. Every premultiplied pixel must have RGB<=alpha; zero
alpha requires zero RGB. A malformed final pixel rejects the entire view.
No decoder metadata or ICC/gamma chunk is inferred. The consumer decides whether
an `ImageAsset`'s decoded numeric bytes represent sRGB or another supported profile.

`convert_into` validates exact destination length before any write. Identical
profiles copy exact bytes. Same-encoding alpha reassociation uses exact rational
integer nearest half-up rounding. Changed transfer uses normalized f64 functions,
followed by nearest half-up byte quantization. RGBA8 linear storage is lossy;
arbitrary transfer roundtrips are not promised.

`composite_source_over` takes foreground and backdrop views, explicit output
encoding/alpha mode, and a caller destination. Both dimensions must match and the
output length must be exact. Full preflight precedes output writes. Safe Rust
borrows prevent input/output alias. Compositing is linear-light Porter-Duff
source-over; final alpha is linear, then RGB receives the chosen output transfer
and association. Both caller-buffer operations allocate nothing; caller-created
assets/buffers and consumer snapshots can allocate. Scalar powf/library work has
no real-time deadline guarantee.

The normalized [standard sRGB transfer](https://www.w3.org/Graphics/Color/srgb.pdf)
uses encoded breakpoint 0.04045 and divisor 12.92, otherwise exponent 2.4 with
constants 0.055/1.055; inverse linear breakpoint 0.0031308 uses the inverse exponent.
[Source-over](https://www.w3.org/TR/compositing-1/#simplealphacompositing) combines
premultiplied linear RGB as Cs*as + Cb*ab*(1-as), with resulting alpha
as + ab*(1-as). Nonzero result alpha recovers straight RGB before output encoding.
Normalized contribution weights avoid unnecessary underflow at subnormal alpha.

## Complete standalone consumer

```sh
cargo run -p ravet-examples --bin color_workspace --locked
cargo run -p ravet-examples --bin color_workspace --locked -- --fixture
cargo run -p ravet-examples --bin color_workspace --locked -- --export 128 amber
python3 scripts/check-color-preparation.py
```

The ordinary labelled opacity slider and white/amber choice send bounded compact
single-field local messages to a Rust model. It prepares both full 32x8 image
arrays using the reusable APIs and a numeric comparison path, publishes a checked
u32 revision plus a snapshot under 16 KiB, and renders those explicit straight sRGB
bytes through browser ImageData. Unknown/oversized/noncanonical/out-of-range
messages reject without source/revision change; exact no-ops retain revision and
exhausted revision refuses new changes. No DSP or device is opened. The default
native Audio/MIDI popup remains present. Headless `--export` creates no window;
`--export all` emits the bounded 512 white/amber/opacity reference models.

The explicit macOS fixture checks actual WK pixel arrays after five opacity
changes and a palette change through the native bridge, three measured resizes
with source/control/DOM focus retained, default no-device popup and explicit
closed-parent/editor callback cleanup. Trusted Chromium slider keys and native
select typeahead use Rust-produced reference models; the test-only browser bridge
has its own JS revision and performs no color math. Browser fixture transport is
separate from native Rust execution. Failed native-select key admission and a
palette blur with document.hasFocus=false remain diagnostic observations; later
slider focus/resize hasFocus=true is recorded separately.

The independent 70-digit Decimal check covers 256 transfer samples, 4096 profile
conversions and 2048 mixed-profile source-over cases. Nine color/allocation and
two consumer-message groups cover shape/output preservation, scalar thresholds,
encoded premultiplication, alpha-zero behavior, reassociation ties and 8192
prepared operations with zero allocations. These are numeric/browser/native
resource observations. No ICC/Display-P3/HDR/profile embedding, physical monitor
calibration, display transform, SVG parser/font discovery, screen-reader/native
AX, other-platform runtime, device/DAW or deadline certification is implied.
