# 0206: Explicit static GIF assets

Status: Implemented; local codec/compiler validation is separate from platform
support and animation requirements.

The default-disabled `gif-image` feature provides a separate
`decode_static_gif(bytes, GifBackground)` helper without extending the existing
`ImageFormat` enum. Consumers explicitly choose transparent zero RGBA or opaque
global-palette logical-screen fill. A local palette overrides global image
colors; source transparency replaces canvas pixels with alpha zero. Interlacing
is decoded. The API publishes one complete immutable straight RGBA asset and
preserves source bytes; it performs no file/window/caller I/O.

Require exactly one complete image, source<=16 MiB, logical-screen/frame
dimensions1..4096/output<=64 MiB and8192 total structural block/subblock records.
Preflight rejects second images, looping application extensions, plaintext or
unknown extensions, user-input flags, invalid palettes/descriptor extents,
reserved control bits and trailing data. Bounded GIF89a comments/non-loop opaque
applications are ignored; GIF87a extensions reject. Delay/disposal scheduling,
animation compositing and color-management metadata are consumer work.

The reviewed gif0.14.2 Indexed decoder enables consistency and LZW end-code
checking; the wrapper validates every index and transparency entry before own
RGBA expansion. Indexed decoding avoids the backend RGBA path silently leaving
absent palette entries unchanged. Decode through the LZW end code even after
the expected pixel buffer fills, rejecting extra decoded pixels; requesting the
next frame alone could skip the remaining compressed bytes.

Pin gif0.14.2 and weezl0.1.10 with defaults disabled/std only, no color_quant or
native C. Preserve both original MIT texts and the complete identical Apache-2.0
text, verified primary archive hashes and upstream revisions in
[notices](../../crates/ravet-surface/third-party/gif.md). Backend working
allocations, allocator failure, execution time and total RSS are not bounded by
the framework asset limits.

Independent first-party Pillow generation/decoding and literal-LZW fixtures test
exact global/local/interlaced/transparent pixels, partial canvas policy, complete
prefix/trailing/LZW/profile faults and inclusive structural limits. Linux/iOS
ARM64 compilation is compiler evidence only. See [guide](../image-assets.md).
