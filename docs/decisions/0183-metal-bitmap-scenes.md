# 0183: Explicit bounded Metal bitmap scenes

Status: prototype; native probe, three Rust scene/assets/media integration groups,
three rectangle regressions and thread-transfer doctest passed locally; package
all-target warning-denied lint passed.

Extend the explicit backend with immutable owned premultiplied RGBA8 bitmap
assets and ordered mixed rectangle/image scene commands, retaining the original
rectangle render contract. Rust validates exact dimensions/byte count and
RGB<=alpha before adopting assets; public readback adoption revalidates fields.
Sources contain no native handles and can cross worker boundaries. Renderer
ownership remains thread-confined and synchronous.

Bound raw commands by prepared0..4096 capacity, distinct referenced asset objects
to16, total distinct upload bytes to64 MiB and conservative clear/covered-pixel
work to67,108,864. Even clipped images count towards input budgets. Complete
preflight precedes upload/submission. Native transient resources/driver work remain
outside total-memory/time bounds. Source borrows survive only through the call;
independent output images remain owned and stable after reuse/rejection.

Clip destination geometry before f32 endpoint conversion and proportionally derive
source UV. Use normalized nearest sampling with edge clamping; tiny UV intervals
may collapse to one constant sample. Ordered source-over operates on numeric
premultiplied RGBA8 components with no implicit gamma/color-space transform, tint,
file/network loading, decoder, display clock, high-quality resampling or window.
Create fresh native shared source textures per call with prepared scene buffer
and separate rectangle/image pipelines. No audio callback/deadline guarantee.

An explicit consumer may adopt decoded VideoReader pixels. Its sRGB label does not
cause a hidden GPU color transform. Actual native two-by-two scale/orientation
passed locally; Rust checks include independent mixed-scene pixel oracle, asset
validation/distinct budgets/reuse and explicit exact2x synthetic video-frame
upload. See [Metal drawing](../metal-drawing.md). This extends [0179](0179-metal-offscreen-rectangles.md);
no display or cross-platform/GPU conformance is inferred.
