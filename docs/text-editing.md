# Bounded source text editing

The optional surface `text` feature provides `editing::TextEditBuffer` for UI or
control threads. It owns plaintext, directional anchor/focus selection and exact
snapshot undo/redo. It allocates and deallocates; never use it in audio callbacks.
It supplies editing state and prepared layout identity, rather than native input,
IME composition, clipboard, rich text, accessibility or a finished text editor.

Input and complete source are limited to 4096 UTF8 bytes. Construction accepts a
history limit from 0 to 64; total snapshots across undo and redo stay within that
limit. LF, CRLF, CR and Unicode line/paragraph separators retain their original
bytes. Other controls (including Tab) and noncharacters reject, matching paragraph
layout. There is no normalization, newline rewriting or font coverage policy.
Grapheme boundaries use the same pinned Unicode 17.0 data as caret maps; separate
line-break/bidi/shaping versions and their limits still apply.

Every constructed buffer gets a distinct process-local owner identity. Every
mutating method requires its current `EditRevision`, comparing both owner and
monotonic sequence. A token from another document fails even for identical source
and sequence; stale cursors, edits and undo/redo requests fail without mutation.
Owners and revisions never wrap: exhaustion rejects. Tokens are runtime cache
identities, not persistent IDs or serialized collaboration/version histories.

Selection endpoints must be original UTF8 extended-grapheme boundaries and keep
both direction and affinity. Selection-only changes advance revision, while
preserving undo/redo. Exact selection no-ops leave revision unchanged. Replacing
an ordered half-open range is one atomic edit. Successful content changes collapse
selection after the insertion; if combining/ZWJ/CRLF joining removes that byte
boundary, the caret snaps forward to the next resulting grapheme boundary or
source end. Net identical source replacements preserve all state, including
selection, revision and redo, without collapsing the old selection.

`replace_selection` uses the selected logical range, independent of anchor/focus
order. `delete_backward`/`delete_forward` remove the selection or one adjacent
complete grapheme; boundaries inside combining marks, CRLF or joined emoji are
never exposed. Source-edge deletions are exact no-ops. Validation, byte capacity,
owner/revision and sequence-exhaustion errors preserve text, selection, history
and token together; no partial edit is published.

Each content edit adds one exact prior text/selection snapshot, clears redo and
evicts the oldest undo at capacity. No implicit typing coalescing or grouped
transaction protocol is supplied. History 0 permits edits without snapshots.
Undo/redo restore exact text and directional selection, advance revision and
never restore the old token. Selection moves after undo keep redo; redo snapshots
the currently selected state for the next undo. Empty histories are no-ops.

`prepare_layout` shapes the current source under one immutable borrow, retaining
its token in `PreparedTextLayout`. `layout()` supplies the immutable paragraph for
caret/selection preparation; `matches(&buffer)` must pass before treating a cached
layout as current. Selection-only changes conservatively invalidate the token.
Old layouts remain readable immutable source snapshots, and restoring the same
text through undo does not make their old tokens current again. Font and shaping
style identity remain caller-owned; rebuilding font/style also invalidates the
consumer's cache independently.

Portable tests cover foreign/stale caches, byte/history caps, complete failure
preservation, no-op redo semantics, historical directional selection, grapheme
joining/deletion and overflow without resetting the global owner allocator.
This remains a bounded source model: no native/browser input, screenreader,
real-time deadline or full Unicode/script support is established.
