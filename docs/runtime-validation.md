# Runtime contract automation

## Hosted portable release checks

The [portable release workflow](../.github/workflows/portable-release.yml)
configures GitHub-hosted `macos-latest` and `windows-latest` jobs on **every
push to `main`**, on `pull_request`, and on manual dispatch. It requires no
secrets or external plug-in SDKs. Checkout does not persist credentials; token
permissions are `contents: read`. Fork PRs stay on hosted runners and still
require the repository's maintainer-approval policy in
[maintenance](maintenance.md). Outside code is not accepted.

Each matrix job uses stable Rust, Python 3.11 and two Cargo workers, with a
40-minute timeout and independent failures. It:

1. Builds and tests default features of `ravet-foundation`, `ravet-embedded`,
   `ravet-signal`, `ravet-state`, `ravet-data`, `ravet-graph`, `ravet-instrument`,
   `ravet-audio-file`, `ravet-osc`, `ravet-surface`, `ravet-native`,
   `ravet-native-dsp` and `ravet-clap`, with all targets and the lockfile.
2. Tests optional `ravet-data` crypto/entitlement and the native DSP processor
   guard with all their features. Other portable optional backends are outside
   this focused job; this is not an all-features workspace check.
3. Builds/tests `ravet-plugin` with `clap` and `native` (all its current features)
   and tests `ravet-delay-plugin`. The macOS GUI dependency uses the runner's
   system Xcode SDK; VST3, AUv2 and AAX format SDKs are not downloaded or built.
4. Builds the actual release Delay CLAP bundle and runs
   [clap-validator 0.4.1](https://github.com/free-audio/clap-validator/releases/tag/0.4.1)
   against it. The [fetch helper](../scripts/fetch-clap-validator.py) pins the
   universal macOS and Windows release archive URLs and SHA-256 digests,
   checks the complete download before unpacking and copies only the named
   executable. Hashes were compared with GitHub's release-asset digests on
   5 November 2026. Upstream's universal ZIP contains a nested tarball labelled
   0.3.2; the outer pinned asset is 0.4.1 (`152b982`).

The helper's [regression tests](../scripts/tests/test_clap_validator_download.py)
check both archive layouts, corruption refusal before writes and isolation from
archive-controlled paths. Tests use synthetic payloads; downloaded release
archive checks are recorded separately in [work status](WORK_STATUS.md#release-010-docs-ci).
No format SDK is needed for this job, and LV2 headers are not fetched. The
workflow neither installs bundles into host folders nor publishes artifacts.

**The hosted jobs have not run for this change.** Local YAML/download checks
and constituent macOS commands do not prove that either hosted job passes.
Native Windows packaging is a new check, distinct from the recorded Linux-to-
Windows cross-builds. A future CI result still would not establish DAW,
device, editor interaction, signing or deadline support.

## Broader workspace and native fixtures

The existing [runtime workflow](../.github/workflows/runtime.yml) retains a
self-hosted Linux workspace job, skipped on pull requests. Its macOS/Windows
workspace jobs and macOS CLAP reference fixtures currently run on manual
dispatch or pull requests; they are not the new every-main-push checks. The
workspace jobs cover formatting, all targets/features, Clippy, Rustdoc tests
and warning-free API documentation. Linux uses the runner's preinstalled ALSA,
pkg-config and toolchains.

The separate macOS fixture builds development-signed Gain and Sine CLAP bundles
and exercises their ABI, plus a guarded native editor lifecycle fixture.
Licensed format wrappers, DAW runs, physical audio/MIDI, native accessibility
and deadline measurements require their own resources and receipts.

Configuration is source, not executed-platform evidence. See the
[support matrix](support-matrix.md) for dated host/validator scope and
[work status](WORK_STATUS.md) for historical attempts and exact environments.
