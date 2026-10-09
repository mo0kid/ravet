# 0022 — External AUv2 wrapper for shared reference DSP

Status: accepted for the reference prototype, 9 October 2026.

The native ABI and VST3 reference already share Rust gain DSP. Build the official
external Steinberg AU wrapper against the external Apple AudioUnitSDK and embed
the VST3 bundle as its resource. No SDK sources or proprietary consumer code are
copied into RAVET. Build helpers avoid normal plug-in installation directories.

Keep the official wrapper's normalized AU parameter API and its host-owned bypass
semantics. Preset recall restores gain while preserving current AU bypass;
complete VST3/CLAP gain/bypass recall remains a separate contract. This behavior
is verified through actual AudioComponent rendering and state APIs, rather than
inferred from successful linking.

The local component is ad hoc signed for development. Actual DAW discovery,
auval, native allocation/deadline evidence, custom editor, AUv3, sandbox runtime
and other targets remain unvalidated. This choice does not complete those work
packages. See [the guide](../audio-unit.md) for reproducible build instructions.


The later VST3 suspended-edit controller record is disabled when the SDK host
exposes `IVst3ToAUWrapper`. AU parameter calls update a separately owned snapshot
and can leave controller values stale until UI timers run. Restoring those
controller values as authoritative would overwrite valid component state.
The existing component-based AU recall and preserved host bypass remain the
contract; actual-bundle rendering/recall tests catch regressions in this branch.
