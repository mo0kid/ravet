# RAVET Foundry

Foundry creates, reopens, configures and builds independent RAVET consumer
projects. Its interface uses RAVET's shared web controls/theme and native editor
backends. The Rust project engine is shared by the GUI and CLI; there is no
browser server, account or remote service in the application.

This is a development prototype. The macOS application and generated projects
have local validation evidence. Windows x64 MSVC and Linux X11 source-build
launchers are supplied, with native runtime validation still pending. A source
build recipe is not a platform-support claim. See [work status](WORK_STATUS.md).

## Build Foundry for your platform

Start from a trusted RAVET source checkout. Use Rust 1.88 or newer, RAVET's
minimum supported Rust version. Keep the supplied Cargo.lock and use `--locked`.

### macOS

Install the Xcode command-line tools (`xcode-select --install`) and Rust/Cargo.
The Rust frontend uses `ravet-webview::StandaloneWindow`, Cocoa and system WebKit;
the native backend targets macOS 12 or later. Build and launch from the checkout:

```sh
cargo build -p ravet-foundry --release --locked --jobs 2
./target/release/ravet-foundry
```

For development, use `cargo run -p ravet-foundry --locked`. Run the hidden native
UI fixture without opening devices:

```sh
cargo run -p ravet-foundry --locked -- --ui-test target/foundry-ui
```

Foundry explicitly sets `StandaloneOptions::io_settings = false`: the project
manager does not route audio. Generated standalone applications retain the
default title-bar Audio/MIDI popup. The build produces a development executable;
an application bundle, production signing/notarization and installer remain
future distribution work.

### Windows x64 (MSVC)

Install Rust with the `x86_64-pc-windows-msvc` toolchain, Visual Studio's Desktop
development with C++ workload, a Windows SDK, and CMake 3.22 or later. Use an x64
Developer PowerShell so Cargo can find the native linker. The GUI uses RAVET's
HWND/WebView2 backend.

Obtain and extract the official
[Microsoft.Web.WebView2 NuGet SDK](https://www.nuget.org/packages/Microsoft.Web.WebView2).
Set its directory below; it must contain `build/native/include/WebView2.h` and
`build/native/x64/WebView2Loader.dll`. Install the
[WebView2 Runtime](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)
on the machine running Foundry. The SDK headers/loader and runtime are separate
requirements; the ordinary Edge browser is not the production runtime.

```powershell
cmake -S tools/foundry -B target/foundry-windows -A x64 -DRAVET_WEBVIEW2_SDK="C:/SDKs/Microsoft.Web.WebView2"
cmake --build target/foundry-windows --config Release
& ./target/foundry-windows/Release/ravet-foundry-gui.exe
```

CMake copies the x64 loader beside the GUI executable. Keep them together.
The CLI is also built under
`target/foundry-windows/rust-target/release/ravet-foundry.exe`. Native Windows
execution has not yet been validated; ARM64 and MinGW Foundry builds are not
claimed. Build for the current machine rather than mixing Rust/CMake architectures.

### Linux (GTK3 / WebKitGTK 4.1 / X11)

Install Rust/Cargo, a C++17 compiler, CMake 3.22 or later, pkg-config, GTK3,
WebKitGTK 4.1 (2.40 or later) and JSON-GLib development files. The GUI uses RAVET's
GTK/XEmbed backend. An X11 session or XWayland display is required; native Wayland
hosting is not implemented. Package names vary by distribution. For Debian/Ubuntu:

```sh
sudo apt-get install build-essential cmake pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libjson-glib-dev
cmake -S tools/foundry -B target/foundry-linux -DCMAKE_BUILD_TYPE=Release
cmake --build target/foundry-linux --parallel 2
./target/foundry-linux/ravet-foundry-gui
```

The CLI is built under `target/foundry-linux/rust-target/release/ravet-foundry`.
The launcher code/build recipe is supplied; Linux runtime validation remains
pending. If needed for a particular driver/display, configure
`-DRAVET_WEB_LINUX_SOFTWARE_RENDERING=ON` to select the existing RAVET software
painting path. That choice also requires target-system validation.

### CLI-only builds

The project engine/CLI does not require the Windows/Linux native GUI SDKs:

```sh
cargo build -p ravet-foundry --release --locked --jobs 2
```

macOS builds also link the Rust window backend. Foundry itself does not require
a VST3, AU or AAX SDK. Requirements for building a generated consumer are separate.

## Create and configure a project

Choose **Offline**, **Application** or **Plugin**. Enter the package/product
names and a new directory outside RAVET; its parent must exist. Set the framework
checkout. **Product settings** holds vendor, version, product identity and licence.
The header embeds the existing SVG logotype in the shared theme's accent colour.
The desktop sidebar stays in place while long forms scroll.
Folder icons beside SDK, framework and project-path fields open the platform's
native directory chooser. SDK/framework/open-project picks use the selected
existing directory. The new-project icon selects a parent directory and appends
the package name; Foundry creates that new directory on **Create project**.
Typing paths remains available. Choosers start at the nearest existing directory
of the current value. Cancel leaves the field unchanged; a choice edits the form
until **Create project** or **Save & regenerate**. Paths are bounded to 4096 bytes.
macOS uses Cocoa, Windows a COM folder chooser, and Linux a GTK folder chooser;
Windows/Linux picker execution remains pending alongside their GUI validation.

Plugin projects select **Effect** or **Instrument**, and one or more of CLAP,
VST3, native AUv2, AAX and LV2. Both roles use the generic export layer. AUv3 has
no generic adapter and is unavailable; VST2 is excluded. AUv2 requires macOS;
AAX requires macOS/Windows. The custom editor opens on macOS, Windows and Linux
(Windows builds need `RAVET_WEBVIEW2_SDK`; Linux builds need WebKitGTK 4.1).
Generated DSP is a small stereo gain effect or monophonic sine instrument with
sample-timed gain/bypass and note input, intended for replacement by consumer DSP.
It is not a full synth or MPE template.

Each plug-in project contains consumer-owned `src/dsp.rs`, `src/lib.rs` with a
`PluginExport` and `export_plugin!`, and `editor/index.html`. Every format uses
that same processor and product description. There are no copied C++ wrappers or
dependencies on the 0.0.1 gain/sine/native reference crates. All consumers have
an independent `[workspace]`, `publish = false`, template licence text and a
version-one `ravet-project.toml`. Creation does not build, fetch, install or open
devices. Proprietary consumers stay outside RAVET.

**RAVET dependencies** (`framework_source`) selects local checkout paths (`path`,
the default) or Git dependencies on the release tag matching the checkout's
workspace version (`tag`). All RAVET packages use the same source. The tag must
exist and be fetchable. Templates, adapters and bundle tooling always use the
local `framework_path`; keep it at the matching release. At release time, change
`DEFAULT_FRAMEWORK_SOURCE` in `tools/foundry/src/project.rs` from `Path` to `Tag`
on one line to change both GUI and CLI defaults. The starter CLI has its own
one-line `DEFAULT_RAVET_SOURCE` in `scripts/new-project.py`. Neither default has
been switched for the unreleased 0.1.0 tag.

The offline template remains a four-frame gain fixture. The standalone template
retains the public multi-mode synth/delay source, four-tab UI, default Audio/MIDI
popup and explicit Start action. Its Rust window/device integration is currently
macOS-only; see [examples](examples.md) and [native editor](native-editor.md).

## Reopen and regenerate

Use **Project settings → Open project** with a directory or settings-file path,
then **Save & regenerate**. Foundry manages Cargo.toml, build.rs and, for generic
plug-ins, `.foundry/product.rs` (identity-derived state tag). It preserves DSP,
product/editor source, README, ignore rules and Cargo.lock. Metadata edits update
the environment read by the product; identity changes can break existing sessions.
The package name, template and effect/instrument role are immutable.

Manual edits to managed files reject before mutation. Preserve and reconcile
those edits before regeneration. Keep `.foundry/managed.json` in version control.
The exclusive `.foundry/operation.lock` serializes cooperating builds/saves; after
an interrupted operation confirm no process is active before removing a stale
lock/stage directory. Per-file replacement is atomic, the group is not crash
atomic, and external writers are not serialized.

Historical Foundry projects using copied reference wrappers are rejected with a
migration diagnostic. Create a fresh generic project and port its DSP/editor
explicitly; the generator does not silently rewrite processor bindings or migrate
old identities/session data.

## Build and find outputs

**Build & tools** offers debug/release builds, offline dependency resolution,
format selection, bounded live logs, folder reveal and an installed editor
executable. Build only trusted consumers: Cargo build scripts and product table
metadata execute consumer code. Closing Foundry cancels its direct child and
joins the worker; arbitrary descendants are outside that ownership boundary.

For plug-ins Foundry invokes Python 3.11+ and the framework's
[single bundle command](bundle-plugin.md). It checks selected SDKs before Cargo
resolution, generates a missing lockfile, then lets the bundle tool compile the
product once and build the selected adapter. Every plug-in Build produces a
bundle; `--bundle` remains an accepted compatibility flag. Keep Cargo.lock.
Outputs are `target/plugins/<debug|release>/<package>.<format>` (`component` for
AUv2, `aaxplugin` for AAX), without global installation. CMake/C++ and selected
external SDKs are required; no SDK is downloaded or copied into source.

SDK fields in Foundry are explicit overrides. Empty fields use `RAVET_*_SDK`
environment variables, project `ravet-sdks.toml`, then per-user settings as
documented in the [bundle guide](bundle-plugin.md#sdk-settings-and-precedence).
Only VST3 needs the VST3 SDK; native AUv2 needs AudioUnitSDK, AAX needs AAX SDK,
and LV2 needs headers/COPYING. Development macOS signing is ad hoc unless the
project/user settings supply a vendor signing hook. Vendor credentials and PACE
configuration remain outside RAVET. Foundry does not validate or install bundles;
use `bundle-plugin.py --validate` explicitly for format validation. Cross-builds
are available from the bundle CLI; Foundry targets the current machine.

```sh
./target/debug/ravet-foundry new ../my-effect my-effect plugin GPL-3.0-only
./target/debug/ravet-foundry new ../my-instrument my-instrument plugin GPL-3.0-only --type instrument --formats clap,vst3,au,aax,lv2
./target/debug/ravet-foundry open ../my-effect
./target/debug/ravet-foundry regenerate ../my-effect
./target/debug/ravet-foundry build ../my-effect --format vst3 --release
./target/debug/ravet-foundry edit ../my-effect code
./target/debug/ravet-foundry doctor
```

`new` accepts `--framework /absolute/ravet`, `--framework-source tag` (alias
`--ravet-source tag`), and `--vst3-sdk`, `--audio-unit-sdk`, `--aax-sdk`, `--lv2-sdk`.
`build --offline` requires a populated dependency cache. `--format` selects one
saved format, defaulting to the first. Executable names differ for platform
launchers above. Builds from missing tools/SDKs fail with logs; Foundry does not
install toolchains. A blank Windows editor requires checking the WebView2
Runtime/loader; Linux needs X11/XWayland and WebKitGTK 4.1 development files.

Validate generation in disposable external directories:

```sh
cargo test -p ravet-foundry --locked
cargo build -p ravet-foundry --locked
python3 scripts/check-foundry.py
python3 scripts/check-foundry.py --build
```

`--build` also checks the existing offline and macOS standalone fixtures. Optional
SDK flags add native bundles; `--vst3-validator` checks the VST3 copy. The old
`--auv3` fixture selection is rejected. Platform/UI, DAW, production signing and
real-time support remain qualified by [work status](WORK_STATUS.md).
See [decision 0288](decisions/0288-unified-product-bundling-and-foundry.md).
