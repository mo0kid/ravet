# Changelog

User-facing changes since the [0.0.1 source preview](docs/releases/0.0.1.md),
grouped in the style of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
See the [0.1.0 release notes](docs/releases/0.1.0-notes.md) and the
[release criteria](docs/releases/0.1.0.md).

## 0.1.0 — 2026-11-05

### Changed

- **Compatibility boundary:** the new export layer uses keyed or explicit
  permanent parameter IDs and the versioned `RAVETSTA` state envelope.
  Products moving from the 0.0.1 references must preserve identities/IDs and
  supply state imports explicitly; migration is not automatic. The forward
  compatibility policy starts at 0.1.0. See [upgrade notes](docs/releases/0.1.0-notes.md#upgrading-from-001)
  and [compatibility](docs/compatibility.md).
- The umbrella `ravet` dependency exposes every component except
  `ravet-embedded` through Cargo features. Internal library dependencies now
  have version requirements; the minimum Rust version is 1.88.
  See [getting started](docs/getting-started.md).
- `ravet-data` cryptography is opt-in through `crypto` and `entitlement`;
  XML-only use avoids those dependencies. See [control data](docs/control-data.md).
- [Starters](docs/project-starters.md) and [Foundry](docs/foundry.md) can select
  tagged RAVET sources; local paths remain the default before release.
- RAVET accepts no outside pull requests or code contributions. Bug reports,
  questions and host test results are welcome as issues. See
  [contribution policy](CONTRIBUTING.md).

### Added

- A generic [plug-in export layer](docs/plugin-export.md): one product
  description supplies identities, parameters, state migration/import hooks,
  automation and bypass policies, and a web editor (macOS, Windows and Linux;
  Windows/Linux evidence is REAPER-only). Native table
  adapters export [VST3](docs/vst3.md), [AUv2](docs/audio-unit.md),
  [AAX](docs/aax.md) and [LV2](docs/lv2.md), alongside generic CLAP.
- A [tempo-synced Delay example](docs/examples.md#tempo-synced-delay-plug-in)
  with ping-pong, feedback tone, parameter smoothing, typed editor values and
  eight explicit parameter IDs.
- [Linux builds and Windows cross-builds](docs/plugin-export.md#linux-and-windows-builds)
  for plug-in bundles, and a disposable
  [REAPER delay harness](docs/host-validation.md#scripted-ravet-delay-check-any-format-macoslinuxwindows)
  for parameter readback, saved-session recall and checked renders.
- [Editor assets](docs/plugin-export.md#editor-assets): products embed images,
  fonts, scripts and styles served to the editor from a local origin, beyond
  the 1 MiB page and 64 KiB message limits.
- A [REAPER editor harness](docs/support-matrix.md#editor-runs-on-windows-and-linux)
  and an X11 probe for Windows/Linux editor runs.
- A [support matrix](docs/support-matrix.md) separating named host runs from
  validators, and [hosted CI configuration](docs/runtime-validation.md)
  for portable crates and the Delay CLAP bundle on macOS and Windows.

### Removed

- The unused contributor terms, replaced by the no-outside-code policy in
  [CONTRIBUTING.md](CONTRIBUTING.md). No contribution was accepted under them.

### Fixed

- Unitless toggles now display On/Off; fractional parameter text and typed
  Delay values avoid rounding a dragged value to a misleading integer.
  See [parameter text](docs/plugin-export.md#clap-export).
- The generic VST3 adapter negotiates product preparation limits before
  accepting setup, avoids hidden MIDI-controller ID collisions, and includes
  rebuilt product libraries in incremental bundles. See [VST3](docs/vst3.md).
- Platform packaging fixes cover VST3 runtime/output paths, LV2 target-side
  Turtle generation and AAX cross-link arguments. The AAX test host's oversized
  block buffer overrun was corrected. See the
  [recorded builds](docs/WORK_STATUS.md#release-010-platform-builds).

All new format integrations remain prototypes within the matrix's recorded
scope. CI configuration does not establish a passing hosted run or DAW support.
