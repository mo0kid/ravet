# 0109: Native f64 hosted main audio graph nodes

Status: implemented fixed main-audio profile.

Preserve the f32 factory and introduce a distinct f64 factory/node. Share generic
private lifecycle/control/ownership/error code, while prepared native scratch and
Clack port pointers remain typed at the selected precision. Every mapped f64 port
must advertise SUPPORTS_64BITS; reject before activation and retain the instance
otherwise. No implicit conversion or inferred native support. Existing Gain CLAP
stays f32; a separate public memoryless stereo x2 fixture advertises and renders
both precisions for linked/actual-binary validation.

The same one-shot lifecycle, main activation/deactivation, stopped token return,
audio-owner guards, clocks, reset, telemetry and runtime failure contract apply.
Main-only zero/one ports, 64 channels and 65536 frames bound f64 scratch to 64 MiB
outside the graph cap, versus 32 MiB for f32. No events/transport/auxiliary/editor/
active parameter mapping or general compatibility/device/deadline claim.

Tests preserve wide finite values and sub-f32 increments through typed ABI buffers
and graph routes, assert unsupported-precision rejection, owner/retirement and
heap behavior. Loaded public fixture passes both formats and existing own/upstream
gain binaries retain f32 behavior. Actual-binary evidence is macOS arm64 only.
