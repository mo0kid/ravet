# 0176: Explicit public VST3 build profiles

Status: implemented; complete Performance and default Gain bundle pipelines passed locally.

The public VST3 packaging entry point previously selected Gain only, while the
performance instrument required a manual CMake build. Make selection explicit
with `--plugin gain|performance`, keeping Gain as the default and preserving its
existing profile paths, editor/program options and validation order.

The focused `scripts/vst3_bundle.py` module owns immutable profile/context data,
argument-list command execution, archive/target construction, selected-bundle
notice copying/local signing and dynamic validation. The CLI owns argument
parsing and invokes prepare/build/package/verify. Compiler jobs remain two;
external SDKs remain caller-supplied. No automatic host installation, SDK download,
commercial signing, release upload or target-platform support is introduced.

Performance has a separate build root, selects its own module/contract targets,
and filters CTest to its instrument fixture. The shared Gain CMake configuration
still requires the public Gain archive, but the instrument links only Performance.
Gain-only editor/program switches reject before mutation. Configuration failures,
missing selected artifacts and validator failures propagate; stale unrelated
bundles are not signed or loaded. Three boundary tests cover profile separation,
pre-mutation rejection, notice/sign scope and error propagation.

The performance processing and host-evidence contract remains
[decision0166](0166-vst3-performance-instrument.md). Native Windows/Linux execution,
production signatures and further host validation require independent evidence.

The complete Performance profile executed on macOS27 ARM64 with external SDK
3.8.1. Its native archive contract passes and the dynamically loaded packaged
module passes 47 SDK tests with 0 failures. The original default Gain pipeline
passes all 12 existing CTest contracts and 47 dynamic SDK tests with 0 failures. Local test launching uses a temporary
checksum-identical executable copy for the known workspace-volume dyld delay;
no source/security/signing policy changes follow.
