# Working in RAVET

Read [docs/README.md](docs/README.md), [docs/FEATURES.md](docs/FEATURES.md), and
[CONTRIBUTING.md](CONTRIBUTING.md) before making project changes.

## Documentation is part of implementation

- Update affected guides and Rustdoc in the same change as behavior, public APIs,
  defaults, limits, commands, dependencies, or platform support.
- Add or update a record in [docs/features.toml](docs/features.toml) for every
  capability change. Preserve stable feature IDs. Include source and test
  references for implemented features and prototypes; state their limits.
- Run `python3 scripts/check-docs.py --write` after changing the feature registry
  or Cargo package metadata. Do not manually edit generated `docs/FEATURES.md`.
- Run `python3 scripts/check-docs.py` before finishing. For API comments or
  examples, also run `cargo test --workspace --doc --locked` and
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`.
- Record significant contract or backend decisions under `docs/decisions/` and
  link them from its index. Keep implementation status separate from plans.
- Update `docs/WORK_STATUS.md` for completed work packages or new validation
  evidence. Preserve earlier entries there as historical observations.

Follow [docs/maintenance.md](docs/maintenance.md) for ownership and review rules.
Do not claim a platform, DAW, device, or real-time deadline is supported from
offline tests alone. VST2 is excluded from the project, rather than deferred.

## Proprietary consumers stay outside this repository

Keep proprietary product code, assets, session/licence fixtures, and product
wrappers in separate private sibling projects. Do not add them as workspace
members or publish them with RAVET. Only reusable framework components belong
here. Review both tracked files and Git history before publishing a public
remote; deleting a proprietary file does not remove it from existing commits.

Keep external framework comparison notes and feature-parity plans in private
sibling notes outside this repository. Describe RAVET's own capabilities and
limits directly in public documentation.

## Standalone UI defaults

Standalone RAVET UI projects use the standard title-bar Audio/MIDI settings popup
from `ravet-webview::StandaloneWindow` by default. Omit it only through an explicit
`StandaloneOptions::io_settings = false` configuration. The popup includes output
and available input channel pairs, sample rate, buffer size and available MIDI
inputs. Embedded plug-in editors leave device routing to their host; headless
fixtures do not create windows. See [docs/native-editor.md](docs/native-editor.md).
