# 0247: Complete short MIDI AAX input and shared precision synthesis

Status: Implemented; publicly adopted with the W234/W241/W249 AAX chain. Local
macOS arm64 package and workspace evidence only.

Keep note-only defaults and unchanged instrument Contexts. Add an explicit raw
short-MIDI policy preserving every defined complete1..3-byte channel/system message
as canonical kind14 native events. Standalone F7 has only canonical EndOfExclusive
meaning; no SysEx inference. Validate the whole stream and combined effective
trailing-control capacity before native state/transport/dequeue/history. Raw AUDIO
admission is a lock-free CAS with nested Busy before getters/mutation; stopped
CONTROL snapshots/lifecycle require joined audio. Validate the public live
`AAX_IMIDINode` base range/alignment/owner nonalias before any getter, then protect
that known span against audio metadata/sample writes and fault silence. No private
host-node layout or hidden extent is inferred.

The raw owner explicitly accepts fully preflighted zero-frame offset0 events and queued controls without phase/sample advancement. The existing note/effect default keeps its empty-only zero-frame restriction. Actual component fixtures prove zero-frame controller/note-on/off and known Full program request changes, untouched audio/tails, and late malformed/combined-capacity refusal before state/dequeue/history.

Use an additive `SineInstrument64` facade around the shared existing render kernel,
retaining original `SineInstrument::new` inference and f32 samples. Expose explicit
constructor program policy and CONTROL Level schema/setter/request observations.
The raw reference disables automatic program Level replacement while retaining
receiver request/voice behavior. Persist only acknowledged Level with distinct
RAVETMS/version1 state; raw telemetry and voices remain transient.

Optional telemetry composition requires complete metadata and exact selected
core/sample origin before any cache/state acquisition. Stopped snapshots detect an already entered callback. All snapshot/lifecycle
operations still require AUDIO stopped and joined; those checks are not atomic
mutual exclusion. Raw SPSC scalar/restore requests remain accepted during AUDIO. This depends on separate ADR0249 canonical runtime addresses and
ADR0250 exact client admission/balanced CONTROL acquisition. Internal64 depends
on ADR0240 and keeps its nonrollback output-narrowing contract. Overlapping AAX
owner/model/script files are explicit additive changes after those baselines.

A distinct AAX-only MidiSine reference registers owned mono/stereo factories from
exact public effect IDs. The external wrapper module infrastructure needs an
empty authored Steinberg factory; real bundle/SDK startup, descriptor identity,
zero unrelated class count and initialized raw Process demonstrate it. No SDK
source is copied, no stock VST3 DSP selected and no proprietary product included.

Isolated production modules32/64 each pass6validators: normal/fast exhaustive short
matrix plus telemetry acquisition preflight, actual registered mono/stereo
component lifecycle/render contracts, and maintained raw/generated compile refusal.
The older task harness has separate native32-only5-validator evidence. All123 defined statuses reach canonical
Rust32/64. Independent analytic onset/bend/pedal checks, request/Level coherence,
maximum packet/no ordinary C++ allocation loops, combined1024+pending-control fault
and raw/transport/metadata-alias phase oracles pass. Rust shared f32 equality and
actual f64 above-f32 output pass. Historical package notices/local signature/export-version and focused Rust/Python
receipts remain in the original frozen packet. The corrected revision adds strict
local signature verification and positive live/concurrent SPSC control evidence.

Historical test corrections are preserved: positive bend divides by8191 rather
than8192; the analytic oracle was corrected, not the processor. Earlier source
compilation missed a Performance fixture header include. Canonical table mismatch
was a real implementation defect, split to ADR0249 rather than weakened identity.

No generic effect/aux raw MIDI, SysEx/UMP, host program menu, Pro Tools/PACE,
AudioSuite/editor, other-platform or deadline claim follows. Public qualification
followed coherent reviewed adoption of the W234/W241/W249 chain.



The raw profile is explicitly input-only: combining RawShortMidi with generated
output is a compile-time rejection. LastNativeProcessStatus retains the last
actually returned native AUDIO code in both precisions. Local Preflight, Busy or
Exception does not replace it; rawDiagnosticsStopped exposes attempted/returned,
state/process codes and command/sample transfer flags after stop/join. A native
refusal and violating throw poison readiness and preserve independently safe
output tails; no DSP/history rollback is implied.

## Review revision3

Admitted Context-level raw refusals now replace stale prior success diagnostics,
including selected64 precision flags, without replacing the last native return.
The numeric status starts at1 and retains its prior value across lifecycle calls;
there is no lifetime has-returned bit. Metadata alias tests explicitly align the
node span. Distinct Level.3 restoration and native telemetry make CONTROL
consumption distinguishable from ignored queue data. A coherent facade pauses
inside actual DSP for following-block source-order/audio checks, followed by1000
concurrent publications and C++ ThreadSanitizer qualification (Rust uninstrumented).

The locally supplied SDK's timestamp/compatibility documentation supports retaining
authoritative ordered timestamps while accepting boolean immediate1. Negative
compilation is now a maintained normal/fast gate. Production CMake explicitly runs
the registered component with its module path, separate from frozen task harness
receipts. Production Clang21 Release static model/owner/collector frame estimates sum to
172976/173024 bytes for native32/64 raw+transport. These are partial compiler
estimates, not runtime stack measurements or total upper bounds; opaque helper/
native/host frames and small-stack portability remain unqualified.
Behavior dependencies and actual harness/runner/manifests/lock are frozen; root
must integrate registry/index/status/generated docs and complete fresh public
reference/joint gates before acceptance. Prior frozen revisions and receipts stay
historical and unchanged.


## Review revision4

The locally licensed SDK defines its boolean as uint8_t, so the conditional signed
flag concern does not occur here. Nevertheless, exact0/1 admission and80/FF tests
make the contract independent of that signedness assumption. Ordered timestamp
policy, including rejection of a late timestamp0 after a nonzero event, remains
explicit; no real-host packet ordering is inferred.

The getter-blocked concurrent test now proves native Level.3 before publication and
Level.6 after consumption, with exact64-frame progression and independent voice
audio. Early refusal snapshots compare all native telemetry; fake aligned node
aliases preserve exact known metadata bytes without claiming an observable virtual
getter on those fake addresses. Collector null/misaligned/header-packet-output
aliases remain whole-output/count atomic. A fresh initialized owner explicitly
asserts the numeric1 never-returned sentinel.

The selected first-party build tree, actual compiler depfile include paths, Rust
workspace/lock/archive build argv/toolchain, rebuilt external SDK library hashes
and exact runners are qualified together. C++ TSan records compiler flags, imported
sanitizer symbols, dynamic runtime linkage and captured stderr/exit; the Rust
archive and separately registered module remain uninstrumented. The production
registration translation unit supplies raw+transport compiler stack estimates.
The local promoter's ordinary exception rollback is tested with failure at its
second replacement for existing/new files, bytes/mode and created-directory
cleanup. No public apply, crash atomicity, host validation or isolation proof is
claimed. Prior frozen revisions/failed tooling preparations remain historical.
