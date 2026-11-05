# 0288: Unified product bundling and generic Foundry projects

**Date:** 5 November 2026  
**Status:** Implemented prototype; platform evidence is recorded in work status.

## Decision

Complete the build-tool delivery step of [0281](0281-generic-export-layer.md)
with one Python entry point, `scripts/bundle-plugin.py`, and importable format
builders. Cargo compiles the selected product once per Rust target, emitting both
cdylib and staticlib when AAX is requested. AAX links the staticlib; the other
adapters use the same build's cdylib. Adapter builders accept prebuilt library,
target directory and profile inputs while preserving their direct CLI behavior.
A target-native metadata executable reads the trusted v1 product table; Windows
cross-builds run it under Wine, avoiding a second host Rust product build.

SDK discovery is explicit flags, then environment, nearest project settings,
then user settings. Files merge per key and relative paths belong to their file.
No SDK path is inferred from another product. A vendor signing command is an
argument array, receives one final bundle path plus format/path environment
variables, and replaces default final ad hoc macOS signing. Vendor configuration,
identities and credentials remain outside RAVET. Validators check signed copies;
AU validation alone installs a temporary component with exclusive destination
ownership and cleanup. Missing validators reject explicit validation requests.

Foundry generates one consumer-owned Processor and PluginExport product, with
identity metadata managed separately from DSP/editor source, and builds through
this command. Effect and monophonic sine instrument templates use the same five
generic formats. AUv3 has no generic adapter and is rejected rather than silently
retaining a reference wrapper. Historical Foundry wrapper projects need explicit
DSP/editor migration into a new project; regeneration never rewrites their DSP
bindings or translates old session identities implicitly.

Local-path dependencies stay the default between releases. One
`DEFAULT_FRAMEWORK_SOURCE` constant supplies both CLI and GUI defaults; setting
it to `Tag` is the release-time change. Existing `framework_source = "tag"` and
CLI tag overrides still use the checkout version's tag. Tooling/adapters require
a matching local framework checkout even when Cargo uses tag dependencies.

## Consequences and limits

Products need one cdylib target and a retained Cargo.lock. Native metadata reading
adds CMake/C++ to CLAP-only builds; all cross metadata reading needs Wine. Build
outputs replace only selected bundle paths and are not transactional as a group.
The tool does not create installers, configure production signing, infer host
support, or change the release tag/default now. Per-format reference fixtures
remain available for historical tests. See the [bundle guide](../bundle-plugin.md)
for exact flags, settings and validation limits, and [work status](../WORK_STATUS.md)
for executed evidence rather than prospective platform claims.

## macOS architecture selection — 5 November 2026

`--arch arm64|x86_64|universal` selects explicit macOS Rust targets and passes
`CMAKE_OSX_ARCHITECTURES` to every adapter, SDK library and helper. Universal
Rust cdylibs and AAX staticlibs are combined by `lipo` before adaptation; no
second product implementation is built. Target-specific dependency notices
cover the union of both target graphs. Architecture caches and explicitly
selected output directories are isolated; omitted `--arch` retains the earlier
native output path while selecting the running host architecture.

Validators execute each requested slice explicitly. The optional Homebrew lilv
host test is disabled for universal/non-host builds because it can resolve a
thin host library; LV2's own actual-module checks still execute both slices.
Intel AU validation uses a RAVET constructor to register the exact signed
component in a private ad-hoc-signed copy of Apple's auvaltool, avoiding global
registration and installed-identity collisions. No Apple executable is shipped.
The arm64 system validator is arm64e on this OS and cannot load an arm64
constructor; it retains exclusive temporary installation and the documented
same-identity registrar caveat. Exact in-process component contracts still
check the selected artifact in both slices.

The [receipt](../WORK_STATUS.md#release-010-macos-architectures) records all
five-format universal/thin-Intel builds and Rosetta execution. Rosetta evidence
is distinct from native Intel hardware support. Production signing and Avid
validation remain vendor-owned.
