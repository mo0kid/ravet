# 0179: Explicit bounded Metal offscreen rectangles

Status: prototype; actual native GPU and three Rust integration groups plus
thread-transfer compile-fail doctest passed locally.

Keep Metal in a separate `ravet-metal` package outside portable DSP and the
umbrella graph. Rust owns validated portable rectangles/colors, prepared staging,
input/work limits and independent returned pixels. A private Objective-C++ C ABI
owns the system-selected Metal device, queue, compiled pipeline, shared render
texture and bounded instance buffer. Native ownership is confined to its creation
UI/worker thread; creation/submission/wait/readback/destruction may allocate/block.

Use ordered source-over premultiplied RGBA8Unorm rendering. Clip finite portable
f64 geometry before f32 endpoint conversion. Skip empty/fully clipped/quantized-
degenerate instances while retaining raw input capacity accounting. Bound width
and height to4096, raw rectangles to4096, returned pixels to64 MiB and conservative
clear/covered-pixel work to67,108,864. Preflight complete inputs before submission;
GPU failure returns no partial public image. Native transient resources/driver
work remain outside total-memory/time caps.

No implicit window, presentation clock, antialiasing, paths, text, images, rounded
corners, gamma/color-space conversion, software fallback or other-OS binding is
selected. Returned CPU pixels are independent owned snapshots. Backend support
and real-time deadlines require separate evidence; macOS12 build targeting alone
is not runtime support.

The actual native probe caught a shader compile failure caused by a reserved
parameter name. Renaming it restored rendering; creation now explicitly rejects
missing library/functions before invoking pipeline creation, avoiding a native
validation assertion on failed compilation. Native strict-warning compilation
and local GPU top/bottom orientation then passed. Rust integration compares an
independent half-open pixel/source-over oracle, bounds/work rejection, retained
images and repeated worker-thread lifecycle. See [Metal drawing](../metal-drawing.md).

[0183](0183-metal-bitmap-scenes.md) extends this backend with explicitly validated
bitmap assets and ordered mixed scenes; the rectangle-only API contract remains.
