# Browser text editing

`RavetControls.TextEditor` owns the committed source of one connected, consumer-labelled
`textarea` in a live document. Include the [web controls](web-controls.md) asset,
provide a label and any help text, and create the helper after inserting the field:

```html
<label for="notes">Notes</label>
<textarea id="notes"></textarea>
<script>
const editor = new RavetControls.TextEditor(document.getElementById('notes'), {
  maximumBytes: 4096,
  history: 64,
  onEdit(snapshot, kind) {
    // UI-thread notification: persist or bridge according to consumer ownership.
    console.log(kind, snapshot.value, snapshot.selection);
  }
});
const snapshot = editor.snapshot();
editor.replace(snapshot.revision, {start: 0, end: 0}, 'Hello');
</script>
```

This is a DOM textarea frontend. The [Rust text buffer](text-editing.md), paragraph
shaping, source caret maps and SVG drawing remain separate APIs. Textarea painting,
scrolling, bidi movement and platform input presentation belong to the browser.
Consumers own labels, styling, native host integration and validation with their
actual users and platforms.

## Source and selection

`maximumBytes` is an integer from 1 through 4096; `history` is an integer from 0
through 64, shared across undo and redo snapshots. Defaults are 4096 and 64.
Committed source is valid Unicode scalar text with LF newlines. CR, other C0/C1
controls, Unicode noncharacters and lone UTF-16 surrogates reject without replacing
surrogates with U+FFFD. Browser textarea normalization can already have converted
CR or CRLF to LF before construction or native input; programmatic source containing
CR rejects. The helper validates the actual native DOM value before publishing it.
Native preedit, browser internal history, layout and operating-system work remain
opaque; the committed-source limits do not bound that transient work or establish
a real-time deadline.

Selections are `{anchor, focus}` UTF-8 byte offsets at extended grapheme boundaries,
with direction retained. Conversion to native UTF-16 offsets is explicit; byte
offsets inside a scalar or grapheme reject. A native collapsed caret inside a
grapheme advances to its end. A native range expands its start backward and end
forward to complete graphemes, then restores that legal range in the field.
Insertion that joins combining marks or emoji moves its caret to the next complete
grapheme. `Intl.Segmenter` in the textarea's own window supplies boundaries. Its
Unicode data depends on the browser, and can differ from the Rust buffer's Unicode
17 data. No caret affinity or shaped-ligature position is carried by this frontend.

## Revisions, edits and history

`snapshot()` returns a frozen `{value, selection, revision}` view. Keep the exact
revision object for an operation; `matches(revision)` compares object identity.
Foreign editors, stale snapshots and serialized or cloned revision objects reject
with `TextEditorError.code === 'StaleRevision'`. The visible `sequence` is an ordinal,
not an ownership token. `maximumRevision` optionally bounds that ordinal from 1
through `Number.MAX_SAFE_INTEGER`, which is also the default; exhaustion rejects
changes without wrapping. Exact no-ops remain legal at the limit. Tokens are local
JavaScript objects, independent of Rust `EditRevision`, and are not security
credentials.

The methods `replace(revision, {start, end}, text)`, `undo(revision)` and
`redo(revision)` notify `onEdit(snapshot, kind)` after the committed state and DOM
agree. The replace range must be ordered and grapheme-safe. History restores source
and directional selection together under a fresh revision, evicts the oldest undo
snapshot at capacity, and clears redo for a new source edit. Zero history retains
edits without snapshots. An empty undo/redo or source-identical programmatic replace
preserves revision, selection and redo.

`setSelection(revision, selection)` quietly changes only selection and preserves
history. `update(revision, {value, selection})` quietly resynchronizes source and
clears both history directions when the source changes; omitted selection retains
the existing offsets and must remain legal for the supplied source. Quiet methods
allow consumer acknowledgements without a notification loop. Exact state no-ops
preserve revision. Public invalid source, capacity, stale revision and exhaustion
errors preserve committed state and history.

Native arrow and pointer selection remain browser behavior, followed by legal
selection synchronization. Recognized `beforeinput` insertion, newline and basic
backward/forward deletion use complete graphemes. Unknown or noncancelable input
falls back to reconciliation of the actual DOM on `input`, before notification.
Source-identical native typing can collapse the native selection and publish a
selection revision while preserving history. Control/Command Z, Shift Z and Control
Y use the helper's undo/redo history. Tab leaves the field using normal browser
navigation. Hidden, inert, disabled and readonly fields reject native source edits.
Programmatic methods can update such fields; they still require a connected field
in its original live document.

Notifications execute synchronously under an edit guard. Reentrant mutating calls
throw `Busy`; reading snapshots and disposing are allowed. A callback exception
leaves an already committed edit intact. Native event handlers record validation
errors or `Callback` in `lastError()` and restore invalid native candidates; direct
method calls throw to the caller. Dispose is idempotent, cancels pending work,
detaches listeners/observers and restores the exact original `maxlength` attribute.
It retains committed source in the consumer's field. Removal or adoption into a
different document disposes after the mutation observer runs; immediate public
updates reject `Unavailable`. Disposed operations reject `Disposed`.

## Composition and consumer bridges

Composition retains the committed baseline while the browser displays preedit.
Programmatic writes and history reject `Busy` throughout composition and its pending
finalization. One next-task timer coalesces a trailing `input` after `compositionend`
into one validated source edit and one history snapshot. Invalid or oversized
commits restore the baseline. Blur, disabling/hiding, removal and disposal cancel
pending ownership; no queue of programmatic updates is retained. Native noncancelable
history likewise allows one pending transaction until its `input` or cancellation.
Actual platform IME event order requires consumer validation.

A notification does not mean Rust accepted the source. A consumer bridge must own a
specific Rust document and current Rust revision, assign a separate bounded request
identifier, and retain the local JavaScript revision associated with that request.
Validate LF policy, UTF-8 ranges, browser/Rust grapheme differences and source limits
on the receiving side. Acknowledge or reject explicitly. Apply a returned committed
snapshot quietly only if its associated local token still matches; an older response
must not overwrite newer editing. Resynchronize deliberately after conflict, and
retry after composition ends rather than queuing unbounded updates. This helper
provides no transport, Rust-token serialization, distributed undo or acknowledgement
protocol.

## Validation

The [native fixture](../crates/ravet-surface/native/tests/text-editor.js) checks bounded
source, owner/revision identity, legal selections, undo/redo, actual-DOM reconciliation,
synthetic composition, callback reentrancy/disposal and iframe document ownership.
Run the macOS fixture with the existing native editor CMake build:

```sh
cmake -S crates/ravet-surface/native -B target/web-editor -DRAVET_WEB_EDITOR_TESTS=ON
cmake --build target/web-editor --target ravet-web-text-editor
ctest --test-dir target/web-editor -R '^ravet-web-text-editor$' --output-on-failure
```

The same trusted HTML/JS fixture runs in a browser. Chromium keyboard automation
also exercised typing, directional selection, deletion, undo/redo, newline and Tab
exit. The automated Chromium document reported `document.hasFocus() === false`, so
that establishes DOM keyboard behavior rather than native-window visual focus.
Hidden WebKit passed the DOM contract groups without creating a native window.
Synthetic composition tests do not establish an operating-system IME, screen reader,
clipboard permission, mobile keyboard or other browser/backend support claim.
