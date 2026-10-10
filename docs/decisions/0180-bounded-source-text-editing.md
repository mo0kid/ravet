# 0180: Bounded source edit ownership and history

Status: implemented within the bounded source model; focused source/cache/overflow contracts passed.

## Decision

Own plaintext and directional grapheme selection within a UI/control-thread
buffer. Match the paragraph source/control policy and existing caret grapheme
data. Retain at most 4096 source bytes and 64 exact snapshots across undo/redo.
Use existing state-store concepts (atomic errors, net no-op preservation, bounded
snapshot history) without introducing its allocating generic tree into text or
processing dependencies. There is no new runtime dependency.

Give each constructed document a distinct process-local atomic owner identity;
require matching owner and monotonic revision for every mutation. Never wrap
owner/revision values. Cache preparation captures source/token under one immutable
borrow and stores both together. Selection changes also advance revision and
conservatively invalidate caches. Restored old text never reuses an old revision.

Validate all byte/grapheme/control/capacity/identity conditions before mutation.
One successful content replacement stores one exact prior text/selection,
branches by clearing redo and evicts oldest undo at capacity. Content no-ops
preserve selection/revision/history/redo; selection changes preserve history and
redo. Undo/redo restore exact text/affinity/directional selection and advance
revision. No typing coalescing or generic grouped transaction is inferred.

Inserted/deleted text may join neighboring graphemes. Collapse selection after
the insertion and snap forward if that byte ceases to be a resulting grapheme
boundary. This handles combining, ZWJ and CRLF joining without creating invalid
editable byte positions. Font/style coverage and shaping remain caller-owned.

## Evidence and limits

Seven source-edit integration groups and two internal exhaustion contracts pass,
alongside twelve unchanged paragraph/caret regression groups. They exercise
foreign/stale tokens and layouts, undo-restored source with stale cache identity,
complete source/selection/history failure preservation, redo/no-op/eviction,
Unicode joining/deletion and allocator/revision exhaustion without global reset.
This is a source edit model and cache identity,
rather than an input method, clipboard, collaborative document protocol, native
text editor, rich text or accessibility backend. Logical payload bounds are not
a global allocator/backend-work bound. Mutations/preparation may allocate and
deallocate, so no audio callback or deadline claim is supplied.


Sources: [edit buffer](../../crates/ravet-surface/src/editing.rs),
[tests](../../crates/ravet-surface/tests/editing.rs),
[shared source policy](../../crates/ravet-surface/src/text.rs),
[guide](../text-editing.md).
