# Contributing to RAVET

RAVET is an early source preview with provisional APIs. Start with the
[documentation index](docs/README.md), [architecture](docs/architecture.md), and
[feature catalogue](docs/FEATURES.md) before changing a contract or integration.
See [LICENSING.md](LICENSING.md) for contribution and dependency requirements;
commercial contributor terms are still pending.

Proprietary consumer products must remain in separate private sibling projects.
Do not add their implementations, assets, session/licence fixtures, or wrappers
to this repository or workspace. Contribute reusable framework components here;
validate product-specific integrations in the private consumer project. Review
both tracked files and Git history before public publication.

External framework comparison notes and feature-parity plans belong in private
sibling notes outside this repository. Public documentation should describe
RAVET's own capabilities and limits directly.

## Make a change

Keep portable DSP independent of host, device, and editor backends. Preparation
may allocate; processing and reset must avoid allocation, deallocation, blocking,
and I/O. Preserve validation before mutation and silence on faults. Add meaningful
tests when behavior or contracts change, and record significant design choices
in [decision records](docs/decisions/README.md).

Update documentation in the same change as features, public APIs, defaults,
limits, build commands, dependencies, or supported platforms. Edit Rustdoc beside
the API, edit [features.toml](docs/features.toml) for capability changes, and update
the relevant guide. Regenerate the catalogue after changing the registry or a
Cargo package manifest:

```sh
python3 scripts/check-docs.py --write
```

Do not edit generated `docs/FEATURES.md` directly. A refactor with unchanged
behavior still requires review of source and test references; explain in the
change description when no documentation update is needed.

## Validate a change

```sh
python3 scripts/check-docs.py
python3 -m unittest discover -s scripts/tests -v
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Documentation-only changes need the documentation checker; run Rustdoc tests and
builds when API comments or examples change. Run the checker tests when changing
the checker or registry format. The complete Cargo test suite includes Rustdoc
examples. CLAP changes also need the bundle, smoke-host, and validator checks
described in [Getting started](docs/getting-started.md).

Describe the resulting behavior, validation performed, and material limits in
each change. Record platform or host evidence with the actual environment in
[work status](docs/WORK_STATUS.md); test results from one host do not establish
general compatibility.

For live guide/API previews and strict static-site builds, use the [documentation portal workflow](docs/documentation-site.md).
