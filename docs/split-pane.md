# Local split-pane layout

`RavetControls.SplitPane` lays out two consumer-owned panes with a labelled,
keyboard-focusable separator. It uses CSS grid in one connected live original
HTML document. The root must have exactly three direct element children in the
order `first`, `separator`, `second`; both panes need distinct existing IDs whose
lookup resolves to those exact elements. The separator is an empty, non-native-
interactive HTML element. The separator must remain element-child-free and non-interactive/non-editable (including inherited contenteditable). Plain separator text is allowed; adding an element child, native-interactive href or editable state retires before input admission. Arbitrary text and elements inside panes remain consumer-owned. Root direct text must consist only of CSS-collapsible ASCII space/tab/LF/CR/form-feed; NBSP and other text would create anonymous grid items and are rejected. Root direct child nodes are bounded to 64; comments and collapsible whitespace are allowed. Later illegal direct content retires the wrapper. Duplicate live
wrappers on a root or separator reject before DOM changes.

```javascript
const split = new RavetControls.SplitPane(root, first, separator, second, {
  label: 'Editor and preview sizes',
  axis: 'horizontal',
  minimumPixels: 120,
  secondMinimumPixels: 160,
  onResize: ({value, source}) => savePaneFraction(value),
});
```

The explicit label contains 1–256 UTF-16 code units and at least one non-whitespace
character. `axis` is `horizontal` or `vertical`; `rtl` is an explicit boolean and
reverses horizontal placement and arrow/pointer direction. `value` is a finite
fraction in 0–1, initially 0.5. Integer `minimumPixels` and `secondMinimumPixels`
are each 0–4096; integer separator `thickness` is 1–64 CSS pixels, initially 8.
The finite positive keyboard `step` is at most 1, initially 0.01. An optional
`onResize` is a function or `null`. Invalid profiles reject atomically.

`setValue(fraction, {notify: false})` validates and clamps to measured legal pixel
minima before committing. With `notify: true`, a changed accepted value sends one
frozen `{value, source: 'programmatic'}`. Axis arrows use `step`, Shift uses one
tenth of it, and Home/End choose measured feasible bounds. Accepted keyboard and
pointer changes send the corresponding `keyboard` or `pointer` source. Unrelated
keys and Ctrl/Meta/Alt remain consumer-owned. An effective same-value update
succeeds quietly. Programmatic updates reject while a pointer drag is held; an accepted keyboard resize cancels the old pointer anchor before committing. `currentValue()` returns the committed fraction.

`refresh()` measures the available root content extent, updates layout/ARIA, and
silently clamps the current value if constraints changed. ResizeObserver and
original-document mutation observation call the same bounded control operation;
there is no audio callback or timer-polled focus trap. If the extent minus the
separator cannot fit both minima, `refresh()` returns false and `lastError()` is
`TooSmall`. Fractional tracks still fit the viewport and clip consumer content;
the requested pixel minima cannot be honored. The separator becomes disabled
and leaves sequential focus. A later feasible refresh restores its Tab stop
without moving global focus. Consumer content should create its own scroll
containers if clipping is undesirable.

A drag owns one pointer ID in the original window, attempts native pointer
capture, and stops on exact release/cancel/lost capture, original-window blur,
hidden document, removal, unavailable members, or changed available extent.
Changed extent cancels the delta origin rather than guessing a new coordinate. In-flow generated root ::before/::after content is rejected at construction and later suspends with Layout. Pseudo content none/normal, display:none, or out-of-flow absolute/fixed decorators is allowed; the component supplies no decorator pointer/occlusion policy.
Every CSS transform, individual translate/rotate/scale, and non-default zoom on
a member or any of up to 64 root ancestors is unsupported, even an identity
transform. Construction rejects such layouts. Later transforms suspend input
with `Transform`; removing them allows `refresh()` to recover. No transformed
coordinate, ancestor beyond that bound, shadow DOM, or cross-origin policy is
implied.

Removal/adoption, child order, pane ID, duplicate ID ownership, or fixed
association changes retire the wrapper. `Ownership` records that retirement.
Healthy unavailable/hidden/inert/display-none layouts suspend with `Unavailable`.
`lastError()` also reports `Callback` when notification throws; committed layout
cannot be rolled back after the consumer received it. Callback and focus-time
reentry cannot start another change or refresh. Disposal is legal in callbacks,
and post-callback/focus checks reject stale success. Consumer callbacks that
hide/transform or change association suspend/retire synchronously, including
throwing callbacks. No global focus restoration is attempted.

The component owns enumerated root grid templates/areas/auto-placement/spacing/horizontal writing mode/direction/overflow styles,
member static position/float/grid placement/order/alignment, pane width/height/minimum/maximum and
clipping styles, member box sizing/edge spacing/border widths, separator touch
action, and separator role/name/orientation/controls/value/disabled/tabindex
attributes. Consumers must not edit those owned properties during a live
wrapper; theme or content styles outside that set remain consumer-owned.
Original inline style text is bounded to 16384 code units per element; each
owned original longhand/attribute is bounded to 4096. `dispose()` is idempotent,
removes listeners/observers, ends the drag, and restores exact original owned
values and priorities. If no unrelated inline style edits remain, it restores
raw style-attribute serialization and presence too; otherwise unrelated edits
survive while owned longhands restore. Cleanup works after association changes.
Queued observer work cannot mutate a disposed wrapper.

The focused fixtures exercise 37 groups in Chromium and hidden WKWebView:
actual horizontal/vertical/RTL geometry, measured minima, TooSmall recovery and
oversized content clipping, single-pointer ownership, reentrant callbacks,
transforms on all members, fixed associations, exact cleanup, queued observer
work, and child-document routing. Trusted Chromium Tab/arrow/Home/End delivery
also changed the separator value as expected. Pointer/focus reentry fixtures use
explicit synthetic input; the hidden Chromium document reports
`document.hasFocus() === false` outside key delivery. These are browser DOM and
keyboard contracts, with no native pointer routing, native focus, native
accessibility-tree, screen-reader, window minimum, or deadline certification.

Build and run the native hidden fixture from an ordinary local checkout:

```sh
cmake -S crates/ravet-surface/native -B target/native-editor -DRAVET_WEB_EDITOR_TESTS=ON
cmake --build target/native-editor --target ravet-web-split-pane -j 2
ctest --test-dir target/native-editor -R '^ravet-web-split-pane$' --output-on-failure
```

The CMake fixture embeds the same trusted shared script inline within its local
read scope. It opens no host/device/window. See [decision 0204](decisions/0204-local-split-pane-layout.md).
