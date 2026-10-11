# 0274: RAVET reference VST3 modules for hosted-VST3 tests

Status: prototype implemented; local macOS arm64 evidence from the SDK
validator, a direct SDK robustness harness, the native provider and the Rust
Graph bank only.

## Context

The hosted-VST3 stack ([0268](0268-hosted-vst3-graph-banks.md)) had actual-module
evidence only for the public Gain and Performance references. Those modules have
no auxiliary bus and consume only notes and parameters. Auxiliary-bus profiles
and the nine SDK input event kinds carried by the rich wire were tested only
against mocks and hand-written native fixtures. A recording of a third-party
plug-in would need licensing and could not prove exact sample positions or
payloads.

## Decision

1. Build two RAVET-owned reference modules from portable Foundation processors in
   a new workspace crate, `ravet-vst3-reference-native`, with a small C ABI. The
   external-SDK wrappers live in `examples/vst3-reference` and follow the public
   Gain VST3 CMake structure. The SDK stays external.
2. **Keyed Sidechain**: stereo main in/out and one mono or stereo auxiliary
   input. Output is `main * key`, where the key is exactly 1 when the auxiliary
   bus is inactive. Other auxiliary arrangements are refused.
3. **Event Probe**: a stereo instrument that renders each of the nine SDK input
   event kinds, and each `Marker` parameter point, as an impulse at its exact
   sample offset. The left channel carries the kind code and the right channel
   a 20-bit FNV-1a hash of a documented canonical serialization. It echoes each
   input event to its output, emits one count CC, and reports a running digest
   through a read-only parameter. Zero-frame blocks still fold the digest.
   Records sharing an offset are summed, so decoding needs known candidates or
   distinct offsets.
4. Admission is explicit and fails closed. The probe admits at most 2048
   events and `4096 - events` Marker points, with fixed payload storage.
   Events on other buses and unknown kinds are ignored before any other
   check. Every other malformed input refuses the whole block silently and
   leaves the digest unchanged. The single documented exception: the digest
   advances in the render, so a host refusing the output posts sees
   `kResultFalse` after the digest moved. The sidechain reads silence-flagged
   inputs as exact zeros and refuses activation before initialize.
5. Bundles are ad-hoc signed inside the build tree and are never installed or
   linked into plug-in folders. The plug-in link and user-path SDK options are
   pinned off. CTest checks the signature, proves there is no install path, and
   runs the SDK validator.
6. Actual-module tests run through the native provider (normal and
   fast-math-harness profiles), through a direct SDK robustness harness, and
   through the Rust `vst3-hosted-audio` Graph bank. Together they cover:
   - native32 and native64;
   - active and inactive auxiliary buses;
   - every event kind;
   - variable and zero-frame blocks;
   - every documented refusal and ignore rule.

   They compare against independent closed forms, an independent hash
   implementation and twin instances. The C ABI has its own direct tests for
   every entry point. Module paths and the Rust staticlib are explicit CMake
   inputs. Without them no actual-module tests are registered, and
   configuration says so.
7. Allocation claims match measurements. A test-only `allocation-probe`
   feature installs a counting global allocator in the module staticlib.
   Native tests read it around every process call, valid or refused, and
   require zero Rust heap operations inside the loaded module. They also count
   C++ `operator new`/`delete` on the provider AUDIO worker. The ABI unit tests
   measure every process and reset call with `allocation-counter`.

## Consequences

Auxiliary-bus negotiation, inactive-bus handling, and the complete rich-event
input and output paths now have exact actual-module evidence in both hosts that
RAVET owns. The modules are test references only: they have no state, editor,
presets, bypass or latency reporting, and they are not public plug-ins.
Allocation evidence excludes direct `malloc` by the C++ wrappers or the SDK.
It is not verified whether the harness counts C++ `new` made inside a module.
No DAW, third-party host, other-platform or real-time deadline claim follows.
