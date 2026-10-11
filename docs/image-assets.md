# Owned worker image assets

The separate optional [SVG importer](svg-images.md) supplies owned sRGB
premultiplied image pixels from a bounded resource-free static SVG profile.
It does not select these static bitmap codecs.

Enable the default-disabled `ravet-surface/image` or umbrella `ravet/image`
feature for `image::{ImageAsset, ImageFormat, decode_image, encode_png}`.
Select PNG or JPEG explicitly and supply owned/borrowed bytes; these APIs create
no files, windows, network requests, image cache or implicit presentation.
Use them on UI/worker threads: allocation, codec working memory and execution
time have no real-time bound.

`ImageAsset::from_straight_rgba` adopts exact packed top-to-bottom numeric RGBA8.
Dimensions are 1..4096 and pixels at most64 MiB. Private dimensions/pixels preserve
the validated shape; `pixels()` borrows immutable bytes, cloning copies them and
`into_straight_rgba()` transfers their allocation. `into_premultiplied_rgba()`
consumes the asset and rounds RGB×alpha with `(component * alpha + 127) / 255`,
preserving alpha. Read dimensions before consuming pixels for explicit
`MetalBitmap::from_premultiplied_rgba` adoption or another presenter.

Encoded input is 1..16 MiB. Structural preflight walks at most8192 PNG chunks or
JPEG markers, checks complete extents and dimensions before allocating framework
pixels, and requires exact IEND/EOI completion with no trailing bytes. Backend
validation remains responsible for codec semantics. These input/output limits
do not bound all backend buffers or process memory.

PNG decoding supports one static grayscale, grayscale-alpha, RGB, RGBA or indexed
frame, including Adam7, sub8-bit samples and tRNS. Sixteen-bit samples retain their
high byte. It rejects animated acTL/fcTL/fdAT, unknown critical chunks, duplicate
IHDR and nonconsecutive IDAT. CRC and Adler checks are explicitly enabled; text
and ICC payload decompression is ignored. The backend's best-effort64 MiB working
limit excludes framework pixels and is not a whole-process memory guarantee.

JPEG supports selected8-bit mono/RGB/YCbCr baseline and progressive streams with
at most32 scans. A marker/entropy walker handles stuffed bytes, fill and restart
markers before strict safe decoding. CMYK/YCCK, other precision, arithmetic,
hierarchical and other unsupported forms reject. JPEG output is opaque RGBA;
codec YCbCr conversion applies, while EXIF orientation, ICC and gamma transforms
are consumer choices. PNG components also receive no color-management transform.

`encode_png(&asset, maximum_bytes)` emits RGBA8 into an owned private sink with
an explicit1..16 MiB output budget. Encoding/overflow errors publish no partial
bytes. Backend destructor writes can only reach that private sink; once rejected,
its gate accepts no further bytes. There is no external I/O or caller sink.
WebP, animation, SVG parsing, metadata editing and resampling are outside
this API.

Enable the separate default-disabled `ravet-surface/gif-image` or umbrella
`ravet/gif-image` feature for `image::{GifBackground, decode_static_gif}`. It
includes `image`; image-only consumers do not select GIF dependencies. Exactly
one complete GIF87a/89a image is decoded with explicit `Transparent` zero-RGBA
or `LogicalPalette` opaque global-background fill. The latter requires a valid
global palette; a local palette overrides global colors inside the descriptor.
The descriptor must fit the logical screen. Indexed transparency replaces those
canvas pixels, including their numeric RGB, with alpha zero. Interlacing is
decoded; no frame timing, disposal or animation compositing is scheduled.

Full structural preflight enforces 1..16 MiB source, 1..4096 screen/frame
dimensions, 64 MiB RGBA output and 8192 total blocks/subblocks. Missing/invalid
palette entries, incomplete/end-code-invalid LZW, extra decoded pixels, second
images, looping applications, plaintext/unknown extensions, interactive flags
and trailing file data reject. GIF89a comments and non-loop opaque applications
are bounded and ignored; GIF87a extensions reject. Color management, metadata,
file I/O, codec working allocations and execution deadlines remain explicit
consumer/worker obligations.

First-party [GIF fixtures](../crates/ravet-surface/tests/fixtures/gif/README.md)
compare exact independently decoded Pillow pixels and independent literal-LZW
partial canvases. Five groups cover local/global palettes, transparency,
interlacing, every truncated prefix, malformed LZW, extension/animation faults,
source/dimension bounds and inclusive 8192-record limits. Complete independent
MIT/Apache-2.0 [notices](../crates/ravet-surface/third-party/gif.md) retain exact
gif0.14.2/weezl0.1.10 pins; defaults disabled, std only, no color_quant/native C.
See [decision0206](decisions/0206-explicit-static-gif-assets.md).

Enable the separate default-disabled `ravet-surface/jpeg-encode` or umbrella
`ravet/jpeg-encode` feature for `image::{JpegOptions, JpegSampling, encode_jpeg}`.
This includes `image`; image-only consumers do not select the encoder. Construct
options with checked quality 1..100 and explicit `S444`, `S422` or `S420` sampling.
Encoding uses baseline 8-bit JPEG, average chroma filtering and standard Huffman
tables. Quality never implicitly changes sampling. All alpha bytes must be 255
before encoding starts; composite translucent pixels explicitly first.

`encode_jpeg(&asset, options, maximum_bytes)` preserves the source and publishes
only complete owned output within an explicit 1..16 MiB budget. Failures expose
no partial bytes or caller I/O. Numeric RGB receives codec YCbCr conversion,
with no ICC/EXIF metadata or gamma/orientation transform. Compression is lossy;
codec working allocations, execution time and perceptual quality are not bounded.

```sh
cargo test -p ravet-surface --features image --test image --locked
cargo test -p ravet-surface --features image --doc --locked
cargo test -p ravet-metal --test image --locked
cargo test -p ravet-surface --features jpeg-encode --test jpeg --locked
cargo test -p ravet-surface --features jpeg-encode --test jpeg --locked -- --include-ignored
cargo test -p ravet-surface --features gif-image --test gif --locked
```

The first-party fixture [generator/provenance](../crates/ravet-surface/tests/fixtures/image/README.md)
uses independent struct/zlib PNG rows and Pillow JPEG oracles. Tests cover exact
PNG pixels, palette/alpha/16-bit/Adam7, seven JPEG profiles, owned conversion,
truncation, corruption, animation, limits and bounded output. JPEG fixture
differences are at most3 component bytes, with a4-byte test tolerance; this is
fixture evidence rather than a general fidelity guarantee. The optional Metal
test requires an actual local macOS device and checks explicit PMA adoption.
Other-platform compiler checks establish compilation only.

JPEG writer tests cover 36 dimension/quality/sampling profiles, complete physical
headers, deterministic output, exact output capacity and whole-source alpha
rejection. The explicitly invoked ignored test requires local Python/Pillow and
decodes all 36 profiles independently. Its maximum component difference from
the strict Zune decoder is 2, within a 4-byte fixture tolerance; this is local
fixture evidence, not a general fidelity guarantee.

Exact png0.18.1 and zune-jpeg0.5.15 defaults are disabled; JPEG selects only `std`
and disables unsafe runtime paths. PNG uses the selected pure-Rust miniz backend;
the optional zlib-rs lock entry is not selected. Complete backend/transitive
[notices](../crates/ravet-surface/third-party/image.md) and
[licensing](../LICENSING.md) remain separate from RAVET's licence. See
[decision0195](decisions/0195-owned-worker-image-codecs.md).

The JPEG writer pins `jpeg-encoder` 0.7.1 with defaults disabled and only `std`;
it has no runtime dependencies or native C. SIMD is disabled and this backend
configuration forbids unsafe code. Complete independent MIT/Apache-2.0/IJG
[notices and DCT attribution](../crates/ravet-surface/third-party/jpeg.md) include
the required executable acknowledgement. See
[decision0201](decisions/0201-bounded-opaque-jpeg-encoding.md).

Caller-declared standard sRGB or linear-sRGB pixels can be prepared through the
dependency-free [explicit color APIs](color-preparation.md). These borrowed views
validate alpha association, transfer into caller buffers and composite in linear
light; they infer no ICC/gamma metadata and leave numeric decoding unchanged.
