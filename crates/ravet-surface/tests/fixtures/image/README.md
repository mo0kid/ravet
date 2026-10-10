# First-party image codec fixtures

All pixel patterns and PNG/JPEG files here were generated for RAVET. No external
photographs, product assets or user sessions are included. Raw .raw oracles use
packed straight RGBA8. PNG grayscale/palette/tRNS/16-bit and Adam7 rows were built
independently with Python struct/zlib; the Adam7 fixture uses explicit seven-pass
filter-zero rows. RGBA PNG and baseline/progressive RGB/grayscale,4:2:2,4:2:0 and
EXIF-orientation6 JPEG fixtures were produced/read independently with Pillow.
JPEG raw files retain unrotated numeric output, without EXIF transpose. The first
measured decoder comparison is at most3 component bytes for these patterns;
that tolerance is fixture evidence rather than an arbitrary JPEG quality promise.
Fixtures are distributed under RAVET's repository licence. Generator and hashes
are included alongside provenance; regeneration uses explicit local Python/Pillow.
