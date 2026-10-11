# 0254: Raw short-MIDI AAX effects, auxiliary buses and one AUDIO admission

Status: Implemented; publicly adopted with the W234/W241/W249 AAX chain. Local
macOS arm64 package and workspace evidence only.

## Decision

Add opted-in raw effect/multibus contexts and result-returning prepared raw layout
entry paths while preserving all default context fields/policies. Factor one
lock-free AUDIO admission token across opted-in raw, generated and timed profiles;
do not independently acquire/release per-feature flags. Busy precedes foreign
getters and any active outer callback mutation.

Keep ordinary raw scalar/complete-state publication on the supported SPSC CONTROL
producer/AUDIO consumer route. Stopped lifecycle/timing/telemetry still requires
stop/join and serialized CONTROL; reading the processing bit is not atomic
mutual exclusion. Full packet/effective-control/directional metadata/audio preflight
precedes state/dequeue/history. Protected node base and stream/packet extents
remain known public spans with explicit caller liveness/capacity obligations.

Compose existing native routing and fixed-delay kernels into distinct public
RawBusRouting/RawDelayRouting consumers with complete two-global state and exact
selected 32/64 companion provenance. Require reusable SDK registration and actual
initialized callback evidence for main, input-only/output-only auxiliary source
orders and shared raw+dynamic-timing profiles. Retain the discontinuous prepared
zero-history candidate policy and original native/host statuses.

## Evidence and limits

Fresh production CMake32/64 modules pass five maintained tests each: four
normal/fast native/registered validators and the negative profile compile gate.
The runtime validators cover actual all 123 statuses, late faults, both auxiliary
orders, valid in-place mapping, getter/DSP SPSC publication, nested admission,
Delay8->16 ACK/state adoption, backend errors and maximum callback heap counters.
Packaging notices, exports/version and strict local ad-hoc signature checks pass.
The isolated Rust suite passes 21 tests and warning-denied lint/Rustdoc; bundling
Python passes 15 tests.
The corrected raw instrument/default-note fixtures also qualify the shared helper.
Exact receipts and guards belong to W241; historical failed or stale archive
observations remain preserved separately.

A complete raw node is input-only; raw+generated output is rejected explicitly.
There is one identified input, no invented source-port provenance, no SysEx stream
or MIDI2, and no automatic MIDI-to-parameter policy. Existing strict reference
policies/default ABI remain. Internal64 output conversion occurs after native DSP
and cannot promise history rollback. Local external-SDK tests do not establish
Pro Tools/ACF host routing, signing, other platforms, AudioSuite, arbitrary editor
support or real-time deadlines. SDK code stays external.

See [the guide](../aax-raw-routing.md) and
[the reference](../../examples/raw-routing-aax/README.md).
