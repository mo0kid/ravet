# Local video worker boundary

The separate `ravet-media` prototype opens explicitly selected local video on
macOS and extracts timestamped first-track frames through AVFoundation. It stays
outside the portable processing/umbrella graph. No audio callback, window,
automatic playback clock, remote URL, device or consumer asset is selected.

Use `VideoReader::open(path)` on the worker that will also extract and drop it.
The reader is neither Send nor Sync. File paths are UTF8/no-NUL/<=4096 bytes,
regular sources <=2 GiB, duration (0,24h], oriented dimensions 1..4096. Preparation
and each extraction may block or allocate; decoder transient memory/time are not
globally bounded. `frame_at(seconds)` accepts [0,duration), requests zero native
time tolerance and returns actual source timestamp plus owned top-to-bottom
premultiplied RGBA8 sRGB pixels (<=64 MiB). Requests between source frames may fail;
no interpolation/cursor/clock or successful-all-frames guarantee is inferred.

Codec/container/color conversion follow the OS. The synthetic H.264/MP4 fixture
checks declared size/duration, selected timestamps, red/blue regions and white
top-strip orientation, with explicit lossy/decode color tolerance. It does not
establish general codec, colorimetric, A/V-sync, live-playback, GPU-window or
other-platform conformance. Xcode and macOS 12-targeting headers are required;
only the locally executed macOS version is validation evidence.
