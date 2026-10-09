# Getting started with RAVET

Run the portable gain example first, then build the CLAP prototype to exercise
the same processor through a plug-in boundary. Neither command opens an audio
device or installs a plug-in into a host's search path.

For small synth, effect, MPE and MIDI 2.0 walkthroughs, see
[Small examples](examples.md). The example commands open live native UIs on macOS; explicit flags retain offline fixtures. The guide
also links to the existing CLAP exports.

## Requirements

- Rust and Cargo supporting edition 2024 and Cargo resolver 3. The observed
  development toolchain is 1.99.0; a minimum supported Rust version is not set.
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
