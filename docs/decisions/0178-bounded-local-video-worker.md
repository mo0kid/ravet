# 0178: Explicit local-video worker extraction

Status: prototype; native and Rust local H.264/MP4 extraction, ownership and fault checks passed.

Add an explicit `ravet-media` package outside portable DSP and the umbrella graph.
Its macOS-only reader owns an AVFoundation asset and first-track image generator
through a narrow private C ABI. Rust validates selected local paths/file bounds,
owns public metadata/returned pixels and confines the reader to its creation
thread. Native code validates track duration/oriented dimensions, requests zero
timestamp tolerance, converts complete images to sRGB RGBA8 with premultiplied
alpha, and releases native ownership on that same UI/worker thread.

Bound paths to4096 UTF8 bytes, files to2 GiB, durations to24 hours and oriented
sizes to4096x4096. Frame destination allocation follows declared dimensions;
returns cannot exceed64 MiB. No partial frame is public on failure. Native decode
transient memory/time are not globally bounded. Opening does not prove whole-file
integrity or successful decoding at every timestamp. File mutation/races, codec
availability and color conversion remain OS/consumer responsibilities.

No implicit window, automatic playback clock, remote URL, audio decoding, A/V sync,
DRM, GPU view or device routing is selected. The owned reader is neither Send nor
Sync; create it inside the worker instead of transferring native ownership. Other
OS APIs remain unavailable. macOS12 build targeting is not a runtime support
claim; only separately recorded executions are evidence.

The public synthetic H.264/MP4 clip has independent two-second/64x48/25fps,
red/blue and top-white-strip expectations. The native probe caught an inverted
raster-row transform; removing it restored top-to-bottom output. Tests permit
explicit lossy/native color conversion tolerance rather than claiming exact PCM
or colorimetric conformance. See [local video](../local-video.md).

On local macOS27 ARM64 the three integration groups and thread-transfer
compile-fail doctest pass. All-target package Clippy with warnings denied passes.
Shared workspace/doctest/Rustdoc checks follow the next accepted group snapshot.
