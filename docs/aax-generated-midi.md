# Native AAX generated short-MIDI output

This optional local prototype consumes canonical generated native events through
an explicit AAX LocalOutput node. A reusable fixed stage and initialized native
model produce/deliver actual Rust note-routing output. Gain and Performance's
public factories keep their existing contexts and generated capacity zero.
There is no Pro Tools/signing, arbitrary host route or audio deadline receipt.

## Profile and capacity

`AaxMidiOutput<MaximumPackets>` owns up to1024 packet slots. Whole canonical fields,
source order, offsets and supported MIDI subset validate before the first post.
Supported ordinary nonidentity NoteOn/Off and raw MIDI status are NoteOn/Off,
pitch bend, poly pressure, CC0 bank select, program change and channel pressure,
matching the external SDK's documented subset. Other CC, system, SysEx, UMP,
identity/scoped/expression/completion and parameter output rejects. A raw short
message accepted by the generic native codec may still be outside this profile.

Ordinary notes preserve channel/key and quantize exact widened-f32 velocity
nearest/ties-up. A positive On rounds to at least1; exact zero On stays zero.
Explicit NoteId0 remains an identity and rejects like every identity. The stage
never strips scope or IDs. Packet timestamps are original offsets, immediate=0
and unused bytes zero. Typed live aligned input/node capacities, exclusive AUDIO
ownership and synchronous host packet borrows remain caller obligations. The
host may copy packets during PostMIDIPacket, not retain or mutate the packet.

Nonzero appended `MaximumOutputEvents` enables the native owner and SDK model;
zero preserves old contexts/APIs/configuration. Initial generated model requires
Instrument=true/Multibus=false and selects owner0,frames1,outputs2,inputNotes3,
generatedOutput4. Register LocalInput3 and LocalOutput4 explicitly. Every native
process passes the FULL prepared generated capacity, even a zero/one-event block;
result.event_count is only the produced prefix. No output capacity reservation or
whole-batch transaction exists in the SDK. Live output node, whole map/input MIDI/
effective controls capacity precede native state apply/dequeue/process. Known
callback metadata/public node base ranges are protected; full derived node size
and role/provenance remain a host caller contract, not private-layout inspection.

## Real note-routing producer

`NoteRouting<S>` is separate from the existing BusRouting reference. It exports
paired f32/f64 processor, schema/cache/audio-state/codec/telemetry tables under
ravet_note_routing_*; default AAX selects the f32 exact originating tables.
The explicit [internal precision adapter](aax-precision.md) also exercises its
f64 origin, preserving full output capacity and generated-delivery boundaries. Input-none
main mono/stereo has silent audio, no aux/synthesizer/voices. Level1 linear0..1,
default1 is the sole complete parameter, no bypass. RAVETNTE/version1/24-byte state
stores Level separately from transient saturating processed-frame telemetry.
Prepare/reset clear history; complete parameter recall preserves it. History is
not a clock or cross-thread publication. No new member or dependency is required.

The reference supports physical Level controls and ordinary nonidentity notes;
exact {channel,key,noID} scoped key release explicitly lowers to ordinary Off.
Wildcard/ID-rich scopes and other event kinds reject before edits/output/history.
SDK NoteOn velocity0 already becomes scoped release in the existing input decoder,
so this actual route emits ordinary Off; direct portable zero On stays On. Level
scales On velocity in f32; Off release velocity is retained. Source order/offsets
stay exact; same-offset controls affect following notes. Whole input/emit-capacity
preflight precedes portable Level edits/history and generated publication.
Actual SDK input still validates/omits defined short nonnotes; no raw input MIDI
passthrough is claimed. Raw output subset is stage/canonical-codec tested only.

## Failure, delivery and lifecycle

Callback admission begins before GetNodeBuffer and lasts through the final Post.
Nested registered callback, direct owner entry, stage/delivery/invalidate attempts
reject Busy without touching active buffers/getters/DSP/diagnostics/packet bytes.
CONTROL joins AUDIO before lifecycle/state/diagnostic copies; readiness doesn't
make arbitrary concurrent control access legal. Failed stage invalidates delivery
readiness while previous bytes remain. Delivery consumes readiness before a host
call, including empty/null-node batches, and stops on first refusal or throw.
There is no stale replay or automatic retry. Earlier accepted posts are final.
Attempted/returned/posted counts and exact returned AAX_Result are retained;
throw means attempted/not-returned without an invented SDK error.
`hasStatus` means returned>0 and `status` is the last result actually returned.
After a later throw it may retain preceding success0; Exception with
attempted>returned marks the unreturned latest attempt, not that preceding status.

Native success consumes global queue intent before output delivery. A late
unrepresentable generated record posts none but cannot roll back DSP/history,
complete state or dequeued controls. A host refusal likewise cannot restore
previous history/posts. Backend error/panic remains fail-closed. Local stage/host
failure keeps native prepared for a fresh block; reset/reprepare/deactivate clear
readiness. No guessed compensation, note-off cleanup or voice lifetime policy.
Owner0/1/2 outcome, actual native status/report and stage/SDK diagnostics are
separate. Native status is known for the block only if nativeReturned is true;
SDK status only if delivery.hasStatus. Latest atomic native status can be older
than a rejected preflight. Copy non-atomic diagnostics only after stop/join.
Smaller explicit generated capacities may lead to native failure after complete
state apply; owner does not infer one-output-per-input for arbitrary processors.
The actual reference profile uses1024 input/output capacity so accepted notes fit.

## Local validation

Target strict C++17 normal/fast-math passes8/8: stage, actual registered mono/stereo
SDK Initialize→Rust producer→Post and default Gain/Performance regressions. Real
output bytes/order/zero/tiny/half velocity, exact key release, full capacity with
zero/one/1024 records, retained complete/trailing Level/history on preflight,
combined input capacity, late valid-native scoped output rejection after native0,
SDK refusal/throw and no stale replay pass. Getter and Post both reenter the actual
registered callback and prove no nested getter/DSP/silence. Authoritative Rust
accepts the exact scoped record rejected by format staging. Repeated1000 max
cycles per profile advance exact Rust history and allocate zero C++ heap; that
counter does not instrument Rust or prove deadlines. Rust passes10 tests including
five unchanged bus-routing tests and five note/codec/state/precision/preflight
checks; release archive, warning-denied Clippy/Rustdoc pass. The promoted public
sources pass normal local bundle validation: native Gain30/30,
Performance4/4 and stock14/14, each with an actual loaded-module contract check.
Public Rust10 tests/doctests, warning-denied Clippy/Rustdoc, Python8 and strict
documentation checks pass. A new combined workspace acceptance gate is pending;
these focused receipts do not replace it. SDKs stay external.

Generated callback admission uses one lock-free atomic compare-and-exchange with no
spin. CONTROL setter/restore availability reads this atomic flag; it reads no
AUDIO-owned packet storage, stage readiness or diagnostics. If availability observes a held AUDIO admission, control setters/restores reject
before SDK intent or queue mutation. After release they can publish through the
ordinary SPSC ring. This availability observation is not a general CONTROL/AUDIO
mutex: a control operation that began before admission may complete through the
ring while AUDIO runs. Caller serialization still supplies one CONTROL producer and one AUDIO
consumer; stop/join remains mandatory for lifecycle and diagnostic copies.
