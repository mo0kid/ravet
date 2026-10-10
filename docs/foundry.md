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

Start from a trusted RAVET source checkout. Use Rust/Cargo with edition 2024
support; the local validation compiler is rustc 1.99.0. The framework has no
established MSRV. Keep the supplied Cargo.lock and use `--locked`.

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

Plugin projects select **Effect** or **Instrument**, and one or more formats.
The effect reference offers CLAP, VST3, AUv2, AUv3, AAX and LV2. The instrument
reference offers CLAP, VST3, AUv2 and LV2; instrument AAX/AUv3 adapters are not
provided. The custom effect editor is macOS-only for CLAP/VST3/AUv2; instruments
use host controls. AUv3 uses its own parameter view. VST2 is excluded.

| Export | SDKs | Starter target restrictions |
|---|---|---|
| CLAP | Cargo dependencies | Native Rust build |
| VST3 | External VST3 SDK | macOS/Windows/Linux build recipe; platform execution evidence below |
| AUv2 | External VST3 SDK and AudioUnitSDK | macOS with Xcode |
| AUv3 | System Apple SDK | macOS effect extension/container |
| AAX | External VST3 SDK and AAX SDK | macOS/Windows effect; development packaging only |
| LV2 | External LV2 headers and COPYING | Native C++17/Rust build recipe |

Native adapters require CMake 3.25 or later and a C++ compiler matching the Rust
architecture. Enter absolute SDK source directories under **SDKs**. SDKs are
not downloaded or bundled with generated source. Missing SDKs reject the build
before Cargo dependency resolution. Format selection does not enable another
operating system: builds target the current machine. Platform matrices, module
selection (including MPE), cross-compilation and universal binaries remain plans.

All consumers have their own `[workspace]`, `publish = false`, local framework
paths, a readable version-one `ravet-project.toml`, editable source and template
licence text. Creation performs no build, network, installation or device action.
Consumer/package names, template and effect/instrument role stay fixed after creation.

| Template | Starting point and limits |
|---|---|
| Offline | Four-frame stereo gain/timed-event fixture; no window/device |
| Standalone | Consumer-owned public multi-mode synth/delay reference UI, DSP and explicit audio/MIDI routing; macOS Rust window only |
| Plugin effect | Consumer-owned CLAP wrapper/editor and native format adapters referencing public gain processing |
| Plugin instrument | Consumer-owned sine CLAP reference and VST3/AUv2/LV2 performance-sine adapters; these references expose different performance contracts, not a promised MPE-ready product |

The standalone template deliberately retains the public reference's complete
source, including its four-tab UI and hidden offline UI/audio fixture. Replace
or simplify its DSP and editor as your application develops. It uses the default
Audio/MIDI popup and starts devices only through an explicit Start action. See
[examples](examples.md) and [native editor](native-editor.md) for routing limits.
Foundry disables building this generated standalone template on other platforms
until its Rust window/device integration is available; this does not prevent
building Foundry's own native launcher there.

Use **Project settings → Open project** with a directory or settings-file path.
Edit product settings and select **Save & regenerate**. Foundry regenerates only
`Cargo.toml`, `build.rs`, native CMake configuration and `.foundry/` product
headers/plists/LV2 metadata, plus its settings/ownership records; it never rewrites
DSP, editor source, README, ignore rules or Cargo.lock. Local dependency paths can
be updated through the framework field after moving the checkout.

Manual edits to any managed build file reject regeneration before any file is
changed. Preserve your changes, restore the last Foundry version of that file,
then regenerate. Native C++/Objective-C++ adapters are copied once, even for
unselected formats; export selections can change without replacing edited source.
Replace the CLAP processor binding and native ABI binding when changing reference
DSP. Foundry does not infer a shared processor from arbitrary consumer code.
Template customization/dependency selection in the GUI is future work. Hand-editing settings is supported, but unknown fields/versions reject and
TOML comments/formatting are not preserved when saving. Retain `.foundry/managed.json`
in version control. Regeneration uses an exclusive `.foundry/operation.lock`;
after an interrupted process, confirm no Foundry build/save is active before
removing a stale lock or `.foundry/stage` directory. Per-file replacement is
atomic, but the group is not crash-atomic; disk errors attempt rollback and require
inspection. This does not protect against concurrent non-Foundry writers.

## Build and find outputs

**Build & tools** provides a toolchain check, debug/release builds, optional offline
dependency resolution, selected-format bundling, live bounded output, **Open folder** and **Open in editor**. Enter an installed
editor executable (for example `code`) or its absolute path, without arguments.
Builds run on one worker while native events continue. Closing Foundry requests
build cancellation and waits for the direct child process/worker; arbitrary
descendants created by consumer build scripts are outside that ownership boundary.
Initial dependency resolution can use Cargo's network unless **Offline** is selected.
Build only trusted consumers: Cargo can execute their build scripts.

Build first applies settings, then creates Cargo.lock if absent and uses
`cargo build --locked --jobs 2` with the consumer's `target/` directory. Keep the
lockfile with the consumer. Select a plug-in format and **Build**. CLAP writes
`target/clap/debug/<package>.clap` or the release equivalent; macOS receives a
bundle/Info.plist with development ad hoc signing, Windows/Linux a copied dynamic
library. Other exports are packaged under `target/plugins/<debug|release>/` as
`.vst3`, `.component`, `.app` with embedded `.appex`, `.aaxplugin`, or `.lv2`.
AUv2 embeds the generated VST3 and builds AudioUnitSDK locally. SDK/framework
notices are included in native packages. macOS native bundles are ad hoc signed.
AU metadata uses deterministic product/vendor four-character codes and a packed
numeric version (major <=65535, minor/patch <=255); VST3/AAX class IDs derive from
the saved product identifier and remain stable across title/vendor/version edits.
Changing product identity changes native IDs and may break existing sessions.
The displayed output is never installed into a DAW or global plug-in
directory. No production signing or compatibility claim follows from generation.

The CLI supports the same operations:

```sh
./target/release/ravet-foundry new ../my-effect my-effect plugin GPL-3.0-only
./target/release/ravet-foundry open ../my-effect
./target/release/ravet-foundry regenerate ../my-effect
./target/release/ravet-foundry build ../my-effect --release --bundle
./target/release/ravet-foundry edit ../my-effect code
./target/release/ravet-foundry doctor
./target/release/ravet-foundry new ../my-instrument my-instrument plugin GPL-3.0-only --type instrument --formats clap,vst3,au,lv2
./target/release/ravet-foundry build ../my-instrument --format vst3 --release
```

Add `--framework /absolute/path/to/ravet` to `new` when the compiled-in checkout
path differs. Add `--offline` to `build` only when its dependency cache is ready.
For native SDK paths, `new` also accepts `--vst3-sdk`, `--audio-unit-sdk`,
`--aax-sdk` and `--lv2-sdk`; later changes use settings and regeneration.
CLI `--bundle` packages CLAP; native builds always produce their local bundle.
`--format` chooses one configured format (default: first saved format).
Executable names/locations differ for the Windows/Linux CMake launchers above.

## Troubleshooting and remaining scope

- **Cargo missing/linker failure:** put Rust and the platform compiler on PATH;
  on Windows use x64 Developer PowerShell and matching MSVC Rust.
- **Windows blank/editor creation failure:** check the installed WebView2 Runtime
  and loader beside the executable, not just the SDK headers.
- **Linux display failure:** use X11/XWayland and check DISPLAY; install the 4.1
  development packages rather than a different WebKitGTK API version.
- **Offline resolution failure:** populate the Cargo cache with an ordinary build
  before selecting Offline. Foundry does not install toolchains or SDKs.
- **Regeneration conflict:** preserve and reconcile the named managed-file edit;
  DSP/editor changes do not cause this conflict.

Next work includes component/dependency/MPE selection, platform matrices,
additional instrument adapters, richer source/resource navigation and file
selection, application packaging, and target-platform UI validation. VST2 is
excluded. Proprietary consumers remain separate private workspaces.
Validate the generator in disposable external workspaces:

```sh
cargo test -p ravet-foundry --locked
python3 scripts/check-foundry.py
python3 scripts/check-foundry.py --build
# Optional native builds/probes with externally installed SDKs:
python3 scripts/check-foundry.py --build --vst3-sdk /path/to/vst3 --audio-unit-sdk /path/to/AudioUnitSDK --aax-sdk /path/to/AAX --lv2-sdk /path/to/lv2 --auv3 --vst3-validator /path/to/validator
```

Build the debug Foundry executable first. `--build` adds offline consumer builds,
the offline runnable fixture, effect/instrument CLAP packages and the macOS
standalone hidden UI/audio fixture.
It opens no devices and installs no plug-in. Use `--executable /path/to/ravet-foundry`
for a separate Cargo target directory.

See [decision 0194](decisions/0194-foundry-project-management.md).
