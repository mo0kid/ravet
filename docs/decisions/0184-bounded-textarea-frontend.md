# ADR 0184: Bounded textarea frontend ownership

Status: implemented within the browser DOM profile

## Decision

Expose `RavetControls.TextEditor` around an explicit connected textarea, retaining
consumer markup and native browser editing presentation. Own at most 4096 committed
UTF-8 bytes and 64 combined undo/redo snapshots. Validate scalar text, LF newlines,
byte/grapheme selection boundaries and revision limits before public mutations.
Reject lone UTF-16 surrogates instead of relying on lossy UTF-8 encoding.

Give each JavaScript committed state an exact object-identity token. Rust document
ownership and revision tokens remain separate. Quiet resynchronization APIs and
explicit consumer request/acknowledgement ownership prevent notification loops and
stale responses from overwriting current editing. Browser `Intl.Segmenter` data is
not assumed to equal the Rust buffer's Unicode 17 grapheme data.

Intercept recognized basic native edits and history, then reconcile unknown or
noncancelable input from the actual DOM before notifying. Preserve directional
selection and snap/expand browser selections to complete graphemes. Do not implement
another browser painting, scrolling or bidi-navigation engine.

Retain a committed baseline through native composition and reject programmatic
writes as Busy. One next-task finalization coalesces trailing input into one history
snapshot. Cancel pending ownership on blur, uneditable transitions, removal or
disposal. Consumer callbacks run behind a reentrancy guard; exceptions retain an
already committed state and disposal is allowed. Repeated application of unchanged
selection avoids recursively generating native select events.

## Consequences and evidence

The frontend allocates on the UI thread and bounds its committed snapshots, but
native preedit, browser undo/layout and operating-system work remain opaque. Exact
source no-ops in programmatic replace preserve all state; source-identical native
typing may produce a selection revision. Textarea LF normalization, browser grapheme
versions and source-affinity differences require explicit bridge validation.

Actual Chromium and hidden WebKit pass the scoped DOM contract fixture. Trusted
Chromium keyboard routing exercises Tab, typing, directional selection, deletion,
undo/redo and newline. `document.hasFocus()` was false in the automated Chromium
surface; no native-window focus or assistive-technology proof is inferred. Composition
fixtures are synthetic and establish guard/coalescing semantics only. Actual IME,
screen readers, mobile keyboards and platform/backend certification remain consumer
validation work. See [browser text editing](../browser-text-editing.md).
