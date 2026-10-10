# 0195: Optional owned worker image codecs

Status: implemented; focused/shared validation is recorded separately in
[work status](../WORK_STATUS.md).

PNG/JPEG are explicit byte codecs, independent of raster preparation, windows,
files and presentation. Immutable owned straight RGBA8 preserves shape and lets
consumers deliberately transfer or convert pixels for other renderers. Numeric
components receive no implicit ICC/gamma/EXIF transform; selected JPEG YCbCr
conversion belongs to the codec.

Source16 MiB, dimensions4096, output64 MiB,8192 structural records and32 JPEG
scans limit framework-controlled work before output allocation. Complete bounded
PNG chunk/JPEG marker-and-entropy walks precede strict backend decode and require
exact stream completion. Static PNG expansion/16-bit high-byte reduction and
selected8-bit mono/three-component JPEG are deliberate format limits. Backend
working memory, allocator failure and execution deadlines remain separate.

Exact png0.18.1 and zune-jpeg0.5.15 defaults stay disabled. JPEG uses std-only and
unsafe runtime paths disabled; PNG explicitly retains CRC/Adler verification and
ignores text/ICC decompression. Reviewed selected compression uses pure Rust
miniz; optional zlib-rs remains an unselected lock entry. Complete exact-archive
notices accompany the public code, preserving independent upstream terms.

PNG encoding writes only a bounded private owned sink. Failure exposes no partial
output; backend destructor completion cannot mutate external I/O. Straight/PMA
conversion is an explicit consuming integer operation. First-party independent
PNG/Pillow JPEG fixtures and generator hashes keep pixel evidence reproducible;
actual Metal readback and cross compilation have distinct limits. No other codec,
native presenter, physical host/device or real-time support is inferred.
