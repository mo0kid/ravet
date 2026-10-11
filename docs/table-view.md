# Fixed readonly table grid

`RavetControls.TableView` supplies rectangular cell keyboard navigation and
separate row selection over an existing native HTML table. It creates no rows,
headers or cells and does not edit data. Consumers supply labels, content, theme
and table layout. Sorting, editing, dynamic membership, virtualization, column
resizing/reorder, nested tables and interactive cell controls are separate
capabilities.

```javascript
const grid = new RavetControls.TableView(table,
  [{node: nameHeader, label: 'Name'}, {node: valueHeader, label: 'Value'}],
  [{node: firstRow, cells: [
    {node: nameCell, label: 'Gain'}, {node: valueCell, label: '0.75'},
  ]}],
  {label: 'Readouts', multiple: true, selected: [],
   active: {row: 0, column: 0}, onSelection: snapshot => {}});
```

## Native profile and bounds

The root is a connected native table with exactly one thead followed by one
tbody. Thead contains one native tr with exactly the supplied native th headers
in order, each with explicit scope=col. Tbody contains exactly the supplied
native tr rows in order, each with exactly the supplied native td cells. No
caption, extra sections, spans, nested table or generated membership is supplied.
Colspan/rowspan must be absent or exactly `"1"`, with native properties equal1.
There are1..32 columns,1..256 rows and at most4096 body cells. All rows have the
same column count. Header/row/cell IDs are distinct existing1..128 UTF16 code
units without whitespace, whose original-document lookup resolves exactly.
Root/header/cell labels are explicit nonblank1..256 UTF16 code units.

All members belong to one original live document/window. Removed original owner
frames and replaced/closed window documents reject subsequent admission. Roots/
members cannot have competing wrappers. No editable members (including inherited
contenteditable), native interactive or other tabindex descendants are allowed.
Other labelled decoration may remain consumer-owned. A bounded profile traversal
admits at most16384 descendant elements. Shadow and cross-origin hierarchies are
outside this profile. Model/scan bounds do not bound browser layout, global
document lookup/ancestor cost, memory use elsewhere or real-time deadlines.

Malformed profiles, dimension/model/name errors, duplicate ownership or original
owned attribute values over4096 UTF16 code units throw TypeError before writes
or ownership/listener installation. Constructor arrays, entry labels and active
coordinates are copied; later mutation of supplied model objects is not a
membership update. Consumers recreate the control for a new fixed table.

The root owns grid role, explicit name/reference precedence, aria-readonly=true,
multiselectable, rowcount(including header row), colcount, active cell reference
and tabindex. Header/body sections own rowgroup role and tabindex=-1. Header row
owns row role/index1 and tabindex=-1. Headers own columnheader role, explicit
name/reference precedence, column index and tabindex=-1. Body rows own row role,
row index(beginning2), aria-selected and tabindex=-1. Body cells own gridcell
role, explicit name/reference precedence, column index, headers=matching header
ID and tabindex=-1. Native structural profile, IDs, scope and spans remain
consumer-owned and must stay unchanged.

Only the table is sequentially focusable while eligible cells exist. The table
holds actual focus and aria-activedescendant points to a body cell; headers are
associated but do not receive grid navigation focus or sorting actions. All
owned attributes restore exactly on idempotent dispose, including absence and
prior tabindex/name references, even after retirement. Cleanup does not restore
focus, reorder/remove rows or roll back consumer structural/content changes.

## Cell focus versus selected rows

An eligible cell requires an eligible row, cell and matching header. Hidden,
inert, aria-hidden/disabled, disabled, invisible and non-rendered elements are
unavailable. Hiding a header logically removes that column from keyboard/selection
admission even if theme CSS still renders a body cell; layout remains consumer
owned. A row is selectable if at least one cell is eligible.

Active focus and selected rows are independent. Initial selected rows may be
unavailable (including a hidden table). Later selected replacements may carry or
remove already-selected unavailable rows but cannot newly add them. Refresh
never prunes selection. Explicit accepted user/programmatic replacement may
remove unavailable selections.

When active cell becomes unavailable, quiet fallback chooses the nearest column
in the same row first, then nearest row and column; equal distances prefer the
following row/column. It never changes selection or notifies. All-unavailable
sets active row/column=-1, removes active reference and retires table Tab; reveal
recovers quietly.

| Input | Behavior |
| --- | --- |
| Up/Down | Previous/next eligible cell in the same column. Single mode selects a changed destination row; multiple mode changes active only. |
| Left/Right | Previous/next eligible cell in same row without changing row selection. |
| Home/End | First/last eligible cell in current row. |
| Ctrl/Meta+Home/End | First/last eligible cell globally, active only. |
| Ctrl/Meta arrows | Active movement only. |
| Space | Select active row in single mode; toggle active row in multiple mode. |
| Ordinary cell click | Replace selection with row and restore table focus. |
| Ctrl/Meta cell click, multiple mode | Toggle eligible row. |
| Shift+Up/Down or cell click, multiple mode | Add eligible row range to captured baseline; shrinking removes range additions. |

Directional boundaries without movement leave unrelated selection unchanged and
never install a range anchor. Only genuine Shift+Up/Down without Ctrl/Meta
preserves an established anchor at a boundary; other navigation boundaries clear
it, including Shift horizontal/Home/End and Ctrl/Meta+Shift.
Ranges exclude new unavailable rows and preserve retained baseline selections.
Other actual navigation, explicit model replacement, Space(including Shift), Shift
release, eligibility change, blur and document hiding clear range anchors.
Ctrl/Meta+Shift changes active/toggles rather than creating ranges. Alt/composing,
Ctrl/Meta+Space, Enter and unrelated keys remain consumer-owned. Native Tab exits
the one-root profile; header and row-background clicks are unowned.

## Methods and lifecycle

`setSelected(indices, {notify=false})` atomically checks bounded unique row
indices (at most one single mode) and unavailable-admission rules.
`setActive({row,column}, {focus=false})` requires an eligible cell and changes
no row selection. Methods return false for invalid arguments, busy/disposed
ownership or unsuccessful requested focus. Invalid argument/model admission
does not overwrite the previous diagnostic. Valid programmatic selection models
can succeed while all cells are unavailable; actual no-ops never notify.

`selection()` returns a frozen snapshot of frozen selectedIndices/selectedIds/
unavailableSelectedIds(each at most256) and frozen activeCell with row,column,id.
`activeCell()` exposes those coordinates/ID read-only, with -1/-1/null when all
unavailable. Selection returns null after disposal or invalid fixed ownership.
`refresh()` reasserts owned profile and reconciles eligibility, returning whether
table keyboard focus is available. `lastError()` and idempotent `dispose()`
complete the local lifecycle.

Only changed row selection notifies onSelection. Input notifies; programmatic
updates default quiet. Frozen callback snapshots add keyboard,pointer or
programmatic source. One transition guard rejects reentry, permits disposal and
requires actual original-document table focus before pointer model changes.
Rejected/redirected/throwing focus and focus-time cell/header/row retirement
cannot notify a selection. Current ownership/focus is rechecked after callbacks.
Committed/notified selection cannot roll back if consumer mutation or callback
failure occurs afterward. Throwing callbacks report Callback, actual focus
exceptions Focus, invalid ownership Ownership(with retirement), and eligibility
suspension Unavailable. Owned-write guards stop after synchronous DOM setter disposal
before subsequent members can receive stale attributes.

Original-document mutation observation and explicit refresh reassert names/roles/
Tab/associations. Fixed membership/order/ID/section/scope/span, nested interactive/
editable/focusable changes or descendant scan overflow retires. Queued disposed
work is inert. External stylesheet/geometry changes require explicit refresh.
No automatic global focus move or timer-polled focus trap is used.

## Evidence boundaries

Target fixtures test actual table/header geometry and DOM associations, keyboard/
row model behavior, maximal counts/scan boundaries, hidden-column and retained
row models, range/focus failure, runtime retirement, original child documents and
exact cleanup in Chromium and hidden WKWebView. Synthetic focus/pointer/native
setter-hook cases remain synthetic. Trusted browser cell keys/pointer delivery
provide separate evidence; no native OS focus, native AX, screen-reader,
deadline/global-memory or other-platform certification is claimed.

Build and run the native hidden fixture from an ordinary local checkout:

```sh
cmake -S crates/ravet-surface/native -B target/native-editor -DRAVET_WEB_EDITOR_TESTS=ON
cmake --build target/native-editor --target ravet-web-table-view -j 2
ctest --test-dir target/native-editor -R '^ravet-web-table-view$' --output-on-failure
```

The CMake fixture embeds the same trusted shared script inline within its local
read scope. It opens no host/device/window. See
[decision 0214](decisions/0214-fixed-readonly-table-grid.md).

Selection inputs use dense arrays: holes, duplicates and invalid indices reject
before changing the model or notifying. Actual allowed local iframe navigation
also retires a connected root when its original document/window is replaced;
committed model is retained without a callback. Cleanup restores original owned
attributes/styles and disconnects original document/node listeners/observers,
removing safely matching original window listeners even if its frame detached;
replacement/inaccessible/closed WindowProxy cleanup is skipped.
