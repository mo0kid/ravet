# Fixed hierarchical tree selection

`RavetControls.TreeView` owns one labelled, fixed hierarchy in a connected local
document. It supplies virtual keyboard focus, branch expansion and a separate
selected model. It creates no items, groups or buttons. Consumers supply theme,
layout and row contents. Table/grid navigation, dynamic membership,
virtualization, sorting, typeahead and drag/reordering are separate capabilities.

## Construction and owned profile

```javascript
const tree = new RavetControls.TreeView(root, [
  { node: folder, label: 'Folder', parent: null, group, toggle },
  { node: child, label: 'Child', parent: 0, group: null, toggle: null },
  { node: sibling, label: 'Sibling', parent: null, group: null, toggle: null },
], {
  label: 'Library', multiple: true, selected: [1], active: 0,
  expanded: [], onSelection: snapshot => {}, onExpansion: snapshot => {},
});
```

Supply 1–256 entries in fixed depth-first preorder, with depth at most 32. A
parent is `null` or the preceding index of its parent. Root direct elements must
be exactly the top-level nodes, in order. A branch supplies an existing direct
child group containing exactly its direct child item elements in order, and an
existing direct child native `button` with the explicit attribute `type="button"`.
A leaf supplies neither. Nested text/decoration inside the button is allowed.
Group-background clicks are unowned; a row click routes to the deepest item,
rather than its containing ancestor.

Root, items, groups and toggles must be distinct connected HTML elements in one
original live document/window. One tree owns each root/member. A removed original owner frame or replaced
window document rejects later admission. Items and groups
have distinct IDs of 1–128 UTF-16 code units, with no whitespace, resolving to
exactly themselves in that document. Root and item labels are explicit nonblank
strings of 1–256 UTF-16 code units. Root/items/groups must be non-native-interactive
and non-editable, including inherited contenteditable. Other descendants cannot
be native interactive/editable or have tabindex; owned items/groups/toggles are
exempt from the owned tabindex check. Root descendants are scanned with a bound
of 8,192 elements before writes and every admission. The implementation's model
and scan bounds do not bound browser layout, document lookup or ancestor costs,
or certify a real-time deadline.

Malformed profiles/models, duplicate ownership, detached/no-window documents,
preorder/parent errors or capacity violations throw `TypeError` before changing
attributes/styles or installing ownership/listeners. Entry objects and initial
selected/expanded arrays are copied. Consumer mutation of the original array
does not alter the fixed model.

The root owns `role=tree`, explicit `aria-label` (removing aria-labelledby),
aria-multiselectable, aria-activedescendant and tabindex. It is the only sequential
Tab stop while available. Items own treeitem role, explicit name/reference
precedence, level/position/sibling count, selected state, branch-only expanded
state and tabindex=-1. Groups own group role, tabindex=-1, hidden and inline
display. Collapsed groups receive display:none!important, so hostile theme CSS
cannot reveal collapsed descendants; expanded groups restore captured original
display and priority. An original display:none can still make an expanded group
unavailable. Toggles own button role, a bounded Expand/Collapse name (at most
265 code units), reference precedence, controls/expanded state and tabindex=-1.
The native type remains consumer-owned and must stay explicitly button.

Original owned attribute/display values are at most 4,096 code units; original
group style text is at most 16,384. `dispose()` removes listeners/observation and
restores exact owned attributes/display/priority, including originally absent
attributes. Original raw style serialization restores when no unrelated inline
edits remain; unrelated edits survive. Cleanup restores already-owned properties
even after retirement. It does not restore focus or generate/remove elements. Synchronous custom-element
attribute callbacks can dispose safely during owned writes: later attribute/style
writes stop before touching the cleaned-up profile.

## Active focus, expansion and selection

Active focus is one visible eligible item referenced by the root. A collapsed
ancestor always makes descendants unavailable. Hidden, inert, aria-hidden,
aria-disabled, disabled, invisible or non-rendered items are unavailable.
Refresh never prunes selected items. Constructor `selected` may initialize
unavailable/collapsed entries; later replacements can carry or remove already
selected unavailable entries but cannot newly add them. This permits hidden-tab
models and retained selections across collapse.

Collapse hides active descendants and quietly falls active focus back to the
nearest eligible visible ancestor, then the nearest eligible preorder position.
Selection is independent and retained. All-unavailable retires root Tab/reference
and sets active=-1. Recovery is quiet. A pointer toggle makes that branch active;
it does not select it.

| Input while root focused | Behavior |
| --- | --- |
| Up/Down, Home/End | Traverse visible eligible preorder. Single mode selects a changed destination; multiple mode changes only active focus. |
| Ctrl/Meta + Up/Down/Home/End | Change active focus without single-mode selection. |
| Right | Expand a collapsed branch without selection; otherwise move to its first eligible direct child. |
| Left | Collapse an expanded branch without selection; otherwise move to its nearest eligible ancestor. |
| Space | Select active in single mode, toggle active selection in multiple mode. |
| Ordinary row click | Replace selection with the eligible row and restore root focus. |
| Ctrl/Meta row click, multiple mode | Toggle the eligible row without removing retained unavailable selections. |
| Shift + Up/Down/Home/End or row click, multiple mode | Add eligible visible range to a captured baseline; shrinking removes only the range additions. |
| Native branch toggle activation | Expand/collapse only; restore root focus. |

No-movement directional boundaries do not select an unrelated active model.
Expansion changes, Left/Right, eligibility changes, explicit model replacement,
Space (including Shift+Space), Shift release, window blur or document hiding clear
the range anchor. A range cannot newly add unavailable/collapsed entries.
Composing, Alt, Ctrl/Meta+Space/Left/Right, Enter and unrelated keys stay consumer
owned. Tab remains native. Consumers own other modifier behavior.

Toggle eligibility matters separately: hidden/disabled/unavailable native
toggles cannot admit pointer or keyboard expansion. Programmatic model updates
are separate and may expand such a branch. Native pointerdown/default button
focus remains intact: a button may receive focus before click. The tree takes
its transition guard, requests root focus and verifies actual original-document
activeElement plus current row/toggle eligibility **before** model changes.
Rejected/redirected/throwing focus, focus-time disable/hide, retirement or disposal
admits no expansion/selection callback. Accepted activation returns root virtual
focus. A nested toggle label emits only one expansion notification.

## Methods and callbacks

`setExpanded(indices, {notify=false})` validates a unique branch-only bounded index
set atomically. `setSelected(indices, {notify=false})` validates a unique index set
(at most one in single mode), with unavailable-admission rules above.
`setActive(index, {focus=false})` requires current visible eligibility and changes
no selection. They return false for invalid arguments, busy transitions, disposed
ownership or unsuccessful requested focus. Effective no-ops do not notify.
Valid programmatic models can succeed while all items are unavailable.

`selection()` returns a frozen snapshot with frozen selectedIndices, selectedIds,
unavailableSelectedIds (each at most 256), and activeIndex. `expansion()` returns
frozen expandedIndices/expandedIds of branch item IDs and activeIndex.
They return null after disposal or invalid fixed ownership.
`activeIndex()` and `lastError()` are read-only; `refresh()` reasserts the owned
profile, reconciles eligibility and returns whether root keyboard focus is
available. `dispose()` is idempotent.

Only changed selected state calls onSelection, only changed expansion calls
onExpansion, and only when notification is enabled. Input notifies automatically;
programmatic updates default to quiet. Callback snapshots add source=keyboard,
pointer or programmatic. Quiet active fallback never sends either callback.
One transition guard rejects callback/focus reentry; disposal stays legal.
Post-focus and post-callback ownership, eligibility and required focus are
revalidated. A host/consumer callback can retire or redirect focus after an
accepted change: the method reports failure but cannot roll back an already
committed/notified model. A throwing callback retains committed state and reports
Callback. Focus exceptions report Focus; original-document/profile failure
reports Ownership and retires; unavailable focus/input reports Unavailable.
Invalid arguments do not replace the previous diagnostic. Native handlers do
not propagate callback exceptions.

Original-document mutation observation reasserts owned roles/names/tabindex and
reconciles state. Fixed member/order/ID/native profile removal/adoption, new nested
focusable elements or descendant scan overflow retires. Queued disposed observer
work is inert. Stylesheet/geometry changes outside observation require explicit
refresh. No timer-polled focus trap or global focus movement is used.

## Validation

Focused fixtures exercise actual browser hierarchy/layout, retained collapsed
selection, native-toggle focus admission, keyboard/model/range boundaries,
reentrant callbacks, retirement, exact restoration and same-origin child-document
routing in Chromium and hidden WKWebView. Trusted browser Tab/Right/Down/Space and
nested-label pointer activation provide additional delivery evidence. Synthetic
focus/pointer cases remain synthetic. Chromium fixture document.hasFocus=false
outside trusted input is retained; trusted pointer delivery observed true.
This evidence is not native accessibility-tree access, screen-reader validation,
native OS focus or other-platform certification. W190's missing native WebKit
control-tree result remains a separate explicit limitation.

Build and run the native hidden fixture from an ordinary local checkout:

```sh
cmake -S crates/ravet-surface/native -B target/native-editor -DRAVET_WEB_EDITOR_TESTS=ON
cmake --build target/native-editor --target ravet-web-tree -j 2
ctest --test-dir target/native-editor -R '^ravet-web-tree$' --output-on-failure
```

The CMake fixture embeds the same trusted shared script inline within its local
read scope. It opens no host/device/window. See
[decision 0211](decisions/0211-fixed-tree-expansion-selection.md).

Selection inputs use dense arrays: holes, duplicates and invalid indices reject
before changing the model or notifying. Actual allowed local iframe navigation
also retires a connected root when its original document/window is replaced;
committed model is retained without a callback. Cleanup restores original owned
attributes/styles and disconnects original document/node listeners/observers,
removing safely matching original window listeners even if its frame detached;
replacement/inaccessible/closed WindowProxy cleanup is skipped.
