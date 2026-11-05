# Getting started with RAVET

Run the portable gain example first, then build the CLAP prototype to exercise
the same processor through a plug-in boundary. Neither command opens an audio
device or installs a plug-in into a host's search path.

For small synth, effect, MPE and MIDI 2.0 walkthroughs, see
[Small examples](examples.md). The example commands open live native UIs on macOS; explicit flags retain offline fixtures. The guide
also links to the existing CLAP exports.

## Requirements

- Rust 1.88 or newer (the minimum supported Rust version, `rust-version` in
  the workspace manifest). CI checks every target and feature with 1.88.0;
  tests, lints and documentation run on the current stable toolchain.
- Python 3.11 or newer for bundling and documentation checks. These scripts
  use only the Python standard library.
- On macOS, Xcode command-line tools (`xcrun clang++` and `ar`) for native
  workspace crates and the default gain editor; `codesign` for the ad hoc bundle.

Run the commands below from the workspace root. CLAP bundling and dynamic loading
have been exercised on macOS arm64. Windows and Linux packaging paths exist,
but are not verified platform support. The project has no published packages.

## Run portable processing

```sh
cargo run -p ravet-gain --locked
```

The executable processes eight stereo frames, applies gain events at frames
zero and four, checks both channels against expected output, and prints the
result. The reusable [GainProcessor](../examples/gain/src/lib.rs) uses the
portable `Processor` trait; its Rustdoc example shows how to prepare a processor,
borrow an audio buffer, and submit events.

## Build and load the CLAP prototype

```sh
python3 scripts/bundle-clap.py
cargo run -p ravet-clap-smoke --locked -- "target/clap/release/RAVET Gain.clap"
```

The bundler prints the artifact's absolute path. If `CARGO_TARGET_DIR` is set,
use that printed path in the smoke command. Add `--profile debug` to the bundler
to build a debug artifact instead.

The RAVET smoke host checks ports, parameter metadata and text conversion,
mono/stereo automation, parameter flush, reset, repeated activation, and session
state round trips while active. It
loads native code; pass it the trusted bundle built from this checkout.

If CLAP validator is installed, validate that bundle as well:

```sh
clap-validator validate "target/clap/release/RAVET Gain.clap"
```

The default macOS build has an embedded Cocoa gain/bypass/meter editor and
committed edits and live slider host gestures. It has no device playback. Product-specific
save/restore is described in [Gain session state](session-state.md).
See the [catalogue](FEATURES.md) and
[CLAP decision](decisions/0002-clap-backend.md) for capacities and validation scope.

For actual DAW checks, follow [REAPER host validation](host-validation.md) in a
disposable session. Results remain specific to the tested host and platform.

## Exercise the native editor on macOS

```sh
cargo run -p ravet-editor-smoke --locked -- "target/clap/release/RAVET Gain.clap" target/editor.png
```

This trusted-bundle probe uses a hidden Cocoa host window, drives page controls,
checks gestures and host automation, reads processed audio meters, and destroys
editors while processing remains activated. The optional PNG captures the page.
Run separately from other WebKit startup probes. See [native editor](native-editor.md)
for ownership, build requirements and current support limits. GUI-free export
builds can use `cargo build -p ravet-gain-clap --no-default-features --locked`;
the bundler builds the default editor-enabled configuration.

## Depend on RAVET from another project

Depend on the umbrella `ravet` crate alone, and enable components with Cargo
features:

```toml
[dependencies]
ravet = { git = "https://github.com/mo0kid/ravet", tag = "v0.1.0", features = ["clap", "webview", "device"] }
```

`ravet::foundation`, `ravet::signal` and `ravet::surface` are always available.
Each other component has a feature named after its crate without the `ravet-`
prefix (`audio-file`, `clap`, `core-midi`, `data`, `device`, `graph`, `host`,
`instrument`, `media`, `metal`, `midi-device`, `native`, `native-dsp`, `osc`,
`plugin`, `preset`, `runtime`, `runtime-ffi`, `state`, `webview`). Each is re-exported as
a module, with hyphens replaced by underscores, for example `ravet::midi_device`.
Component features pass through under their own names and turn on their
component: `compressed`, `flac-encode`, `mp3-encode`, `vorbis-encode`,
`resampled-playback`, `mp3-recording`, `vorbis-recording`,
`crypto`, `entitlement`, `plugin-clap`, `plugin-native`, `convolution-transitions`, `au-hosting`, `vst3-catalogue`, `vst3-editor`,
`vst3-hosted-audio`, `system-fonts`, `postscript-fonts` and the surface image
and text features. `no_std` projects depend on `ravet-foundation` and
`ravet-embedded` directly, from the same tag.

Take every RAVET crate from a single source: one tag, or one local path.
Mixing a path with a Git source, or two revisions, builds two copies of the
framework, and their traits and types do not match. To develop against a local
checkout, replace the Git source with `path = "../ravet/crates/ravet"`. A
`[patch]` redirect is not a substitute offline: Cargo still fetches the tag
before applying it.

The `v0.1.0` tag does not exist yet; see the [release plan](releases/0.1.0.md).
Until it does, pin an exact `rev` of `main`. Released tags are never moved.
The repository is public, so Git dependencies need no credentials.

## Read and check the documentation

```sh
python3 scripts/check-docs.py
cargo test --workspace --doc --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked --open -p ravet
```

The first command checks the generated feature catalogue and repository links.
The Cargo commands test API examples and build browsable reference pages.
For the full development checks, see [Contributing](../CONTRIBUTING.md).

## Standalone output probe

The optional device adapter and silent callback probe have their own native
dependencies. See [audio devices](audio-devices.md) for selection/configuration,
commands and the current output-only limits. They are outside the portable
umbrella dependency graph.

## VST3 reference

The public gain reference also builds through an external VST3 SDK, using the
same Rust processor/state codec. [VST3 setup](vst3.md) documents its explicit
build/validator command, SDK notices and prototype limits.
