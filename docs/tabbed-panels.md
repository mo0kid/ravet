# Local labelled tabbed panels

`RavetControls.TabbedPanels` composes consumer-created native buttons and panel
containers. It owns their local tab semantics and selection. Consumers supply
visible tab labels, panel contents, placement, styling and host transport. Load the
[shared controls asset](web-controls.md) first.

```js
const tabs = new RavetControls.TabbedPanels(document.querySelector('#settings-tabs'), [
  {tab: document.querySelector('#audio-tab'), panel: document.querySelector('#audio-panel')},
  {tab: document.querySelector('#midi-tab'), panel: document.querySelector('#midi-panel')},
], {
  label: 'Audio and MIDI settings',
  activation: 'manual',
  onActivate: ({index, reason}) => console.log(index, reason),
});
// Quiet consumer selection; retain current browser focus.
tabs.activate(1, {focus: false, notify: false});
// Retire the association before retiring its frame/window.
tabs.dispose();
```

The fixed profile is 1..256 distinct `{tab, panel}` pairs in one connected live
owner document. Each tab is a native `button` with explicit `type="button"`, inside
the supplied tab-list element. Panels are connected, outside that list and
nonnested with other supplied panels/tabs. Every tab and panel needs a stable
consumer-supplied document-unique ID, 1..128 UTF-16 code units without whitespace.
Preflight checks pairwise ID uniqueness and `getElementById` ownership; it does
not audit arbitrary duplicate IDs elsewhere in consumer markup. The explicit
list label is nonblank, at most 256 code units. Invalid profiles reject before
attributes/listeners change.

Options are `activation: 'manual' | 'automatic'` (default manual),
`orientation: 'horizontal' | 'vertical'` (default horizontal), `rtl: boolean`
(default false), `wrap: boolean` (default true), initial in-range integer `index`
(default zero), and `onActivate: function | null` (default null). An unavailable
initial index falls back to the first eligible tab, quietly. No eligible tabs
selects null and hides every supplied panel.

Manual arrows/Home/End move roving focus without changing panel selection. The
focused unselected tab becomes the single tab stop. Native Enter/Space/click
activates it once. Automatic mode activates on changed tab focus; a following
native click on the same tab does not duplicate notification. Disabled, hidden,
inert, ARIA-disabled and CSS-hidden tabs are skipped. Modified navigation remains
available to the consumer/browser. Tab uses native browser navigation to the
selected panel and its contents; there is no focus trap. Panel containers receive
`tabindex="0"`; their descendants remain consumer-owned.

`activate(index, {focus=true, notify=true})` rejects unavailable indices,
nonboolean options, reentry, disposal and invalid associations. `focus: false`
retains native focus and the existing roving position. Changing selection hides
the previous panel; consumers choosing quiet unfocused updates own any focus
inside that retiring panel. Selecting the same panel is a notification no-op,
although requested focus is attempted. `selected()` returns the current index,
or null when none/disposed. `refresh()` returns eligible indices and preserves an
eligible selection, repairing owned panel visibility. Otherwise it selects the
first eligible tab or null and emits one eligibility notification. If the retiring
selected panel contains focus, refresh transfers it to the replacement tab when
one exists. External focus is preserved; no eligible replacement leaves browser
focus fallback to the browser.

Callbacks receive frozen `{index, tab, panel, reason}`; `reason` is `activate`,
`focus` or `eligibility`. Null selection has null tab/panel. Attributes and panel
visibility are coherent before notification. Selection commits before optional
focus: focus handlers that remove/dispose the association cause a false return
and cancel notification; selection is not rolled back if focus fails. Reentrant
selection/refresh is rejected throughout focus and notification; disposal remains
legal. A throwing callback retains committed selection, returns false and reports
`lastError() === 'Callback'`. No automatic host acknowledgement or rollback is
implied. Successful operations clear that diagnostic. Focus/notification handlers that redirect
required focus or change selected-tab eligibility reject stale success; invalid
associations dispose immediately after notification, including throwing callbacks.
Eligibility changes can require the next observer checkpoint or another refresh
to choose a replacement; no stale eligible list is returned.

The widget temporarily owns list role/orientation/label (removing its original
`aria-labelledby`), tab role/selection/control links, panel role/labelled-by links,
hidden state and panel tabindex, and roving tab tabindex. Dispose restores the
exact original attribute values/presence and removes listeners/observer. Consumers
must not mutate owned attributes while active. Original-document observation
handles disable/hidden/inert/ARIA/class/style changes. Explicit refresh handles
stylesheet/media changes. Removal, adoption, changed IDs/button types or invalid
fixed membership/nesting disposes the whole association and restores attributes,
even if nodes are detached. Consumers dispose before hiding/removing their owner
iframe/window. Source membership is fixed; construct a new widget for new pairs.

The [contract fixture](../crates/ravet-surface/native/tests/tabbed-panels.html)
covers DOM semantics, bounds, directional selection, lifecycle, reentry and
restoration in Chromium and hidden WebKit. Trusted Chromium keys establish manual
focus versus activation, automatic selection without click duplication and native
Tab order. The hidden preview reports `document.hasFocus() === false` outside key
routing, so focus-listener regressions explicitly dispatch a focus event after
native focus. This is local browser evidence, not native-window focus or
screenreader validation. No dynamic membership, lazy content loading, native OS
tabs, global modal routing, layout engine, shadow-root focus engine or real-time
deadline is claimed. See [ADR0192](decisions/0192-local-tabbed-panels.md).
