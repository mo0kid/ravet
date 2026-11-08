# Plug-in export layer

**Status:** Prototype, under construction for 0.1.0. The design is
[decision 0281](decisions/0281-generic-export-layer.md). The product
description, saved-state envelope, editor protocol, automation and bypass
policies, a generic CLAP export and the generic C table for the other formats
exist in `ravet-plugin` (umbrella features `plugin`, `plugin-clap` and
`plugin-native`; crate feature `native` for the table). Generic adapters over
that table exist as prototypes for
[VST3](vst3.md#generic-product-adapter-010-prototype),
[AUv2](audio-unit.md), [AAX](aax.md#generic-aax-adapter-010) and
[LV2](lv2.md#generic-lv2-export). Product web editors open on macOS, Windows
and Linux ([decision 0293](decisions/0293-cross-platform-plugin-editors-and-asset-origin.md)),
and the [unified bundle tool](bundle-plugin.md) builds every format.

## Product description

A `Product` describes a plug-in once, usually as a `const`:

```rust
use ravet_plugin::{Category, Parameter, Product, StateFormat};

const PARAMETERS: &[Parameter] = &[
    Parameter::linear("gain_db", "Gain", -60.0, 12.0, 0.0).with_unit("dB"),
    Parameter::bypass("bypass", "Bypass"),
];
const PRODUCT: Product = Product::new(
    "Example Gain",
    "Example Audio",
    "1.0.0",
    "com.example.gain",
    Category::Effect,
    StateFormat { tag: *b"EXGAIN01", version: 1 },
)
.with_parameters(PARAMETERS);
```

`Product::resolve` validates the description on a control thread. It checks
names, the three-part version, the reverse-domain plug-in ID, layouts (effects
need audio input), auxiliary buses, at most 1,024 parameters, keys, defaults and
a single bypass toggle. It then resolves parameter IDs and format identities.

### Auxiliary buses and sidechains

Besides the main bus, a product may declare up to eight auxiliary buses: for
example a sidechain input, or the extra outputs of a multi-output instrument.

```rust
use ravet_plugin::AuxiliaryBus;

const PRODUCT: Product = Product::new(/* … */)
    .with_auxiliary_buses(&[
        AuxiliaryBus::sidechain("Sidechain", 2), // input only
        AuxiliaryBus::output("Direct", 2),       // output only
    ]);
```

Each bus has a name (nonblank, at most 64 bytes, no control characters, unique
among the product's buses) and up to 64 input and 64 output channels; it needs
at least one channel. A bus with inputs only is a **sidechain**. Every main
layout uses the same auxiliary buses. `Product::resolve` rejects more than eight
buses (`ProductError::TooManyAuxiliaryBuses`) and invalid ones
(`ProductError::InvalidAuxiliaryBus(index)`).

The processor is prepared with `ProcessConfig::with_auxiliary_buses` in
declaration order and receives every bus in `ProcessBlock::auxiliary` on every
block, with the declared channel counts, in every format. A bus the host leaves
disconnected or inactive carries silence, and its outputs are discarded. The
bypass policies silence auxiliary outputs while bypassed (`Crossfade` fades
them); auxiliary inputs have no dry path.

How each format presents the buses:

| Format | Sidechain inputs | Other auxiliary buses |
|---|---|---|
| CLAP | An audio input port after the main port, named after the bus (ID 1 + declaration index). | Input and output ports in the same order and naming; zero-channel directions get no port. |
| VST3 | A `kAux` input bus, inactive until the host activates it. | `kAux` buses in each direction they have channels, inactive by default, with one fixed arrangement (mono, stereo, otherwise discrete speakers). `setBusArrangements` accepts every bus, or only the main bus (auxiliary buses then keep their arrangement). |
| AUv2 | Input element 1 onwards (after the main input), named after the bus. | Elements in each scope where they have channels, with fixed channel counts and the main bus's sample rate. Multiple output elements are rendered once per cycle. Without a main input, the first auxiliary input is element 0 and the advertised channel table reports its channel count. |
| LV2 | Audio input ports with `lv2:isSideChain` and `lv2:connectionOptional`, after every existing port. | Optional audio ports, symbols `aux<B>_in_<C>` / `aux<B>_out_<C>` (bus and channel from 1). Parameter keys that collide fall back to `p<id>`. Port groups per bus direction; sidechain groups are `pg:sideChainOf` the main group. |
| AAX | The first sidechain maps to the single AAX side-chain (key) input, which Pro Tools delivers as mono: the key feeds every channel of the bus. Layouts without a main input (instruments) declare a main input stem matching their output so the key can be addressed; the product still gets no main input. Further sidechains are silent. | Mono and stereo output-only buses become aux output stems named "<bus> Mono"/"<bus> Stereo" (unless the name already says so; at most 64 bytes), and disable multi-mono. Wider output buses, and buses with both inputs and outputs, have no AAX route: inputs are silent, outputs discarded. |

These mappings have offline evidence only (unit tests, mock tables, the VST3 SDK
validator and auval on mocks); no host or DAW routing has been checked. See
[decision 0298](decisions/0298-plugin-auxiliary-buses.md).

Each parameter has a permanent **key**: an ASCII letter or underscore followed
by up to 63 letters, digits or underscores. Its host ID is either explicit
(`with_id`), so existing sessions keep loading, or derived from the key by
32-bit FNV-1a with the top bit cleared. IDs never depend on declaration order.
Two parameters whose derived IDs collide are rejected; give one an explicit ID.

## Exporting a plug-in

Implement `PluginExport` on a marker type that names the processor and its
product, then call `export_plugin!` once at the root of a `cdylib` crate:

```rust
use ravet_plugin::{PluginExport, Product};

pub struct GainPlugin;
impl PluginExport for GainPlugin {
    type Processor = ravet_gain::GainProcessor;
    const PRODUCT: Product<'static> = /* the product description */;
    fn create() -> ravet_gain::GainProcessor {
        ravet_gain::GainProcessor::default()
    }
}
ravet_plugin::export_plugin!(GainPlugin);
```

Any existing `Processor<f32>` can be exported unchanged. The crate must not
`forbid(unsafe_code)`, because the plug-in entry point is a foreign function;
`deny` is fine. `examples/gain-plugin` exports the gain reference this way, and
`python3 scripts/bundle-clap.py --plugin generic` bundles it. The
[tempo-synced delay](examples.md#tempo-synced-delay-plug-in) in
`examples/delay-plugin` is a fuller example with explicit parameter IDs, host
tempo, a tail and a web editor; `--plugin delay` bundles it. The [deterministic instrument](synth-plugin.md) adds notes, MIDI echo, CC response and switchable nonzero latency through the same generic layer.

The processor receives parameter changes as `ParameterValue` events with the
product's host IDs and physical values, already clamped and snapped to range.
`migrate_state` and `import_state` supply the [state hooks](#saved-state).

### Automation and bypass policies

Every format runs the product's processor inside a `PolicyProcessor`, so these
policies behave the same in CLAP and in the [native table](#native-table):

```rust
use ravet_plugin::{AutomationPolicy, BypassPolicy};

const PRODUCT: Product = Product::new(/* … */)
    .with_parameters(PARAMETERS) // must include a Parameter::bypass
    .with_automation(AutomationPolicy::PerBlock)
    .with_bypass(BypassPolicy::Crossfade { milliseconds: 10.0 });
```

| Policy | Behaviour |
|---|---|
| `AutomationPolicy::SampleAccurate` (default) | Parameter events keep the host's frame offsets. |
| `AutomationPolicy::PerBlock` | Every parameter event (values, modulation, gestures, global or scoped) moves to offset 0, ahead of the block's other events, in its original order. |
| `BypassPolicy::Processor` (default) | The processor receives the bypass parameter and implements bypass itself. |
| `BypassPolicy::Hard` | The processor keeps running; the output switches to the dry input at the bypass event's offset. |
| `BypassPolicy::Crossfade { milliseconds }` | As `Hard`, with a linear crossfade (more than 0, at most 1,000 ms) starting at the event's offset. A reset completes a running crossfade. |
| `BypassPolicy::Frozen` | While bypassed the processor does not run, so its history is frozen. The input, delayed by the processor's latency, is copied to the output. Without latency this uses `AudioBuffer::pass_through` and needs no scratch storage. Parameter events still reach the processor in a zero-frame block; notes and other events are discarded. The switch happens on block boundaries, using the block's last bypass value. Auxiliary outputs are silent while bypassed. |

Every policy except `Processor` consumes the bypass parameter: the processor
never receives its events, and `Product::resolve` rejects the policy unless the
product declares a `Parameter::bypass` (`ProductError::BypassPolicyWithoutParameter`).
`Hard` and `Crossfade` keep one prepared dry copy per output channel; with
fewer inputs than outputs the last input feeds the remaining outputs, and an
instrument's dry signal is silence. **The dry signal is delayed by the
processor's latency** (`Processor::latency_samples`, read at the start of
each block), so it lines up with the processed signal and with the host's
delay compensation. The delay line is fed even while the plug-in is not
bypassed, and a reset clears it. Its capacity is fixed at `prepare`: the
larger of the latency reported then and `Product::with_maximum_latency`, at
most `MAXIMUM_BYPASS_LATENCY` (65,536 samples). Cached parameters and state
are applied after `prepare`. So if they can raise the latency, declare the
maximum:

```rust
const PRODUCT: Product = Product::new(/* … */)
    .with_parameters(PARAMETERS)
    .with_bypass(BypassPolicy::Hard)
    .with_maximum_latency(2048); // lookahead can be switched on later
```

If latency goes above the capacity, the dry signal is delayed only up to the
capacity until the next `prepare`. The export adapters ask the host to restart
when latency changes. If latency changes while active, the dry signal jumps
straight to the new delay. A declared maximum above the limit is rejected as
`ProductError::InvalidBypassPolicy`. Processing and reset never allocate under
any policy.

`AudioBuffer::pass_through` (in `ravet-foundation`) is also usable directly: with
separate storage it copies input channel *c*, or the last input, to output *c*;
in place it does nothing; with no inputs it clears the outputs.

### Denormal guard

Every export wraps the policy processor in `ravet_native_dsp::DenormalProcessor`
with `DenormalMode::FlushBoth`: for the duration of each processing call,
denormal inputs and results are flushed to zero (FTZ/DAZ on x86-64, FZ on
AArch64), and the previous floating-point mode is restored afterwards, also on
unwinding. The product's processor and the policies therefore run with
denormals flushed in every format; products need no guard of their own. A
platform where the mode cannot be set fails activation (CLAP) or creation (the
native table) instead of processing without the guard.

The native product library additionally exports
`ravet_plugin_with_denormal_mode_v1(mode, callback, context)` from
`RavetPlugin.h`. Mode 0 preserves the current mode; mode 1 enters
`ScopedDenormalMode::FlushBoth`. It calls the C callback synchronously on the
calling thread and restores the exact prior control **and sticky status** on
return. Nested calls restore lexically. Returns 0 on success, 1 for an invalid
mode/null callback, or 2 for an unsupported actual environment without invoking
the callback. No allocation, token or arbitrary register setter is involved.

```c
static void process_native(void *context) { /* bounded DSP using context */ }
int32_t status = ravet_plugin_with_denormal_mode_v1(1, process_native, context);
```

The caller keeps the context valid and the callback must return normally on
this thread: no unwind, longjmp or retained scope. The symbol lives in the
exported product library; adapters need no separate call because their table
processing already has the guard. VST3 `Component::process`, AU render chunks,
AAX algorithm callbacks and LV2 `run` chunks dispatch through that same native
`process` pointer. CLAP process/active flush dispatch through the wrapped
bridge. Host event translation outside product DSP does not change the host's
floating-point policy. Native table tests observe flushing inside DSP, and the
instrument's C callback test checks nested scope entry and exact restoration.
See [the instrument](synth-plugin.md) and
[decision 0291](decisions/0291-instrument-midi-and-c-denormal-scope.md).

### CLAP export

The `clap` feature (`plugin-clap` on the umbrella crate) builds the CLAP side:

- The descriptor, audio ports, MIDI input/output ports and parameters come from the product.
  Audio ports are the main bus (port 0, flagged main) followed by the
  [auxiliary buses](#auxiliary-buses-and-sidechains).
  Parameters with integer steps, and the bypass toggle, are marked stepped.
- Parameter text uses `ravet_plugin::text`, shared with the native table. The
  bypass parameter and unitless 0/1 toggles read `On`/`Off`; hosts may type
  either the word or the number. Other values show two decimals below
  100, one from 100 (dropped when zero) and none for logarithmic values from
  1,000, so a fractional value never reads as a whole one.
- Host values are snapped to their ranges before the processor sees them, and
  recorded in a shared parameter cache. Values the host sets on the main thread
  update the cache immediately, so saving state never misses them.
- State uses the versioned envelope and the product's hooks. Loading state
  sends every value to the processor and resets it before the next block.
- Latency is reported at activation; a later change requests a host restart.
  Tail follows `Processor::tail`. Activation applies cached controls before reporting latency; active parameter flushes also detect a latency change.
- Declared MIDI output prepares at most 1,024 generated events and delivers
  validated short MIDI/portable notes through port 0. Both advertised ports use
  the MIDI dialect. Unrepresentable output rejects the generated stream;
  host queue rejection discards the remaining suffix. Successful audio remains
  valid, and no event is replayed on a later block. Cached-state synchronization
  discards generated zero-frame events, as in the native table.
- Every processing call runs inside the product's
  [policies](#automation-and-bypass-policies) and the
  [denormal guard](#denormal-guard).
- A product with an editor gets the CLAP GUI extension when the `editor`
  feature is on: embedded `cocoa` views on macOS, `win32` on Windows and `x11`
  on Linux. On Linux the editor also registers a 10 ms host timer
  (timer-support) to pump GTK while it exists; see [editors](#editors).
- A block carries at most 1,024 events, counting queued editor values. A
  block with more host events is rejected: its outputs are silenced and its
  values are not applied, as in the [native table](#native-table). Editor
  values that do not fit wait for the next block. Only core-namespace events
  are passed to the processor, so the preallocated event buffer never grows
  on the audio thread.

Limits for now:

- Only the first declared layout is used (no audio-port configurations).
- Processing is f32.
- Product extra state is not exported.
- Floating editor windows are not offered, and the CLAP `set_scale` hint is
  accepted but not applied (see [editors](#editors) for the Windows DPI limit).
- The automation reflection racing a concurrent state load can keep the
  automated value for that parameter.

### Native table

With the `native` feature, `export_plugin!` also exports
`ravet_plugin_v1`, which returns the `RavetPluginEntryV1` table declared in
[`crates/ravet-plugin/include/RavetPlugin.h`](../crates/ravet-plugin/include/RavetPlugin.h).
The VST3, AUv2, AAX and LV2 adapters are written once against it. The header is
the contract; the Rust side (`ravet_plugin::native`) behaves as follows:

- **Static data.** The product record, per-format identities, layouts (with
  their AAX type IDs) and parameter records are built once, on the first call,
  and live as long as the library. An invalid product, or one declaring only
  f64 processing, has no table (`NULL`).
- **Parameters.** Text uses the same formatting as CLAP; text that does not fit
  the caller's buffer is truncated at a character boundary and reported with 1.
  `normalize`/`denormalize` honour scale and steps and clamp and snap.
- **Instances** own the parameter cache, the editor edit queue and the
  processor inside its policies and denormal guard. `set_value` (a host-side
  change) and editor edits update the cache at once and mark the parameter;
  the next `process` delivers marked values at offset 0, ahead of the host's
  events. At most 1,024 events reach the processor per block; marked changes
  that do not fit wait for the next block. `save_state` writes the envelope
  from the cache. `load_state` applies all values or none, using the product's
  migrate and import hooks; before the next block the processor receives every
  value in a zero-frame block, then a reset (events the processor generates in
  that zero-frame block are discarded).
- **Processing** is 32-bit float. `prepare` takes the main bus followed by
  every [auxiliary bus](#auxiliary-buses-and-sidechains) with its declared
  channel counts (`bus_count = 1 + auxiliary_count`), and `process` takes the
  same buses; adapters supply silence for buses the host leaves disconnected.
  Buses, events and transport are
  converted by `ravet-native`; host buffers are copied into prepared scratch,
  so in-place hosts are safe. Host parameter values need not be exactly
  representable as `float`; they are clamped, snapped and written to the cache,
  and events for unknown IDs or with nonfinite values are dropped. `process`
  and `reset` never allocate. `latency_changed` reports a latency change after
  a block until the adapter reads `timing`.
- **Faults.** A null or misaligned instance pointer is rejected (1, or `NaN`
  from `get_value`) without being dereferenced. Every function catches panics
  and returns 2. After a panic,
  `process` (silencing its outputs), `reset` and `timing` return 2 until the next
  successful `prepare`, which recreates the processor; parameter, state and
  editor calls keep working.
- **Editor.** `editor_open` attaches the product's web editor, using the same
  editor host as the CLAP GUI extension, to the platform's view kind:
  `RAVET_PLUGIN_VIEW_COCOA` (`NSView`) on macOS, `RAVET_PLUGIN_VIEW_WIN32`
  (`HWND`) on Windows and `RAVET_PLUGIN_VIEW_X11` (X11 Window ID) on Linux;
  other kinds return 1. Resizable editors accept one to four times the
  product's size. `take_edits` returns the editor's begin/value/end edits for
  the adapter to report to the host. The web view polls the cache itself;
  `editor_idle` pumps GTK on Linux and does nothing elsewhere.

Limits for now: product extra state is not saved. Named-host runs of the Delay through the native adapters are
recorded separately in the [support matrix](support-matrix.md); those runs do
not qualify arbitrary products. The table's own unit tests drive every function through the
table (including a C99 compile of the header that checks every record offset
against the Rust definitions), and `examples/delay-plugin/tests/native.rs` uses
the delay's exported table end to end.

### AAX export

The generic AAX adapter in `crates/ravet-plugin/aax` turns any product into an
unsigned `.aaxplugin` on macOS and Windows from the product's table
(`ravet_plugin_v1`): `python3 scripts/bundle-aax.py --plugin delay --aax-sdk
<external AAX SDK>`, or `--package <crate>` for another product crate. Vendors
sign (PACE-wrap) their own builds. See [AAX](aax.md#generic-aax-adapter-010) for
what maps where, the Windows commands, checks and limits.

## Robustness at the host boundary

Exports run inside third-party hosts, so no panic or C++ exception may cross
into one ([decision 0287](decisions/0287-export-robustness-and-notices.md)):

- Table and CLAP entry points catch panics; the C++ adapters catch exceptions
  at every entry point that can allocate or call the table, and return the
  format's error. This relies on Rust's default `panic = "unwind"`. Do not set
  `panic = "abort"` in a product's release profile: a panic would then end
  the host process.
- Processing paths fault the block with silence instead of panicking, and do
  not allocate. A panic hook may still allocate and print while a fault is
  being handled; that happens only on the fault path.
- Every bundle script ships RAVET's licence, the third-party notices of the
  crates linked into the product, and the Rust standard library notice; see
  [dependency notices](dependency-notices.md).

## Linux and Windows builds

On Linux the bundle scripts run natively and produce CLAP, VST3 and LV2 bundles
(AAX is built on macOS and Windows only):

```sh
python3 scripts/bundle-clap.py --plugin delay        # target/clap/release/RAVET Delay.clap
python3 scripts/bundle-vst3.py --plugin delay --sdk /path/to/vst3sdk
python3 scripts/bundle-lv2.py --plugin delay --sdk /path/to/lv2
```

The CLAP bundle on Linux and Windows is the product library itself, renamed to
`<Name>.clap`. Editor products need GTK 3, WebKitGTK 4.1 and JSON-GLib
development packages on Linux, and on Windows the WebView2 SDK at build time
(`RAVET_WEBVIEW2_SDK`, an extracted `Microsoft.Web.WebView2` NuGet package; see
[bundle tool](bundle-plugin.md)) and the WebView2 Runtime on the user's
machine. MSVC builds link the SDK's static loader, so no `WebView2Loader.dll`
ships in any bundle.

### Windows cross-builds

`--target windows` on `bundle-clap.py`, `bundle-vst3.py`, `bundle-lv2.py` and
`bundle-aax.py` builds the x86_64 Windows bundles from Linux (or macOS)
([decision 0285](decisions/0285-linux-windows-plugin-builds.md)):

```sh
# Once: cargo-xwin and the Windows target. The first cargo-xwin run downloads
# Microsoft's CRT and Windows SDK and asks you to accept their licence.
cargo install --locked cargo-xwin
rustup target add x86_64-pc-windows-msvc
# Also needed: CMake, clang (clang-cl), and Wine for LV2 (optional otherwise).

python3 scripts/bundle-clap.py --plugin delay --target windows
python3 scripts/bundle-vst3.py --plugin delay --sdk /path/to/vst3sdk --target windows
python3 scripts/bundle-lv2.py --plugin delay --sdk /path/to/lv2 --target windows
python3 scripts/bundle-aax.py --plugin delay --aax-sdk /path/to/AAX_SDK --target windows
```

| Format | Windows bundle | Windows check to run there |
|---|---|---|
| CLAP | `target/clap/windows/release/RAVET Delay.clap` | `clap-validator.exe validate "RAVET Delay.clap"` |
| VST3 | `target/vst3/generic/delay/windows-release/VST3/Release/ravet_delay_vst3.vst3` | `validator.exe` from that build's `bin/` |
| LV2 | `target/lv2/windows/ravet_delay.lv2` | `ravet_lv2_contracts.exe` from `target/lv2/windows/generic-ravet_delay/` |
| AAX | `target/aax-plugin/windows-delay/RAVET Delay.aaxplugin` | `ravet_aax_check.exe` from the same directory (not Avid's validator) |

How it works:

- Rust is built with `cargo xwin build|rustc --target x86_64-pc-windows-msvc`.
  The C++ adapters are configured with
  `-DCMAKE_TOOLCHAIN_FILE=scripts/cmake/windows-xwin.cmake`, which wraps
  cargo-xwin's clang-cl toolchain file and restores the Windows system
  libraries it clears. The scripts take `PATH` and that toolchain's location
  from `cargo xwin env`; CMake's default generator (Makefiles) is used.
- The C runtime is linked statically on both sides (Rust
  `-C target-feature=+crt-static`, C++ `/MT`), so bundles and the cross-built
  tools need only system DLLs and run on Windows without the Visual C++
  Redistributable.
- Programs that must run at build time run under Wine
  (`CMAKE_CROSSCOMPILING_EMULATOR`): the LV2 Turtle generator (required) and
  the contract tests (skipped without Wine). Format validators are not run on
  the build machine.
- The VST3 SDK's Windows folder-icon step calls `attrib`; the scripts put a
  no-op `attrib` on `PATH`, so the bundle folder's system attribute is not set.

The official clap-validator Windows release needs the Visual C++
Redistributable; on a Windows installation without it, build clap-validator
from source with `cargo xwin build --release --target x86_64-pc-windows-msvc`
and `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-C target-feature=+crt-static"`.
Native Windows builds (MSVC) use the commands in each format's guide; they have
not been run. Evidence for both platforms is in [work status](WORK_STATUS.md).

## Format identities

All identities derive from the plug-in ID, using the same derivations as RAVET
Foundry:

| Format | Identity |
|---|---|
| CLAP | The plug-in ID |
| VST3 | Processor and controller class IDs from SHA-256 of `<id>/processor` and `<id>/controller` |
| AU | Type from the category (`aufx`, `aumu`, `aumf`); subtype from the plug-in ID, manufacturer from the vendor (four letters each) |
| AAX | Manufacturer, product and one type ID per layout, from SHA-256 of `<id>/MANUFACTURER`, `<id>/PRODUCT`, `<id>/MONO`, `<id>/STEREO` or `<id>/<inputs>x<outputs>` |
| LV2 | `urn:<id>` |

`IdentityOverrides` keeps any of them from an earlier product. Tests pin every
derivation, which never changes within a major version.

## Saved state

`ravet_plugin::state` defines one little-endian envelope for every format:
magic `RAVETSTA`, envelope version, product tag and state version, parameter
ID/value pairs and optional product extra data (up to 16 MiB). Read-only
parameters are not saved.

`state::restore` matches saved values to the current product by ID. Unknown IDs
(removed parameters) are dropped and missing ones take defaults. Values outside
the current range are rejected rather than clamped. State from a newer product
version is rejected. A product's `StateHooks` can `migrate` state written by an
earlier product state version, and `import` data without the RAVET magic, such
as sessions from another framework. Restoring never partially applies state.

These rules implement the saved-state section of the
[compatibility policy](compatibility.md).

## Editor protocol

`ravet_plugin::editor` defines the JSON messages between a plug-in and its web
editor, so products write no editor codec. Messages are objects with a `type`;
parameters are addressed by key, and values are physical unless sent as
`normalized`.

| Direction | `type` | Fields |
|---|---|---|
| Editor → plug-in | `ready` | `protocol` (currently 1) |
| | `begin`, `end` | `key` |
| | `set` | `key`, `value` or `normalized`, inside an open gesture |
| | `commit` | `key`, `value` or `normalized`: one complete edit |
| | `product` | `data`, product-defined |
| Plug-in → editor | `info` | `protocol`, product name/vendor/version/category, parameter metadata |
| | `snapshot` | `sequence`, all values by key, optional `telemetry` and `product` |
| | `changes` | `sequence`, changed values by key |
| | `product` | `data` |

`editor::parse` rejects messages over 64 KiB, malformed JSON, unknown types or
keys, edits to read-only parameters, nonfinite values, and a `protocol` other than
1. Values clamp and snap to the parameter's range. `Gestures` tracks open
gestures per editor: `set` needs an open gesture, `commit` is refused while one
is open on the same parameter, and `close_all` returns the gestures to end when
the editor closes. Several parameters may have gestures open at once.
`sequence` increases with each snapshot or change message, so editors can drop
stale updates.

The transport and injected JavaScript client are described below.

## Editors

`Product::with_editor(Editor { html, width, height, resizable })` gives a product
a web editor: one HTML page of up to 1 MiB, between 64 and 4096 pixels in each
direction. Resizable editors may grow to four times their size.

The editor is built only with the `ravet-plugin` feature `editor` (umbrella
`plugin-editor`), which adds `ravet-webview`:

```toml
ravet-plugin = { version = "…", features = ["clap", "native", "editor"] }
```

Without it, `clap` and `native` build no web view: the CLAP export offers no
GUI extension and the native table reports a 0 × 0 editor, so adapters show
none. Products without an editor leave the feature off and link no WebKit or
Cocoa bindings. The delay and gain examples turn it on.

The editor opens in every format that has an embedded editor on the platform
([decision 0293](decisions/0293-cross-platform-plugin-editors-and-asset-origin.md)):

| Platform | Web view | CLAP | VST3 | LV2 | AAX | AUv2 |
|---|---|---|---|---|---|---|
| macOS | Cocoa/WebKit | `cocoa` | `NSView` | `CocoaUI` | `NSView` | Cocoa view |
| Windows | Win32/WebView2 | `win32` | `HWND` | `WindowsUI` | `HWND` | — |
| Linux | GTK 3/WebKitGTK 4.1 on X11 or XWayland | `x11` | `X11EmbedWindowID` | `X11UI` | — | — |

Windows needs the WebView2 Runtime installed (Windows 11 ships it); editor
sizes there are the host's physical pixels, and WebView2 scales page content by
the monitor's DPI, so above 100 % display scaling a fixed-size page has fewer
CSS pixels than its declared size. Keep layouts responsive; content-scale
negotiation is open work. On Linux the parent is an X11 window, which need not
be an XEmbed socket: the editor's GtkPlug is mapped inside plain host windows
such as REAPER's. GTK work is pumped by `editor_idle` (VST3 `IRunLoop` timer,
LV2 idle interface) or a CLAP host timer; hosts already running GTK on Wayland,
or built on GTK 2/GTK 4, are refused. Keyboard input in Windows and Linux
editors is not validated yet. Evidence and hosts are listed in the
[support matrix](support-matrix.md) and [work status](WORK_STATUS.md).

### Editor assets

Images, fonts, scripts and styles need not be inlined into the 1 MiB page or
sent through 64 KiB messages. Declare them as `EditorAsset`s and refer to them
by relative URL:

```rust,ignore
use ravet_plugin::{Editor, EditorAsset};

const ASSETS: &[EditorAsset] = &[
    EditorAsset::new("app.js", include_bytes!("../editor/app.js")),
    EditorAsset::new("img/knob.png", include_bytes!("../editor/img/knob.png")),
    // An explicit content type when the extension does not give one.
    EditorAsset { path: "data/curve.bin", mime: "application/octet-stream", data: include_bytes!("../editor/curve.bin") },
];
// Product::new(…).with_editor(Editor { … }).with_editor_assets(ASSETS)
```

The page then refers to each asset by its relative path, exactly as declared:
a stylesheet link to `style.css`, a script element for `app.js` and an image
whose source is `img/knob.png`.

With assets, the page itself is served as `index.html` from a local asset
origin that also serves them: `ravet://editor/` on macOS,
`ravet-<id>://editor/` on Linux and `https://editor.ravet.invalid/` on Windows.
Requests are answered from the binary's memory (GET only, 404 for unknown
paths); nothing is read from disk and nothing reaches the network. Products
without assets keep the inline page. Limits, checked by `Product::resolve`
(`ProductError::InvalidEditorAsset`): at most 4,096 assets, 64 MiB each and
256 MiB in total; paths of at most 512 bytes from ASCII letters, digits and
`._-/`, with no `.`, `..` or empty segment; `index.html` is reserved. An empty
`mime` infers the type from the extension (HTML, CSS, JavaScript, JSON, SVG,
PNG, JPEG, GIF, WebP, ICO, WOFF/WOFF2, TTF/OTF, WASM, WAV, MP3, text), else
`application/octet-stream`. Each request copies the asset once. The Gain
example ships `asset.css` this way.

RAVET injects its editor client into the page's `<head>`. The client speaks the
[editor protocol](#editor-protocol) and exposes `window.ravetPlugin`:

| Member | Purpose |
|---|---|
| `info` | Product and parameter metadata, or `null` until it arrives |
| `value(key)`, `normalized(key)` | Current physical or 0–1 value |
| `on("info" \| "values" \| "product", listener)` | Metadata, value updates (`{values, changed}`) and product messages |
| `set(key, value)`, `setNormalized(key, n)` | One complete edit |
| `begin(key)`, `update(key, value)`, `updateNormalized(key, n)`, `end(key)` | A continuous gesture |
| `send(data)` | A product-channel message; `PluginExport::editor_message` may reply |

On macOS, focusing a text field in the page takes keyboard focus from hosts
that keep it for their own shortcuts, such as Pro Tools, and gives it back when
the field loses focus; see [keyboard focus](native-editor.md#keyboard-focus).
The editor protocol is unchanged.

`examples/gain-plugin/editor/index.html` is a minimal example;
`examples/delay-plugin/editor/index.html` draws knobs and a division selector.

Editor edits update the parameter cache immediately, then travel through a
bounded lock-free queue (256 edits) to whichever thread delivers parameter
events. That thread sends the host gesture-begin, value and gesture-end events,
and gives the processor the values at the start of the block; host automation
later in the block still wins. A complete edit is queued all at once or not at
all. If the queue is full, the edit is dropped and the page keeps showing the
host's value. Closing the editor ends every gesture the page left open.

The web view asks the plug-in for its next message 20 times a second.
Queued messages (`info`, product replies) go first, then a snapshot, which is
rebuilt only when a value changes. Messages are limited to 64 KiB. A product with
so many parameters that `info` exceeds this receives no `info`. Static data
belongs in [editor assets](#editor-assets); larger dynamic data still has to be
split by the product.

## Build and distribute the product

Use the [unified bundle tool](bundle-plugin.md) to build one product crate and
assemble CLAP, VST3, native AUv2, AAX and LV2 bundles. That guide documents SDK
settings/environment precedence, external manifests, output paths, vendor signing
hooks and optional format validators. [Foundry](foundry.md) generates consumer-owned
PluginExport projects and calls the same tool. Shipping, production signing and
host validation remain product responsibilities.
