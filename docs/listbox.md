# Fixed flat-list selection

`RavetControls.ListBox` supplies a labelled vertical listbox with one root Tab
stop, separate active-option focus and single or multiple selection. Consumers
create an ordinary HTML root and 1–256 fixed direct option elements in exact
order. Each option has a distinct existing ID of 1–128 non-whitespace UTF-16
code units whose original-document lookup resolves to that exact option.
Native interactive/editable roots, options and descendants, inherited editing,
and descendants with tabindex are rejected. Consumer content labels and theme
remain explicit; no shadow or cross-origin traversal is supplied.

```javascript
const list = new RavetControls.ListBox(root, options, {
  label: 'Available presets',
  multiple: true,
  selected: [0],
  onSelection: snapshot => showSelectedPresets(snapshot.selectedIds),
});
```

The explicit root label is nonblank and contains 1–256 code units. `multiple` is
a boolean, initially false. `selected` contains distinct valid option indices,
at most one in single mode; initial `active` is a valid index, initially zero.
`onSelection` is a function or null. The bounded initial selection establishes
the caller's model even when options or their containing tab start unavailable.
Later selection admission cannot newly add unavailable options. Original owned
attribute values are bounded to 4096 code units each. Invalid construction and
duplicate live root wrappers reject before changing attributes.

`selection()` returns a frozen snapshot with frozen `selectedIndices`,
`selectedIds`, and `unavailableSelectedIds` arrays plus `activeIndex`. Each array
has at most 256 entries; IDs remain bounded to 128 code units. `activeIndex()`
also reads active state independently. Eligible active state is referenced by
root aria-activedescendant while DOM focus stays on the root. Options leave Tab
order. The root owns listbox role, vertical orientation, explicit label and
reference precedence, multiple state, active reference and tabindex; options own
option role, aria-selected and tabindex. Consumers must not change those owned
attributes during a live wrapper; refresh/observation reasserts the owned root/member roles, tabindex and naming/orientation/value profile after mutation.

`setSelected(indices, {notify: false})` validates complete input before changing
selection. It may carry an already-selected unavailable index or remove it, but
cannot newly introduce one. `setActive(index, {focus: false})` moves only active
state to an eligible option and optionally requests guarded root focus. It emits
no selection notification. An effective unchanged selection is quiet. Only
changed selection with notify:true sends source=programmatic; accepted keyboard
or pointer selection changes send the corresponding source. Every callback gets
one bounded frozen snapshot including active index and source.

Up/Down and Home/End move between eligible options. In single mode they select
the destination, while Ctrl/Meta arrows move only active state. In multiple mode
ordinary arrows move active state quietly; Space toggles the active eligible
option. Ordinary clicks replace selection; Ctrl/Meta clicks toggle an eligible
clicked entry. Shift+arrow or Shift+click starts a range anchored at the prior
active index with a snapshot of prior selection. The resulting selection is
that baseline union eligible options between anchor and destination. Shrinking
the range removes prior range additions and preserves baseline selection.
Unavailable members are never newly added by a range. Shift release, Space
(including Shift+Space), non-Shift intent, explicit changed programmatic
selection, blur/hidden-document lifecycle, or eligibility change clears the
anchor. Rejected focus admission also clears any proposed range baseline. Alt, unrelated keys and composing-key input remain consumer-owned.

`refresh()` and original-document mutation observation reconcile eligibility.
Hidden, inert, aria-hidden/disabled, disabled, display-none or invisible members
are unavailable. Eligibility changes never prune existing selection; snapshots
list its unavailable IDs. Active fallback is separate and quiet: choose the next
eligible index at or after the old one, or the first eligible entry. With none,
active index is -1 and the root leaves Tab order/removes its active reference.
A later healthy refresh recovers without moving global focus. Explicit user or
programmatic replacement can remove retained unavailable selections. In single
mode selecting a different eligible entry replaces a previous unavailable one.
Fixed member removal/adoption/order/ID ownership or interactive-profile changes
retire the wrapper; caller should recreate after dynamic membership changes.

Callback and focus-transition guards block reentry while allowing disposal.
Original association and eligibility are rechecked after synchronous consumer
changes. Focus-requesting/UI changes reject stale success if the root no longer
has the required focus; already committed/notified selection cannot be rolled
back. `lastError()` reports Ownership, Unavailable, Focus, Callback or Lifecycle
as applicable. Notification exceptions keep committed state with Callback;
focus exceptions cannot masquerade as notification exceptions. `dispose()` is
idempotent, removes listeners/observer, clears the range anchor, and restores
exact original owned attributes even after association retirement. Queued
observer work cannot change a disposed wrapper.

The focused fixture passes 35 groups in Chromium and hidden WKWebView, including
actual role/Tab/active/selected state, hidden-tab initial model, retained selection
versus active fallback, range shrinking/invalidation, explicit replacements,
maximal snapshots, callback/focus retirement, exact cleanup and child-document
routing. Trusted Chromium Tab/ArrowDown/Space confirms root traversal, independent
active movement and one selection notification. Pointer/focus reentry fixtures
are synthetic; preview document.hasFocus is false outside trusted key delivery.
DOM-derived semantics are distinct from native AX or screen-reader validation.
There is no native pointer/focus certification, tree/table/virtualization,
typeahead/global shortcut, deadline or global memory guarantee.

Build and run the native hidden fixture from an ordinary local checkout:

```sh
cmake -S crates/ravet-surface/native -B target/native-editor -DRAVET_WEB_EDITOR_TESTS=ON
cmake --build target/native-editor --target ravet-web-listbox -j 2
ctest --test-dir target/native-editor -R '^ravet-web-listbox$' --output-on-failure
```

The CMake fixture embeds the same trusted shared script inline within its local
read scope. It opens no host/device/window. See [decision 0208](decisions/0208-fixed-list-selection.md).

Selection inputs use dense arrays: holes, duplicates and invalid indices reject
before changing the model or notifying. Actual allowed local iframe navigation
also retires a connected root when its original document/window is replaced;
committed model is retained without a callback. Cleanup restores original owned
attributes/styles and disconnects original document/node listeners/observers,
removing safely matching original window listeners even if its frame detached;
replacement/inaccessible/closed WindowProxy cleanup is skipped.
