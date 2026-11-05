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
selects release. One `cargo rustc --locked --jobs 2` invocation emits a cdylib and,
when AAX is selected, a staticlib in the same compilation. A small executable
loads the trusted product table for its name, version and identity; running a
build therefore executes trusted consumer code. The adapters receive these
already built libraries and reuse their existing importable Python builders.
No SDK is downloaded or redistributed. There is no global plug-in installation
except the explicitly requested temporary AU validation below.

Outputs share `<cargo-target>/plugins/<debug|release>/`, or
`<cargo-target>/plugins/windows/<debug|release>/` for a cross-build. Cargo's
`CARGO_TARGET_DIR` is respected. Filenames use the Cargo package name, with
`.clap`, `.vst3`, `.component`, `.aaxplugin` and `.lv2` suffixes; display names and
format identities come from the product. `--output-dir /absolute/directory`
selects another destination. An existing selected bundle is replaced; unselected
formats are left alone. Intermediate builds remain in Cargo's target directory.
A failed command exits nonzero and may leave earlier formats from that run.

Other options:

- `--formats clap,vst3,au,aax,lv2`: ordered, distinct selections; default `clap`.
- `--offline`: Cargo uses only cached dependencies. Keep the product lockfile.
- `--check`: check platform, format and SDK selection without building or resolving
  Cargo dependencies. Foundry uses this before generating a missing lockfile.
- `--settings /path/ravet-sdks.toml`: select a project settings file explicitly.
- `--vst3-sdk`, `--audio-unit-sdk`, `--aax-sdk`, `--lv2-sdk`: explicit SDK paths.
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
```

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

AU validation temporarily copies the component into
`~/Library/Audio/Plug-Ins/Components/ravet-validation-<package>.component`, refuses
to overwrite an existing path and removes its copy in a `finally` cleanup.
An existing host installation with the same product identity can affect auval's
registration lookup; validate in a clean user environment. Build without
`--validate` to avoid installation. No devices or DAWs are opened by the tool.
A process killed without cleanup may require removing its temporary component.

Cross-build validation uses Wine for CLAP/VST3/AAX contracts; supply a Windows
`clap-validator.exe` with `--clap-validator`, PATH or `target/tools`. LV2 validates
Turtle on the build host. Missing target validators reject validation. A build/validator result is
limited to the recorded machine and format; it does not establish DAW, editor,
production signing or real-time support. The unified command's current executed
evidence is in [work status](WORK_STATUS.md).
