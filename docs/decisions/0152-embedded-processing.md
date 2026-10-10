# 0152: Embedded processing boundaries

Status: portable processing/adapters, public gain references and independent
starters implemented; hardware validation remains separate evidence.

Make the existing foundation usable without std or an allocator, preserving its
processor/event/audio/schema contracts. Use core for portable traits/errors and
pinned pure-Rust libm only for no_std parameter math; keep default desktop math.
Keep the existing native-64-bit atomic condition for snapshots rather than
introducing critical sections or software atomic emulation silently.

Add an explicit no_std fixed-capacity f32 interleaved adapter instead of pulling
desktop signal/surface/device/runtime graphs into firmware. Const channel/frame
arrays bound adapter scratch; processors retain their own preparation/callback
obligations. Preflight the complete block before DSP, preserve sample offsets and
backing tails, and enforce fault silence even for a misbehaving processor.
Integer PCM conversion is explicit; f32 precision and packed 24-bit limits are
stated. Firmware retains drivers, startup, memory maps and DMA/interrupt ownership.

Provide an allocation-free immediate stereo gain reference with a narrow
caller-owned C ABI for STM32 C/C++ static-library consumers. Its f64/u32/f32 state
layout avoids opaque allocating handles. Pointer validity and matching target
float ABI remain consumer obligations. Rust firmware supplies its own panic
handler; standalone C consumers can explicitly select the reference spinning
handler. No generic arbitrary-DSP C exporter or desktop reference parity is implied.

Arduino cores include processor/toolchain families that cannot be assumed to
accept a stable Arm Rust archive. Provide a native header-only C++11 stereo subset and
matching gain fixture for such consumers. This is an independent processing
contract/reference, not arbitrary Rust DSP cross-compilation. Wildcard Arduino
architecture metadata is build eligibility, not a hardware support matrix.

Pi 3 Model B+ and Compute Module Linux consumers use the same Rust processing
boundary, with target selection driven by installed OS bitness. Headless library
and runnable fixtures stay separate from actual device/UI integration. Generate
all consumer starters outside RAVET; preserve the existing offline default and
repository/existing-destination protections. Cross compilation establishes library
feasibility only. Record actual board/core/codec/runtime/deadline evidence before
making a hardware support claim.

Keep small-board compile fixtures in the validation record rather than presenting
them as recommended audio targets. Hardware choice follows measured workload,
CPU/FPU, RAM, codec interfaces and DMA/interrupt behavior; successful synthetic
gain compilation alone does not establish suitability for audio applications.
