# Embedded projects

RAVET provides a provisional embedded processing foundation, fixed-capacity Rust
and C++ adapters, a public stereo gain reference, and independent project
starters. These are headless processing components and synthetic fixtures.
Board startup, audio codecs, I2S/SAI/DMA, cache maintenance, pin assignments,
interrupt/core ownership and callback deadlines remain consumer integration work.
No physical board, audio device or deadline is certified by these offline tests.

## Choose a processing path

| Consumer | Processing path | Current scope |
|---|---|---|
| STM32 Rust firmware | `ravet-foundation` without default features, `ravet-embedded` | Allocation-free contracts and fixed interleaved f32/i16/i32 adaptation; consumer supplies MCU HAL/startup |
| STM32 C/C++ firmware | Rust static library plus `RavetEmbeddedGain.h` | Immediate stereo reference gain, caller-owned C state, 64 frames; matching target float ABI required |
| Arduino C++ firmware | Local `RAVET Embedded` C++11 library | Native fixed-capacity stereo f32 adapter and timed parameter gain fixture; board resources/toolchain must fit the workload |
| Raspberry Pi 3 Model B+ or Compute Module with Linux | Independent Rust Linux consumer | Headless fixture; use installed OS bitness for target selection; device integration is explicit |

Arduino cores cover different processor families; do not assume they all accept
an Arm Rust archive. The native C++ path is a small processing subset, not an
exporter for arbitrary Rust DSP. Arduino `architectures=*` permits core selection;
it does not certify every board or establish suitability for audio workloads.

Choose audio hardware against the actual processor workload, sample rate, block
size and channel count. Budget CPU/FPU capacity, RAM for DSP history and fixed
scratch, codec/I2S/SAI interfaces, DMA and cache/interrupt behavior. Some small
boards can compile the synthetic gain fixture while lacking resources for more
substantial effects or synthesis. Measure the worst callback workload on the
selected hardware before claiming a deadline. A successful fixture build is not
a recommendation for an audio product.

Pi Linux users must select the target for the installed OS bitness. The
[Pi 3 Model B+ processor is 64-bit](https://www.raspberrypi.com/products/raspberry-pi-3-model-b-plus/),
but its OS can be 32-bit. A Compute Module's model, carrier, OS and codec need
separate integration and hardware validation.

## Rust without an allocator

Depend directly on the processing crates, rather than the desktop umbrella:

```toml
[dependencies]
ravet-foundation = { path = "../ravet/crates/ravet-foundation", default-features = false }
ravet-embedded = { path = "../ravet/crates/ravet-embedded" }
```

Foundation uses `core` contracts and pinned pure-Rust `libm` 0.2.16 for
logarithmic/discrete parameter mapping without `std`. Its default `std` feature
retains desktop math behavior. Cargo features unify within a dependency graph:
a desktop consumer enabling foundation's defaults also enables `std`. Check the
actual firmware dependency graph. The existing signal, gain, instrument, graph,
device, runtime and editor crates are not made allocator-free by this change.
Snapshot mailboxes still require native 64-bit atomics; there is no emulated
atomic or implicit critical-section dependency on smaller MCUs.

`FixedBlockAdapter<P, INPUTS, OUTPUTS, FRAMES>` owns planar f32 scratch with
`4 * (INPUTS + OUTPUTS) * FRAMES` sample bytes, plus the processor/configuration.
Layout must exactly match the fixed channel counts; instruments can have zero
inputs. Preparation can choose a smaller frame maximum. Auxiliary buses and
output events are excluded from this adapter. Processor preparation may allocate:
choose a fully allocation-free processor when the firmware has no allocator.
The public embedded gain reference meets that narrower requirement.

Callbacks borrow complete interleaved input/output slices and events. Preflight
rejects lengths, prepared frame/event bounds, event order, offsets and invalid
payloads before invoking DSP. Faults silence the available active output prefix
and preserve backing tails. Processor backend faults can advance histories.
Zero-frame flushes retain sample timing and same-frame event order. There is no
implicit block splitting, resampling, transport clock or event delivery queue.
Reset and optional audio-owner session hooks are explicit. Stop such sessions on
the audio owner before preparation, transfer or deactivation.

Input encodings are f32, signed i16 and full-width signed i32; integer output
truncates toward zero and saturates, with NaN becoming zero. Internal processing
is f32, so i32 low bits may lose precision. Packed/left/right-aligned 24-bit audio
requires explicit codec-word conversion. Firmware exclusively owns adapters and
buffers; a mutable borrow is not DMA/cache/interrupt or cross-core synchronization.
Place large instances in suitable board-owned storage, not a small callback stack.

## STM32 and the C bridge

Choose the target for the exact Cortex core/FPU. These commands check libraries;
they do not link/flash a firmware image:

```sh
rustup target add thumbv6m-none-eabi thumbv7em-none-eabihf
cargo check -p ravet-embedded -p ravet-embedded-gain --no-default-features \
  --target thumbv6m-none-eabi --locked
cargo check -p ravet-embedded -p ravet-embedded-gain --no-default-features \
  --target thumbv7em-none-eabihf --locked
```

Rust's [Armv7E-M target documentation](https://doc.rust-lang.org/rustc/platform-support/thumbv7em-none-eabi.html)
explains Cortex-M4/M7 and hard/soft-float target distinctions. Do not select the
hard-float ABI for a core without the required FPU, or mix float ABIs across C
and Rust objects. No STM32 model or HAL is selected by these library checks.

The public reference [C header](../examples/embedded-gain/include/RavetEmbeddedGain.h)
exposes caller-owned state, init, block-start gain and interleaved stereo f32
processing. Gain is immediate, ID zero, amplitude 0..4, unity by default;
this reference does not implement the desktop gain's smoothing/bypass/meters.
The Rust processor accepts up to eight timed input events; the C ABI accepts
block-start gain only. C audio lengths are bounded to 128 samples, blocks to
1..64 prepared frames; zero-frame calls are legal. Input/output aliasing works,
state must not overlap audio. The C reference uses fixed callback stack scratch;
budget it separately from its caller-owned state. Pointer/alignment/lifetime/exclusive access
remain caller obligations. No handles, allocator, callback panic recovery or
arbitrary-processor export is introduced.

Build the standalone reference archive:

```sh
cargo rustc -p ravet-embedded-gain --no-default-features --features firmware-panic \
  --target thumbv7em-none-eabihf --release --locked --lib --crate-type staticlib \
  -- -C panic=abort
```

Link `target/thumbv7em-none-eabihf/release/libravet_embedded_gain.a` from the
consumer's firmware build. The explicitly selected reference panic handler spins
on panic; choose the product's reset/fault policy before deployment. Rust
firmware using the rlib should leave `firmware-panic` disabled and provide its
own handler. The consumer supplies linker/memory map, startup and compiler
runtime symbols. Archive construction alone is not final firmware link evidence.

## Arduino fixture

Generate an independent project with a local copy of the public C++ library:

```sh
python3 scripts/new-project.py --kind arduino --destination ../my-arduino-project \
  --name my-arduino-project --license GPL-3.0-only
```

Open `OfflineGain/OfflineGain.ino` in Arduino IDE and install
`libraries/RAVETEmbedded` as a local library. Select your exact board/core.
Install the core for the selected board, inspect its exact FQBN, and compile:

```sh
arduino-cli board listall
arduino-cli compile --fqbn YOUR_BOARD_FQBN \
  --libraries ../my-arduino-project/libraries ../my-arduino-project/OfflineGain
```

Replace `YOUR_BOARD_FQBN` with the chosen installed board/core identifier. No
board is selected or recommended by the generator.

The sketch prints
`RAVET offline gain PASS` through Serial once in setup. It opens no audio device
and does not configure a codec, Wi-Fi, I2S or an interrupt. Serial stays outside
any eventual callback. The C++11 `StereoAdapter<Processor, Capacity>` uses fixed
scratch and validates ordered finite parameter events; it covers stereo f32
processing only, without portable Rust MIDI/transport/state APIs. Consumer
processors supply prepare/process/reset/deactivate and their own bounded behavior.
The bundled gain shares the Rust reference's immediate amplitude/event semantics.

## Raspberry Pi Linux fixture

Generate a headless Linux project outside RAVET:

```sh
python3 scripts/new-project.py --kind pi-linux --destination ../my-pi-project \
  --name my-pi-project --license GPL-3.0-only
cd ../my-pi-project
cargo generate-lockfile --offline
cargo run --locked
```

The offline lockfile command assumes the pinned dependencies are cached; otherwise
use `cargo generate-lockfile` once with network access. Build natively on the Pi
with a Rust toolchain compatible with the current edition/core error APIs. Use
`rustc -vV` and `uname -m` to inspect the installed userspace/toolchain. Use
`aarch64-unknown-linux-gnu` for 64-bit GNU/Linux or
`armv7-unknown-linux-gnueabihf` for 32-bit hard-float GNU/Linux. Cross-linking needs
a matching Linux linker/sysroot; `cargo check` does not provide those. The
fixture prints a stereo gain result and opens no window, device or network port.

Live Linux audio is separate opt-in work through `ravet-device` and the board's
chosen ALSA device/codec. Native device builds need ALSA development headers and
pkg-config. Validate actual capture/output, channel routing, underruns and timing
on the Pi; do not infer those from a cross-build. A future standalone window
uses the standard Audio/MIDI settings popup unless explicitly disabled; these
headless fixtures do not create windows.

## Independent bare-metal starter and validation

```sh
python3 scripts/new-project.py --kind embedded-rust --destination ../my-stm32-project \
  --name my-stm32-project --license GPL-3.0-only
```

This creates a `no_std` consumer library with prepared fixed audio and a synthetic
fixture, not an entry-point firmware binary. Keep product code/assets/wrappers in
this private sibling. Absolute Rust dependencies and copied Arduino headers need
updating when RAVET moves or changes; see [project starters](project-starters.md).

Host behavior tests, no-default-feature tests, native C/C++ fixtures and optional
cross-target checks run through:

```sh
rustup target add thumbv6m-none-eabi thumbv7em-none-eabihf \
  aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf
python3 scripts/check-embedded.py --cross
```

The script requires a Unix-like host with Rust, Python, C and C++ compilers. It
builds a host static library and exercises the actual C symbols. The
[embedded workflow](../.github/workflows/embedded.yml) configures Ubuntu execution;
remote CI execution remains separate evidence. See [work status](WORK_STATUS.md)
for checks actually executed. Arduino compile/link environments are recorded in work status as build evidence
only. Physical Arduino/Pi runtime, STM32 final linking/flashing and real-time/device
compatibility remain unvalidated.
