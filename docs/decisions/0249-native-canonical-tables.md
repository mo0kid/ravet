# ADR 0249: Canonical runtime native companion tables

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

## Context

Foreign format adapters require exact originating native table identities, with
separate owning types, sample precisions and library lifetimes. The associated
constant `NativeProcessor::ENTRY` was returned by a const-callable function;
companion tables also returned associated constant references inline. A new
nested native export family produced two different addresses for its same
processor table across export and telemetry codegen paths. Existing successful
format receipts were scoped observations of their exact archives, not proof that
constant promotion preserves address identity for every consumer/codegen layout.

## Decision

Use deliberately out-of-line, non-const runtime address-bearing acquisition for
processor, parameter, cached parameter, audio-state, complete-state and telemetry
v1 tables. Keep table values private and immutable; route every foreign export and
transitive companion through the canonical function. Companion metadata preflight
and original null/error behavior remain unchanged. No runtime interning registry,
private layout cast, copied proprietary code or relaxed structural equivalence is
introduced.

`NativeProcessor::entry` loses const-callability. Inventory of existing repository
call sites found no const usage; a compile-fail Rustdoc example makes the new
contract explicit. This source API change does not change the C v1 layout/version,
DSP behavior or CONTROL acquisition/library-retention obligations.

## Validation and limits

The pre-fix actual C++ receipt recorded different exported/telemetry core pointers
for the new native family. Corrected release archives pass normal/fast tests of
all six table chains in both precisions, including that formerly failing consumer.
An independent fixture consumes existing Gain, Performance, BusRouting,
NoteRouting and Delay8/16 exports, compares repeated exported/transitive pointers,
and runs actual prepared32/64 processors and audio-state/telemetry snapshots.
Focused native Rust tests and the const API Rustdoc check accompany the patch.

The guarantee is acquisition identity for a concrete family/precision in its loaded
library. It does not allow matching foreign types from table bytes, unloading a
live library, sharing DSP owners unsafely, or treating offline evidence as host or
real-time support. Affected public format references must be rebuilt/requalified
after adoption; prior receipts are preserved without being retroactively erased.
