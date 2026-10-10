# Local popup menus

`RavetControls.PopupMenu` owns one level of consumer-created action buttons,
keyboard navigation and dismissal. Consumers provide connected markup, labels,
CSS placement and action behavior. Include the [web controls](web-controls.md)
asset and create the helper after inserting the document's elements:

```html
<button type="button" id="actions">Actions</button>
<div id="actions-menu">
  <button type="button" id="save">Save</button>
  <button type="button" id="recall">Recall</button>
</div>
<script>
const menu = new RavetControls.PopupMenu(
  document.getElementById('actions'),
  document.getElementById('actions-menu'),
  [
    {node: document.getElementById('save'), action: 'save'},
    {node: document.getElementById('recall'), action: 'recall'}
  ],
  {label: 'Preset actions', onAction(action, button) { console.log(action); }}
);
</script>
```

Trigger and menu buttons must be native `button` elements with explicit
`type="button"`, in one live owner document. The menu root contains 1..256 distinct,
non-nested buttons. Action identifiers are unique, nonblank strings of 1..128
JavaScript code units. The menu label is nonblank and at most 256 code units.
Invalid profiles reject before modifying attributes. Consumer button labels remain
consumer-owned. Geometry, clipping and viewport placement come from consumer CSS.

The helper supplies `menu`/`menuitem` roles, the root's label, trigger
`aria-haspopup="menu"`/`aria-expanded`, hidden state and one roving Tab stop. It
removes root `aria-labelledby` while its explicit label is owned. Disposal restores
every exact original attribute value and each member's original tabindex, including
original hidden state. The helper begins closed.

`open('first')` or `open('last')` focuses the first or last eligible entry and
returns whether the document accepted that focus and retained a live open menu
after synchronous focus handlers. Focus-time close/disposal/ownership loss rejects
stale success. Trigger click toggles the menu;
ArrowDown/ArrowUp opens at the corresponding edge. Disabled, hidden, inert,
ARIA-disabled, non-visible and detached entries are skipped. No eligible entries
leave the menu closed. Within the menu, vertical arrows wrap and Home/End select
an edge. Modified directions retain browser/consumer handling. Enter and Space
activate the underlying native button's click behavior rather than submitting a
second action from keydown.

Activation closes the menu and restores eligible trigger focus before calling
`onAction(action, button)` once. Further clicks while closed do nothing. The entire activation, including trigger focus restoration,
runs behind a guard: reentrant opening rejects, and disposing is allowed.
Disposal during focus restoration cancels the subsequent action callback. Exceptions
leave the menu closed and set `lastError()` to `Callback`. This is a consumer action
callback, without transport, host state or parameter-gesture acknowledgement.

Escape calls `close()`, hiding the menu and restoring the eligible trigger when
focus belonged to an entry. `close(false)` retains an external focus destination.
All close/restoration paths temporarily guard reopening, including external
trigger-focus listeners, while allowing intentional disposal. Tab uses native browser navigation; owner-document focus events close after the
browser chooses its destination, with a Tab keyup refresh as a fallback. An outside
pointer closes without suppressing its default action or destination focus. A
single pointer identity and next-task intent reset let a trigger click close a
previously open menu even after focus has transferred back to the trigger; outside
release/cancellation, blur and disposal clear that ownership.

`refresh()` rechecks fixed membership and closes if focus has moved outside,
trigger/root eligibility is lost or no entries remain. The mutation observer checks
removal, adoption, hidden/disabled/inert/ARIA-disabled and inline style/class
changes. Consumers call refresh for stylesheet/media changes and programmatic focus
changes that their browser does not report through focus events. The owner window's
blur and document hiding dismiss the menu. Same-origin iframe menus bind their own
document/window; consumer frame lifecycle calls `dispose()` before retiring it.
`dispose()` is idempotent and removes observers/listeners/pending intent reset.

This one-level helper supplies no submenus, typeahead, checked/radio menu items,
modal focus trap, OS menu integration, cross-origin access or native-window
positioning. It allocates on the UI thread, without a deadline guarantee.

The [fixture](../crates/ravet-surface/native/tests/popup-menu.js) checks ownership,
focus/navigation, exact-once actions, disabled traversal, callbacks/disposal,
pointer-intent cancellation, attribute restoration, bounds and iframe behavior.
Hidden macOS WebKit runs it without creating a native window:

```sh
cmake -S crates/ravet-surface/native -B target/web-editor -DRAVET_WEB_EDITOR_TESTS=ON
cmake --build target/web-editor --target ravet-web-popup-menu
ctest --test-dir target/web-editor -R '^ravet-web-popup-menu$' --output-on-failure
```

Actual Chromium trusted key cases confirm disabled-entry skipping, one Enter
activation, Escape restoration and Tab exit. The automation focuses then blurs its
hidden window around each key, so the fixture explicitly reopens between key cases
and records keyup before the expected blur dismissal. `document.hasFocus()` is false
outside routing. These are DOM contract observations, with synthetic event cases
identified in the fixture; they do not certify visual native-window focus,
screen-reader announcements or other browser/backend accessibility support.
