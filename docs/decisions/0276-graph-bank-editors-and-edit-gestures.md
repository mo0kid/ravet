# 0276: Graph-bank child editors and controller edit gestures

Status: Implemented (prototype, revision R3); publicly adopted after two independent reviews.
Local macOS arm64 evidence only:

- substituted-loader native fixtures (normal and fast-math);
- Rust mock-provider tests;
- the actual RAVET Gain editor attached to a Graph-bank child in a hidden
  offscreen window that is never ordered front, with edits injected into its
  script-message handler in-process;
- ThreadSanitizer runs of the native fixtures and the existing editor window
  test (not the Graph-bank window test).

No DAW, third-party editor, single-component plug-in, accessibility or
real-time deadline claim.

## Context

Decision 0271 left two open items:

- The Rust `Editor` exclusively borrows its `Instance`, so Rust-side audio needs
  the editor closed. Hosted Graph-bank children (decision 0268) process AUDIO on
  a worker while MAIN owns control, so they could not have an editor at all.
- The component handler refused `beginEdit`, `performEdit` and `endEdit` with
  `kNotImplemented`, so edits made in a plug-in's editor never reached its
  processor or the host. `restartComponent` coalesced any defined flag without a
  policy.

The bank already separates MAIN control from AUDIO leases, uses bounded
preallocated rings between them, and reclaims on MAIN in a fixed order: leases
end before any stop or retirement. The native editor entries already take only
the module busy flag, not the live-core CONTROL admission, and the window test
of decision 0271 ran concurrent AUDIO through the raw tables.

## Decision

1. **Opt-in capture.** Add an additive exact-origin
   `RavetVst3EditGestureProviderV1` (40 bytes: `enable`, `drain`, `state`).
   Existing tables, layouts and status domains are unchanged. Only a child that
   is enabled changes behaviour. Every other handler keeps `kNotImplemented`
   for edits and its original `restartComponent` coalescing. A Graph-bank child
   opts in with `Vst3GraphBankBuilder::edit_gestures`. `build` enables it after
   preparation, before any start or lease. `enable` is CONTROL work and is
   refused while any AUDIO capability of the module is live.
2. **Native validation at the source.** `enable` snapshots the writable ids
   (not read-only, not hidden) of the refreshed catalogue. This is the same
   predicate the hosted processing boundary applies to points. Edits are
   accepted on the platform main thread only, into a fixed 256-record ring
   with a gap-free sequence:
   - an unknown id, or a non-finite or out-of-range value, returns
     `kInvalidArgument`;
   - a full ring returns `kResultFalse` and counts an overflow;
   - off-main calls and calls after child retirement return `kResultFalse`.

   The plug-in learns of every capture refusal when it happens. The handler
   calls no foreign or Rust code, so it is a safe leaf for reentry from view,
   parameter and teardown callbacks.

   **Pairing (R2).** At most 64 gestures may be open per child, and each open
   gesture reserves one ring slot.
   - The `endEdit` of an accepted `beginEdit` therefore always records, even
     when the ring is full mid-gesture.
   - A begin needs two free slots.
   - A duplicate begin of an open id, or an end without an open begin, is
     refused with `kResultFalse` and counted `unpaired`, and never recorded.
   - Published begins and ends are balanced per id, except in these cases:
     - Destroying the editor with gestures open records a synthesised `End`
       for each, flagged (R3); a late `endEdit` is then unpaired.
     - Gestures open at child retirement are not closed: the bank and its
       queues are gone.
     - A panicking `drain_edits` callback or a pump `Abi` error can drop
       records on the consumer side.
   - Otherwise the plug-in's next `endEdit` closes an open gesture, and the
     64-gesture cap bounds what is held.
   - `kNoParamId` is refused as unknown.
   - Sequence exhaustion (2^64 records, unreachable in practice) refuses
     instead of terminating.
3. **Explicit restart policy.** Once enabled, `restartComponent` accepts only
   `kParamValuesChanged`, `kLatencyChanged` and `kParamTitlesChanged`. A call
   with any other defined flag is refused whole with `kResultFalse`. Partial
   acceptance would leave the plug-in unable to tell what was honoured. Accepted
   and refused restarts are recorded for the host. A running bank child is never
   restarted or re-prepared: an accepted latency change is published as
   `rebuild_required`, and the node keeps its cached latency until rebuild.
4. **Backpressure, not loss.** `pump_edits` (MAIN) moves records out of the
   native ring only while both the AUDIO value ring and the host event queue
   have room. Both capacities are explicit and allocated at `build`: events up
   to 4096, values up to 256, which bounds the AUDIO coalescing scan at
   65,536 comparisons per block. A record is taken natively only if it can be
   both applied and published. A slow host therefore surfaces as native
   overflow refusals to the plug-in, never as silent loss. After the node has
   returned to MAIN, `pump_edits` refuses (`Abandoned`) and moves nothing.
5. **AUDIO delivery through the existing queue; the edit wins (R2).** Each
   `performEdit` value becomes a parameter point at sample offset 0 of the next
   block the node successfully dispatches. It goes into the node's prepared
   parameter queue, ahead of framework or lane points.
   - Same-id values before that block coalesce (last wins, counted); they would
     land on the same sample.
   - If the block carries its own point for the same id at offset 0, **the edit
     replaces it**. The replaced block point is counted as `overridden`; the
     block's later points for that id still apply.
   - Values beyond the native 64-id or 2048-point limit are deferred to the next
     block (counted).
   - Values leave the pending set only after a successful dispatch, so refused
     blocks and failed dispatches keep them pending.
   - AUDIO allocates, locks and frees nothing.

   The edit-wins rule intentionally differs from the framework
   `SameOffsetPolicy` and `MergePolicy`; edits are merged after them.
6. **Publication.** `drain_edits` passes `EditEvent` records (begin, perform,
   end, restart) in capture order with the native sequence. For an id routed by
   the child's framework translation, the event also carries the framework
   equivalent: `ParameterGesture` begin or end, or a physical `ParameterValue`
   through the declared scale.
7. **Thread safety stays with the plug-in.** With an open editor, MAIN calls
   the controller and view while AUDIO calls `process()` on the same child.
   VST3 permits this and puts the burden on the plug-in, most visibly for
   single-component plug-ins. RAVET guarantees lifecycle ordering, not the
   plug-in's internal thread safety. Only RAVET's split-component Gain was
   exercised.
8. **Bank child editor.** `Vst3GraphBankControl::open_editor` creates a
   `Vst3GraphEditor` through the existing native editor table while the bank's
   leases stay live. The guard borrows nothing. It marks the child (one editor
   per child) before the native call, and close or drop clears the mark only
   when the native editor was consumed. Ordering relies on state:
   - **Reclamation policy: defer.** A bank with an open editor is not released.
     No lease ends and no child stops or retires; the deferral is counted in
     `ReclaimReport::editors`. Reclamation never closes a caller's editor,
     because the caller owns the `NSView` lifetime and may still be using the
     guard.
   - `release` also refuses with `BankError::EditorOpen` as a second line.
   - Native child destruction returns `LIFECYCLE` while the view lives.

   The view is always released before the child's controller, component and
   module retire. `attach` keeps its unsafe caller-owned `NSView` obligation.
   No native pointer escapes.

## Alternatives considered

- **Block point wins at offset 0 (R1).** Rejected in R2 after review: an
  accepted edit was dropped, and steady offset-0 automation of the same id
  masked every user edit. Carrying the edit to the next block has the same
  problem under continuous automation, and placing it at offset 1 does not fit
  when the block also has a point at offset 1. Edit-wins is lossless for
  accepted edits; the cost, one replaced block point per edit, is counted.
- **Synthesise a missing end when the ring is full.** Rejected: a synthesised
  end would itself need a slot, which is the same as reserving one. Reserving
  is simpler and exact.
- **Close editors during reclamation.** Rejected: the guard would dangle, or
  need shared mutable state that reports "closed behind your back". The caller
  also owns the window lifetime. Deferral is explicit and observable.
- **Validate ids in Rust after capture.** Rejected: the plug-in would already
  have received `kResultOk` for an edit that is then refused.
- **Drop the oldest or newest edit on overflow.** Rejected by requirement:
  overflow is refused at the source and counted.
- **Sample-positioned edits by arrival time.** Rejected: MAIN and AUDIO have no
  shared clock, and the VST3 handler carries no timestamp. Offset 0 of the next
  dispatched block is the earliest exact position and is deterministic.
- **Accept unsupported restart flags as notifications.** Rejected: the bank
  cannot honour I/O, routing or reload changes on a running child.

## Consequences

Graph-bank children can show their editor while processing, and the edits
made there reach the processor at a defined block and the host as ordered
events. Hosts must call `pump_edits` and drain events regularly. A host that
stops draining causes refusals to the plug-in rather than unbounded memory.
Scoped (non-bank) instances keep decision 0271's exclusive editor and
`kNotImplemented` edits. Published framework events are for the host's model
and must not be fed back into the same node. `kResultOk` from
`restartComponent` means "recorded", not "applied". Race freedom is a design
argument backed by one real-time window test and ThreadSanitizer runs of
RAVET's own C++, not a demonstrated absence of races. A forgotten editor guard keeps its bank retained for
the process lifetime. Evidence is local macOS arm64. The real editor exercised
is RAVET's own Gain, whose edits are injected into its script-message handler
in-process rather than produced by WebKit JavaScript or input events.
