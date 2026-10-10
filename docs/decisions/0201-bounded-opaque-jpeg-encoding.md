# 0201: Optional bounded opaque baseline JPEG encoding

Status: implemented; executed validation is recorded separately in
[work status](../WORK_STATUS.md).

The default-disabled `jpeg-encode` feature includes `image` and adds an explicit
worker codec. `JpegOptions::new` validates quality 1..100 without clamping;
consumers choose 4:4:4, 4:2:2 or 4:2:0 sampling independently of quality. Encoding
uses baseline 8-bit JPEG, average chroma filtering and standard Huffman tables.
No progressive, optimized-table, metadata or color-management policy is inferred.

Every source alpha byte must be 255 before constructing the codec or output
sink. Consumers composite translucent assets explicitly. The immutable validated
RGBA asset retains its original pixels; codec RGB/YCbCr conversion is lossy.
Dimensions and source storage retain the image API's 4096 and 64 MiB limits.
An explicit 1..16 MiB private output gate publishes only complete owned bytes;
failure exposes no partial output and never writes to caller I/O. Codec working
allocations, execution time, whole-process memory and perceptual quality remain
unbounded UI/worker obligations.

Exact `jpeg-encoder` 0.7.1 selects only `std`, with defaults and SIMD disabled.
This configuration has no runtime dependencies or native C and the backend
forbids unsafe code. Archive and upstream commit verification, complete MIT,
Apache-2.0 and IJG texts, and original DCT attribution are preserved in
[the notices](../../crates/ravet-surface/third-party/jpeg.md). The upstream
`(MIT OR Apache-2.0) AND IJG` terms and required executable acknowledgement
remain independent of RAVET's first-party and commercial terms.

Tests inspect complete physical JPEG headers, sampling factors, quantization,
exact output budgets and source ownership across 36 first-party profiles. Strict
Zune decoding and an explicitly invoked independent Pillow matrix establish
local fixture evidence. Compiler checks on Linux/iOS establish compilation only;
they do not establish runtime, presentation, device, deadline or general fidelity
support. See [image assets](../image-assets.md) for use and limits.
