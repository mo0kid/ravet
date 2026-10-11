# 0273: Framework events in hosted VST3 Graph nodes

Status: Implemented (prototype); publicly adopted after independent review. Local
macOS arm64 mock-provider and actual RAVET Gain/Performance module evidence only.

## Context

Owned hosted VST3 Graph nodes (decision 0268) refused any block that carried
ordinary framework events. Only explicit per-node rich lanes could deliver
parameters, notes, note expression or block context. Graph consumers therefore
could not drive a hosted VST3 node with the same events and transport that
drive CLAP and native nodes. VST3 needs normalized values, int32 note IDs, an
event bus, expression type IDs and context flags. None of these can be inferred
safely from RAVET's portable events.

## Decision

1. Translation is opt-in per child. The bank builder's `translate(child,
   FrameworkTranslation)` checks the declaration's shape and capacity. `build`
   then checks parameter routes and the note bus against the refreshed
   catalogue, after every child is prepared and before any child starts or any
   lease begins. A refusal unwinds like any other build failure. A child without
   a translation keeps the 0268 refusal of framework events.
2. Parameters use declared routes only: RAVET `ParameterId` to VST3 `ParamID`,
   with an explicit `Normalized` or declared `Linear { minimum, maximum }`
   scale. IDs are never assumed equal. A route must name an existing writable
   parameter (not read-only, not hidden). Each value becomes one normalized point
   at its exact sample offset in that ID's `IParameterChanges` queue. Points are
   never sorted or merged. A repeated native ID at the same offset follows
   `SameOffsetPolicy`:
   - `Refuse`, the default, refuses the block.
   - `LastWins` replaces the earlier value and counts it as ignored.

   Unmapped IDs follow an explicit `Refuse` or `Ignore` policy. Point admission
   is bounded per point: the conflict check scans only the trailing
   equal-offset run (at most 64 points), and the distinct-ID set (at most 64)
   is kept incrementally.
3. Notes need a `NoteRoute` on a selected event input bus. Plain on/off notes
   carry `noteId == -1` under `Unidentified` and refuse under
   `RequireIdentified`. `NoteWithId` keeps IDs 0..=`i32::MAX`. Channels must be
   within the bus. Velocity is copied unchanged. Tuning, length,
   `ppqPosition` and flags are zero: nothing is inferred from transport.
4. Note expressions other than pressure become `NoteExpressionValueEvent`
   when declared. They are addressed by note ID only, using SDK normalized
   units: volume divided by 4, tuning divided by 240 plus 0.5, others
   unchanged. Pressure becomes `PolyPressureEvent` when declared. It needs a
   concrete channel and key; an optional ID becomes `noteId`, otherwise -1.
   These are the inverses of RAVET's plug-in-side VST3 input mapping.
5. With transport declared, block-start `Transport` becomes a
   `ProcessContext`. All-absent transport passes no context. Otherwise
   `playing` and `sample_position` must both be known, because the SDK has no
   availability flag for either. Flags are set exactly for the fields present.
   Fields with no SDK representation (discontinuity, pre-roll, tempo increment,
   bar number, sample loops) are validated but not represented.
6. Other kinds (MIDI, UMP, modulation, gestures, scoped values and release,
   choke, completion) and undeclared capabilities follow an `unsupported`
   `Refuse` or `Ignore` policy. Ignored events are counted on MAIN.
7. In queued rich-lane mode, events and points from both sources follow a
   declared `MergePolicy`. `Refuse` refuses blocks where both carry input.
   `FrameworkFirst` and `LaneFirst` merge by offset, with the declared source
   first at equal offsets. Two contexts in one block always refuse.
8. All storage is bounded (at most 2048 routes, 2048 events and 2048 points per
   block; merged storage within the same limits) and allocated on MAIN. It is
   counted in the bank's 64 MiB lane budget. AUDIO translation allocates,
   locks and frees nothing, and has no panic path. Every refusal, including
   capacity overflow, happens before the plug-in runs.
9. A refusal silences the block and the node returns the existing
   `ProcessStatus::Silence`, not `ProcessStatus::Fault`. Neither the node nor
   an enclosing `ravet_graph::Graph` latches, so the next valid block runs. The
   reason is a host-side `framework::Refusal`, reported in three places:
   - a per-node `refused` counter;
   - a bounded ring of `REFUSAL_RECORDS` (16) unread records, drained on MAIN
     with `drain_refusals`; further refusals are counted but their details
     are dropped;
   - in queued mode, the matching lane batch, returned to MAIN as
     `Error::Framework(Refusal)`.

   The `FRAMEWORK_REFUSED` and `MIXED_REFUSED` diagnostic bits are also set.
   Each case has a distinct variant, including the 64-ID native limit, merge
   policy, two contexts, merge conflicts and merged capacity.

### Why `Silence` rather than `Fault` (revision R4)

The first revision returned `Fault`. `Graph::process` latches any node fault
until reset, and hosted nodes answer checked reset with `RebuildRequired`. One
refused event inside a Graph therefore forced a full bank rebuild, which
contradicted the claim that a refusal does not fault the node.

Three options were considered:

- **A new Graph status for refused-but-healthy blocks.** Rejected: it would
  change Graph semantics for every node type, to serve one adapter.
- **Keep `Fault` and document the latch.** Rejected: it would make every
  `Refuse` policy a fatal, one-shot failure inside Graphs.
- **Use the existing non-latching `Silence` status.** Chosen. The Graph
  already clears that node's outputs and continues. RAVET already uses
  `Silence` for a healthy block with no usable data: buffered and graph
  device outputs return it for an underrun, which is counted on a side
  channel rather than faulted. A translation refusal matches that pattern: no
  foreign call was made, no history advanced, and the outputs really are
  silent.

The trade-off is that the Graph's own status does not carry the refusal: a
Graph with a refusing node still reports `GraphStatus::Continue`. Callers
that must react to refusals read the counter, the ring or the queued lane
result on MAIN. The Foundation `ProcessError` vocabulary could not express
these cases without widening a cross-crate enum; the host-side `Refusal` can.
No Graph code changed. Native faults, admission refusals and the refusal of
framework events by a node without a translation still return `Fault` and
latch, as in 0268.

## Consequences

Graph consumers can drive hosted VST3 nodes with ordinary events through
reviewed, explicit mappings. A refusing policy costs one silent block, not a
Graph rebuild. Graph consumers must poll the refusal counter or ring if they
need to observe refusals. Routes are checked against a catalogue snapshot
taken at `build`; a later plug-in restart that changes parameter flags is not
revalidated. The translator is stateless: a note-off without an earlier note-on
is passed through, because the plug-in owns voice state. Plug-in-defined nonlinear parameter mappings need
a declared normalized route, and the plug-in still owns stepped quantization.
Supported expression types are declared, not queried from
`INoteExpressionController`. MIDI mapping (`IMidiMapping`), output event
translation, note-ID synthesis and `ppqPosition` derivation remain outside
this decision. Evidence is local macOS arm64 only. No DAW, third-party
plug-in, foreign-heap or deadline claim follows.
