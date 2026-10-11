# Fixed-model virtual text list

`RavetControls.VirtualList` mounts a bounded viewport of a larger fixed copied
text model. It reuses offscreen native divs after their interaction ownership
ends. Active and pressed rows pin their original model identity. Selection is
independent from scroll and mounted membership. DOM semantics and actual browser
input evidence do not establish native accessibility-tree or screen-reader
support.

```js
const list = new RavetControls.VirtualList(viewport, items, {
  label: 'Readouts', multiple: true, rowHeight: 24, overscan: 8,
  selected: [4], active: 0,
  onSelection(snapshot) { /* changed bounded model; consumer bridge */ },
});
list.setActive(600, {focus: false, scroll: true});
list.setSelected([4, 600], {notify: true});
list.dispose();
```

Each item has `{id, label, disabled}`. Constructor copies and freezes the model;
subsequent caller edits cannot replace items or eligibility. IDs are unique
nonempty strings of at most 128 UTF16 code units, labels explicit non-whitespace
strings of at most 256; disabled is boolean. Root label has the same256 bound.
Model contains 1..65536 items, rowHeight 12..256 finite pixels, overscan 0..32 integer,
and exact model extent cannot exceed 1000000 pixels. Selection uses dense arrays with at most 256
unique indices (single mode at most 1). Constructor rejects malformed or over-limit
input before DOM/style mutation. Owned original attributes are bounded 4096 code
units each, raw original inline style 16384 and owned property values 4096.

## Geometry and generated ownership

Supply one connected original live-document HTMLElement, outside editable or
native-interactive content, empty except at most 64 direct ASCII-collapsible
whitespace/comment nodes. The original document/window/frame must remain live.
Only one wrapper owns a root. Consumer provides viewport height/theme. The
wrapper owns position/display/overflow/padding/box-sizing/grid/contain properties
and exact root role/name/reference/tabindex attributes. Required properties use
important inline declarations; unrelated consumer inline edits survive cleanup.
Root hidden attribute is respected with owned display:none until revealed.

A generated relative spacer defines the model extent. Rows use exact absolute
`index * rowHeight`, fixed height, clipped single-line textContent (no consumer
HTML), roleoption/name/selection/disabled/setsize/posinset and tabindex-1. Root
owns one sequential Tab stop and active descendant. An offscreen active row pins
its real position and ID; it is not a hidden duplicate. Model entries outside
viewport remain eligible unless copied disabled state says otherwise.

Viewport clientHeight must be finite positive and no greater 4096px. Zero,
hidden, inert or unavailable viewport suspends rendering admission and retires
Tab/active reference; height over 4096 records TooLarge. Unsupported transform/individual translate/rotate/scale/perspective/zoom on
root/ancestors (at most 64 inspected), generated spacer or rows records Coordinates.
Root in-flow pseudo content and any visible generated spacer/row pseudo content
(including absolute decorators) also rejects coordinate admission. Scroll extent must match max(model extent,clientHeight)
within 2px; mounted row offset/height must match expected geometry within 2px.
Geometry failure suspends rather than guessing another coordinate mapping.
These limits bound wrapper work, not browser layout, document lookup internals,
consumer callback allocation or global platform memory.

Visible and overscan rows plus at most one active and one pressed pin mount at
most 512 nodes. Mounted plus detached reusable free rows also total at most 512.
Scroll and original-window ResizeObserver/MutationObserver refresh this pool;
scroll does not alter selection/active model or notify. The root can contain at
most 65 direct nodes including its generated spacer. Unexpected consumer children,
removed/adopted root, replaced generated IDs/text/style/children, generated
editable/hidden/inert profiles or externally adopted/reparented retained free
nodes retire ownership before admission. Owned ARIA and Tab fields are reasserted
on refresh. Public getters do not repair stale external mutations implicitly.

Generated IDs use a module-local monotonically increasing safe-integer owner
namespace and model index. Constructor checks all prospective IDs with at most
65536 original-document getElementById lookups before mutation; collision rejects
atomically without consuming the owner. Counter exhaustion rejects instead of
wrapping. Rendering verifies each mounted ID resolves to its owned node; later
collision at a new row retires. This is an original-document lookup contract,
not a global duplicate-ID scan or cross-script cryptographic namespace.

## Selection, keyboard and pointer lifetime

Up/Down/Home/End traverse copied enabled entries and bring active row into view.
Single-mode unmodified destination movement selects; Ctrl/Meta only moves active.
Multiple-mode movement only changes active, Space toggles and Shift movement
forms an additive eligible range over the retained baseline. A genuine range
boundary with no movement preserves an established anchor without creating or
extending selection; nonrange movement clears it. Disabled initial selections
are permitted for model continuity. Later snapshots may carry/remove those
already selected disabled items but cannot newly add them. Scroll/refresh never
prunes selected items. All-disabled model has activeIndex-1 and no root Tab/ref;
explicit programmatic selection removal remains possible.

Ranges/toggles/programmatic sets exceeding 256 fail with Capacity before changing
selection, active, anchor or notifying. Programmatic/keyboard Capacity failure
also preserves pressed ownership. A rejected native Capacity click consumes
its physical event phase but retains the same token identity and row pin; that
consumed phase cannot accept a duplicate click. No partial range.
Other invalid model requests return false without notification. Hidden/suspended
viewport still accepts legal programmatic model updates; reveal reconciles
rendering quietly. Focus/input admission additionally requires healthy geometry,
root eligibility and actual original-document activeElement ownership.

Original-document pointerdown capture snapshots candidate identity before
cancelling any prior token, including prevented, Alt or untrusted fresh intent.
Candidate must still map to the same row after refresh; cancellation/recycling
cannot reinterpret the new target. Pointer selection requires a trusted primary PointerEvent pointerdown, matching
trusted pointerup, then matching trusted PointerEvent click pointer ID on the
same owned row/index/modelID/namespace. Press pins that exact node through the
native click; it is never recycled in between. Native default row focus is
retained, then valid click restores root before selection. Current clipped row
and viewport bounds and root focus must still admit the click. Pointerup over
another row cancels the press and cannot select its recycled identity. Duplicate
click, click with no valid token, synthetic/no-pointer click, prevented/modified
unowned input and stale detached events cannot bypass admission. Keyboard or
programmatic methods provide explicit stable-model alternatives.

Accepted keyboard/programmatic intent, including a recognized navigation boundary
with no model movement, cancels an older press; rejected capacity
transition preserves it. New pointerdown, matching pointercancel, outside press,
owner window blur/hidden document or explicit `cancelPointer()` also cancels.
Pointerup alone retains the pin if no matching click arrives. Retention is
bounded one node, with no timeout/timer/document-wide tombstone. Consumers may
explicitly cancel an abandoned pointer intention. Modifier click toggles;
ordinary click replaces selection; unmodified Shift click adds a bounded range.

`setSelected(indices,{notify:false})`, `setActive(index,{focus:false,scroll:true})`,
`refresh()`, `selection()`, `activeIndex()`, `renderedIndices()`, `lastError()`,
`cancelPointer()` and `dispose()` form the bounded API. `refresh` returns viewport
readiness; `selection()` returns frozen selectedIndices/selectedIds,
unavailableSelectedIds, activeIndex/activeId, or null for disposed/invalid root.
`renderedIndices` is a frozen sorted array of at most 512. Callback adds source
keyboard/pointer/programmatic and only fires on actual selected model changes.
Diagnostics include Capacity,Unavailable,TooLarge,Coordinates,Geometry,Ownership,
Collision,Focus,Callback/Lifecycle. Diagnostics describe the latest attempt;
a later healthy refresh can clear them, except Capacity remains while its
consumed token retains the pin.

Synchronous focus/render/callback reentry rejects; disposal remains legal. Every
owned write checks disposal before continuing. Post-focus/callback ownership and
required focus revalidate. An accepted notification cannot roll back the model
or consumer external effects if callback throws, hides/reparents/removes the
root or redirects focus; method returns false with retained committed-model
boundary/diagnostic. Disposal clears callbacks/listeners/observers/pins, removes
only owned spacer, releases retained free references and exactly restores owned
root attributes/styles. Navigation can retire the old connected document without
removing its root; cleanup skips the replaced/inaccessible WindowProxy and keeps
original document/node cleanup local. Queued resize/mutation/scroll work after disposal is
inert. Recreated wrappers get a new namespace; old detached events cannot reach
their root handlers.

## Evidence and limits

The focused fixture has 60 executable groups in actual Chromium and hidden WebKit,
including 65536 copied items, first/middle/final viewport geometry, actual node
reuse, disabled retention, all-disabled model, selection Capacity atomicity,
active/pressed pins, adopted free-node retirement, namespace collision, queued
teardown, same-origin iframe and actual custom-element synchronous disposal.
A separate bounded namespace proof checks safe-integer exhaustion. Trusted
Chromium and owned AppKit/WebKit mouse input prove press→scroll→release selects
nothing, then a fresh matching visible-row click selects row 501 exactly once.
Trusted keyboard End reaches 39999 with only 14 mounted options and Space selects
once. Native probe windows are owned fixtures and closed after evidence. Actual WK
Option-down/modifier-release up/click and Command-click Capacity phases are
validated. Chromium prevented-down is actual native input; Alt/Meta guards use
explicit getter injection on actual pointer events because preview has no
modifier-hold pointer API, and are not physical Chromium modifier certification.

Hidden/inactive engine fixtures may have document.hasFocus=false; their
activeElement and synchronous focus-method-hook tests are DOM contract evidence,
not OS focus or a real focus-listener/assistive-tech certification. Trusted input
and native pointer-order evidence are kept separate. No REAPER/DAW/device/global
VoiceOver/accessibility-trust/private-SPI actions are performed. Variable-height
rows, dynamic model replacement/sorting/custom renderers, virtual trees/grids,
editing and other-platform/screen-reader/deadline support are separate work.

Build and run the hidden local fixture from an ordinary checkout:

```sh
cmake -S crates/ravet-surface/native -B target/native-editor -DRAVET_WEB_EDITOR_TESTS=ON
cmake --build target/native-editor --target ravet-web-virtual-list -j 2
ctest --test-dir target/native-editor -R '^ravet-web-virtual-list$' --output-on-failure
```

The sibling `listbox-navigation.html` is copied into the native build directory
for actual allowed local child navigation. The hidden fixture creates no host,
device or window. A separate, explicit probe opens only its own validation window
and closes it afterward:

```sh
cmake --build target/native-editor --target ravet-web-virtual-list-pointer -j 2
target/native-editor/ravet-web-virtual-list-pointer "$(pwd)/target/native-editor/ravet-web-virtual-list.html"
```

Native pointer probe checks primary identity across scroll, prevented/Option fresh
press retirement and native Command Capacity-click consumption/pinning. It is
not registered as a hidden CTest. See [decision 0217](decisions/0217-fixed-virtual-text-list.md).
