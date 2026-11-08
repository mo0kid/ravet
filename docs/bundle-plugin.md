# Build and bundle a plug-in product

`python3 scripts/bundle-plugin.py` builds any trusted Cargo product which declares
one `cdylib` target and calls `ravet_plugin::export_plugin!` with `clap` and `native`
features. It uses that product for every selected format. No reference-product
allow-list or product-specific C++ wrapper is needed. See [plug-in export](plugin-export.md)
for the product contract and [Foundry](foundry.md) for project creation.

From a RAVET checkout, with Python 3.11+, Cargo/Rust, CMake 3.22+ and a C++17
compiler (individual SDKs may require newer tools):

```sh
python3 scripts/bundle-plugin.py --package ravet-delay-plugin \
  --formats clap,vst3,au,aax,lv2 --release --validate

# Independent product workspace; generate and retain its Cargo.lock first.
python3 scripts/bundle-plugin.py --package my-product \
  --manifest-path ../my-product/Cargo.toml --formats clap,vst3,lv2
```

The command builds only the named package. Debug is the default; `--release`
selects release. One `cargo rustc --locked --jobs 2` invocation per Rust target emits a cdylib and,
when AAX is selected, a staticlib in the same compilation. A small executable
loads the trusted product table for its name, version and identity; running a
build therefore executes trusted consumer code. The adapters receive these
already built libraries and reuse their existing importable Python builders.
No SDK is downloaded or redistributed. There is no global plug-in installation
except the explicitly requested temporary AU validation below.

Outputs share `<cargo-target>/plugins/<debug|release>/`, or
`<cargo-target>/plugins/windows/<debug|release>/` for a cross-build. An explicit
macOS `--arch` selects `<cargo-target>/plugins/macos-<arch>/<debug|release>/`;
omitting it preserves the native output path above. Cargo's
`CARGO_TARGET_DIR` is respected. Filenames use the Cargo package name, with
`.clap`, `.vst3`, `.component`, `.aaxplugin` and `.lv2` suffixes; display names and
format identities come from the product. `--output-dir /absolute/directory`
selects another destination. An existing selected bundle is replaced; unselected
formats are left alone. Intermediate builds remain in Cargo's target directory.
A failed command exits nonzero and may leave earlier formats from that run.

Other options:

- `--formats clap,vst3,au,aax,lv2`: ordered, distinct selections; default `clap`.
- `--arch arm64|x86_64|universal`: macOS only, incompatible with `--target windows`.
  Defaults to the running host architecture (including an x86_64 Python under
  Rosetta). Rust builds use `aarch64-apple-darwin` and/or `x86_64-apple-darwin`;
  install missing standard-library targets with `rustup target add`. Universal
  combines both cdylibs and, for AAX, staticlibs with `lipo`. CMake receives
  `CMAKE_OSX_ARCHITECTURES=arm64;x86_64` for universal adapters, external SDK
  libraries and helper tools. Build caches are separate under
  `<cargo-target>/plugin-build/macos-<arch>/`. The optional pkg-config lilv host
  check is disabled for universal and non-host slices, because host libraries
  may lack the other slice; LV2 adapter contracts and Turtle validation remain.
- `--offline`: Cargo uses only cached dependencies. Keep the product lockfile.
- `--check`: check platform, format and SDK selection without building or resolving
  Cargo dependencies. Foundry uses this before generating a missing lockfile.
- `--settings /path/ravet-sdks.toml`: select a project settings file explicitly.
- `--vst3-sdk`, `--audio-unit-sdk`, `--aax-sdk`, `--lv2-sdk`: explicit SDK paths.
- `--webview2-sdk` (also `RAVET_WEBVIEW2_SDK` or `[sdks].webview2`): the
  extracted `Microsoft.Web.WebView2` NuGet package, passed to Windows product
  builds (`--target windows` or native Windows) for products with a web editor.
  MSVC builds link its static loader, so no `WebView2Loader.dll` is bundled.
- `--target windows`: cross-build x86_64 MSVC from Linux/macOS with cargo-xwin,
  clang-cl, lld-link and Wine. Rust and C++ link the static CRT. Wine runs the
  target metadata reader and LV2 Turtle generator. Native Windows builds omit
  this flag and use the MSVC toolchain. AU requires native macOS; AAX requires
  macOS or Windows. See [cross-build decisions](decisions/0285-linux-windows-plugin-builds.md).

The existing per-format scripts and their explicit SDK options still work for
format-focused development and historical reference fixtures. The unified
command always selects the generic adapters.

## SDK settings and precedence

Settings contain paths, never SDK source or signing credentials:

```toml
[sdks]
vst3 = "/SDKs/vst3"
audio_unit = "/SDKs/AudioUnitSDK"
aax = "/SDKs/AAX"
lv2 = "/SDKs/lv2"
webview2 = "/SDKs/Microsoft.Web.WebView2"
```

`scripts/fetch-audio-unit-sdk.py <directory>` fetches the checksum-pinned
Apple AudioUnitSDK 1.4.0 for `audio_unit` (see [Audio Unit](audio-unit.md)), and
`scripts/fetch-vst3-sdk.py <directory>` the checksum-pinned Steinberg VST 3 SDK
3.8.1 for `vst3` (see [VST3](vst3.md)). The AAX SDK is distributed by Avid to
signed-in developers only, so it has no fetcher; download it from Avid. Use
SDKs from their vendors, not copies embedded in other frameworks.

Use a project `ravet-sdks.toml` beside the product's Cargo.toml or in an ancestor
(the nearest file wins), or the per-user file:

| Platform | User settings |
|---|---|
| macOS | `~/Library/Application Support/ravet/ravet-sdks.toml` |
| Linux | `~/.config/ravet/ravet-sdks.toml` |
| Windows | `%APPDATA%/ravet/ravet-sdks.toml` |

`$XDG_CONFIG_HOME/ravet/ravet-sdks.toml` overrides the platform config location
when set. Project paths override user paths individually; omitted keys inherit.
Relative settings paths resolve against the file's directory; `~` expands to the
user's home. Explicit `--settings` replaces automatic project-file selection.
Unknown settings keys and malformed paths/commands reject.

For each SDK, precedence is **explicit flag → environment variable → project
file → user file**. Environment variables are `RAVET_VST3_SDK`,
`RAVET_AUDIO_UNIT_SDK`, `RAVET_AAX_SDK`, and `RAVET_LV2_SDK`. Prefer absolute
paths for flags and environment variables. No sibling checkout, package-manager
prefix or developer-specific path is guessed. Missing SDK errors name the key,
environment variable or missing file. Only selected formats need SDKs: AU and
AAX do not need VST3. LV2 accepts official `include/lv2` or flat `lv2` headers,
including COPYING. Format SDK licence notices are added to the bundles.

Foundry's saved SDK fields are passed as explicit flags, so they override the
same shared discovery layers. Empty fields allow environment/project/user
settings to supply a path. Keep machine-specific SDK settings out of public
consumer source control when appropriate.

## Signing

On native macOS, the default signs nested Mach-O images first, then the enclosing
bundle, using `codesign --force --sign -`. LV2 signs its individual images. This
is development ad hoc signing; production distribution remains vendor-owned.
The AU builder also signs its staging dylib before loading it for metadata.

A product can replace final signing with a command argument array:

```toml
[signing]
command = ["/absolute/vendor-tools/sign-plugin", "--profile", "my-product"]
```

Or pass `--sign-command '["/absolute/vendor-tools/sign-plugin","--profile","my-product"]'`.
The CLI command overrides settings. RAVET appends the absolute bundle path and
sets `RAVET_BUNDLE_FORMAT` and `RAVET_BUNDLE_PATH`; it runs the command once per
final assembled bundle from the product directory, without a shell. The vendor
script owns nested signing, Developer ID identities, notarization, PACE wrapping
and any required credentials/configuration. Store secrets with that vendor tool,
not in RAVET or a command line. Hook failures stop the command. Hooks also run on
Windows/Linux when supplied; those platforms have no default signing command.
Validation follows signing on the final copied bundle. RAVET never configures
production credentials or infers shipping readiness from a successful hook.

## Validation

`--validate` runs adapter contracts where available and validates the final signed
bundles. A missing required validator or failed check is an error:

| Format | Validation |
|---|---|
| CLAP | `clap-validator validate`, from `--clap-validator`, PATH or RAVET `target/tools` |
| VST3 | Steinberg validator, built from the selected external SDK |
| AUv2 | Native component contracts; `auval -v` with the table's identity |
| AAX | RAVET synthetic-host bundle check; no Avid/PACE validator |
| LV2 | CTest contracts/product smoke/lilv when available, then `lv2_validate` from PATH |

On macOS, validation explicitly runs the requested architecture; universal runs
both arm64 and x86_64 validators. `clap-validator` must contain those slices.
`lv2_validate` checks architecture-independent Turtle on the build host; the
final LV2 module contracts execute separately in each requested slice.

For x86_64 AU validation, the tool ad hoc signs a **private build-directory copy**
of Apple's `/usr/bin/auvaltool`, then loads RAVET's registration helper into that
process. The helper registers the exact final signed component's factory and
original identity with `AudioComponentRegister`. This avoids installed-product
identity collisions and Apple silicon's omission of Intel-only components from
system registration. No component is installed and no Apple binary is shipped.
Only trusted product libraries should be validated, as with metadata loading.

The arm64 AU path retains system `auval -v`: Apple's arm64e validator cannot
load an arm64 registration constructor. It temporarily copies the component to
`~/Library/Audio/Plug-Ins/Components/ravet-validation-<package>.component`, refuses
to overwrite an existing path and removes its copy in `finally`. An installed
product with the same identity can affect **arm64** registrar lookup; use a clean
user environment to qualify that lookup. Exact-bundle native contracts run in
both slices regardless. Killing an arm64 validation process without cleanup may
require removing its temporary component. No devices or DAWs are opened.

Cross-build validation uses Wine for CLAP/VST3/AAX contracts; supply a Windows
`clap-validator.exe` with `--clap-validator`, PATH or `target/tools`. LV2 validates
Turtle on the build host. Missing target validators reject validation. A build/validator result is
limited to the recorded machine and format; it does not establish DAW, editor,
production signing or real-time support. The unified command's current executed
evidence is in [work status](WORK_STATUS.md).

## Licence notices

Every bundle the tool assembles carries RAVET's licence, the product's
third-party notices (`THIRD-PARTY-NOTICES.txt`) and the Rust standard library
notice, written before signing: inside `Contents/Resources/notices` for CLAP on
macOS, VST3, AUv2 and AAX, at the root of an LV2 bundle, and in a
`<package> notices` folder beside a single-file `.clap` on Windows and Linux,
which installers must ship with it. See
[dependency notices](dependency-notices.md).

For `ravet-synth-plugin`, VST3 `--validate` additionally builds and runs the
real-product instrument adapter fixture (CMake
`RAVET_VST3_INSTRUMENT_TESTS=ON`), before validating the final module. It checks
hidden MIDI controller assignments, note IDs/output offsets, exact audio/tail
and latency notification. See the [instrument guide](synth-plugin.md). AU
validation retries only temporary registrar discovery failures, then removes
its exclusive temporary component installation on either success or failure.

## Recorded macOS architecture run

On 5 November 2026, the Delay built in all five formats as thin x86_64 and
universal bundles on one Apple M4 Pro/macOS 27.0.1. Every shipped image contained
the selected slices. x86_64 validator execution and REAPER 7.82 checks used
**Rosetta 2 on Apple silicon**, not native Intel hardware. CLAP/VST3/LV2 REAPER
renders passed; AU discovery/edits/recall passed but its strict render timing
failed. AAX has synthetic checker evidence only for this slice. See the
[receipt](WORK_STATUS.md#release-010-macos-architectures),
[support matrix](support-matrix.md#macos-x86_64-under-rosetta-2) and
[harness instructions](host-validation.md#macos-rosetta-2).
