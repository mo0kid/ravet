# 0185: Prepared AUv2 native planar bus mapping

Status: prototype; actual packaged component, native mapping/binding fixture and Gain regressions passed locally.

AU AudioBufferLists contain sample pointers inside AudioBuffer records, while
the native processor ABI needs contiguous pointer arrays. A stopped-configured
immutable mapper and caller-owned fixed mapped block make that conversion explicit
without allocating during prepared mapping. Copies retarget native views to
their own storage. The public performance component consumes this mapper before
transferring parameters or invoking its existing native-backed processor.

Logical native main/auxiliary order maps independently to compact AU input and
output element order. Zero-channel directions consume no AU element. Consumer
component registration and host routing must establish those counts explicitly;
the helper never guesses that a bus-index array is compact. The supported profile
is f32 planar, at most nine bus pairs and 64 channels per direction, within 65,536
prepared frames. It does not create, prepare or own native DSP.

Complete list/shape/byte-capacity/alignment/address-overflow and mutually disjoint
output preflight precedes any published pointer update. Input/output aliases
remain legal. Failed configuration and late mapping rejection preserve previous
valid profiles/maps; mapping never writes sample data. Zero frames permit either
exact planar metadata with no audio-range requirement or explicitly omitted audio
with both supplied directional counts zero. The native channel counts remain
declared, with null sample arrays. Bounded pointer storage/sort avoid heap, locks
and CF calls. Live foreign memory ownership remains the consumer's obligation.

Fault silence stays at the existing AU caller render boundary because AUBase can
reject before virtual DSP or skip copying on failure. The mapper does not claim
automatic fault silence, host notifications, format negotiation or routing.
The instrument's owned caller selector preflights raw prepared outputs before
AUBase; alias rejection preserves live voice phase/decay and accepted queued
notes, verified against the complete oscillator/envelope reference. Two thousand maximum
accepted/late-fault maps use zero counted C++ new/delete actions; that counter
does not prove all SDK/backend/Rust heap behavior.
Synthetic main/input-only/output-only/mixed ordering and maximum/late-buffer
regressions are supplemented by actual native Gain/Performance bindings and the
packaged component probe. These local macOS arm64 checks establish no generic AU
host auxiliary routing, f64 AU audio, DAW/auval, devices, sandbox, all backend
allocation behavior, callback deadlines or production distribution support.
