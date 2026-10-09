# 0058: Compact directional CLAP auxiliary buses

## Context

Portable processors already carry ordered auxiliary buses, while native CLAP
main-bus mapping could not represent sidechains or extra output buses. Pairing
same-numbered native inputs/outputs conflates different portable buses when a
bus has channels in only one direction.

## Decision

Add a separate `MultiBusBridge` with independently compact input/output port
indexes in main-then-auxiliary order. Preserve the existing main-bus and narrow
gain APIs. Copy every input into prepared native scratch and validate every
output before DSP; copy output scratch only after successful processing. Prepare
matching borrowed auxiliary views on the stack, including zero-frame flush.
Bound combined sample/event storage to 64 MiB and reuse existing timed MIDI/UMP,
transport continuity and generated-event delivery contracts.

## Consequences and scope

Exporters explicitly advertise matching native ports, precision and MIDI
dialects. No implicit empty ports, precision conversion or sidechain negotiation
is added. Invalid whole input rejects before DSP, while processor faults can
follow processor progression and silence all host outputs. The scratch budget
excludes processor-owned memory and container metadata. Multiple legal maximum
shapes can exceed the combined budget and reject during preparation.

Native Clack buffer fixtures verify compact instrument-sidechain/auxiliary
mapping, f64 sample preservation, invalid timestamp/precision/layout rejection,
processor fault output discard and callback heap behavior. These tests establish
adapter behavior; exported plug-in/DAW auxiliary support needs separate evidence.
