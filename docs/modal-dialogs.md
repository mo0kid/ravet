# Local native modal dialogs

`RavetControls.ModalDialog` wraps a consumer-created native `<dialog>` with local
ownership, labelled opening, bounded completion values and teardown. The browser
supplies modal/top-layer behavior and focus restoration through its
[native dialog API](https://html.spec.whatwg.org/multipage/interactive-elements.html#the-dialog-element).
Load the [shared controls asset](web-controls.md) first. Consumers own contents,
button actions, presentation and host acknowledgement.

```js
const element = document.querySelector('#settings-dialog');
const modal = new RavetControls.ModalDialog(element, {
  label: 'Audio settings',
  initialFocus: element.querySelector('input'),
  onClose: ({value, reason}) => console.log(value, reason),
});
element.querySelector('[data-confirm]').addEventListener('click', () => modal.close('accepted'));
modal.open();
```

Construction requires a connected, initially closed native dialog and a connected,
focus-capable descendant in the same live original document. The focus target is
fixed and distinct from the dialog. Native `showModal`/`close` and `:modal` selector
support are required, with no substitute modal engine. The explicit label is
nonblank and at most 256 UTF-16 code units; `onClose` is a function or null. Two
live wrappers, or a new wrapper on a node whose native close is still draining,
reject before attribute/listener mutation. Consumers supply valid dialog contents
and labels. The wrapper temporarily owns `aria-label`, removes `aria-labelledby`,
sets `aria-modal=true` and restores exact originals plus original `returnValue` on
disposal. It does not add dialog tabindex or scan/bind action buttons.

`open()` resets native returnValue, admits at most one RAVET modal per document,
rejects another actual native modal in that document, calls `showModal`, then
focuses the configured eligible target. Missing/disconnected/adopted ownership,
failed native modality, disabled/hidden/inert/CSS-unavailable focus, redirected
required focus or synchronous disposal rejects success. A rejected opening closes
quietly; if native opening happened, admission stays reserved until its queued
close completes. Same-origin child documents have independent local ownership.
This is document modality, with no application-wide or nested modal stack.

`close(value='')` validates a scalar Unicode string of 0..128 UTF-16 code units
before closing; lone surrogates and oversized/nonstring values reject unchanged.
A true return means native closure was initiated synchronously, with completion
still queued. `isOpen()` reports current owned native modality. Live close retains
its document owner through native focus restoration and notification; reopen or
another modal is rejected during this transition. Frozen completion payloads are
`{value, reason}`, with reason `close`, `cancel` or `native`. Wrapper-issued values
are captured at request time. Direct native `dialog.close()` and `method="dialog"`
forms use the final native returnValue at completion: an invalid external value
closes without notification and reports `lastError() === 'Value'`.

Native Escape/cancel is prevented and routed to one empty-valued cancel closure;
already-prevented cancellation is respected. Explicit synthetic cancel events can
request this same local intent. Completion and drain require actual trusted native
close events; `dispatchEvent(new Event('close'))` cannot finish a transition. This
native-event distinction is lifecycle bookkeeping, not a credential or security
boundary for consumer code. Modified/unrelated keys, native Tab behavior and browser
focus boundaries remain browser-owned. There is no custom focus trap, backdrop
click dismissal or global event routing.

Notification occurs once after native closure. Reentrant wrapper operations are
rejected throughout focus restoration and callbacks; disposal is legal. Throwing
callbacks retain closure and report `Callback`. Native method failures report
`Native`; unsupported native re-opening/open-attribute mutation reports `Ownership`
and retires the association. Invalid association changes during notification retire
immediately. Successful opening/completion clears the prior diagnostic. No host
commit, Rust token, OS-window completion or automatic asynchronous acknowledgement
is implied.

`refresh()` rechecks the fixed ownership/eligibility profile. Original-document
observation detects removal/adoption, disabling, hidden/inert/ARIA, class/style
changes and native open transitions. Explicit refresh handles stylesheet/media
changes. Disposal cancels callbacks, disconnects normal listeners/observer and
closes owned native modality under the focus guard. Browser/window blur retains an
open dialog. Direct removal of `open` is unsupported: both tested engines can leave
native modality stranded; guarded native re-show/close repairs it. Previous-focus
restoration under this ownership violation is not guaranteed.

Disposal can leave one per-node native-close tombstone. `isDraining()` reports this
disposed-but-native-completion-pending state; it does not mean every live close is
draining. Another wrapper on the same node rejects until the actual close event
arrives. Synthetic close cannot release it. After synchronous native closure/focus
restoration and any executing completion callback finish, a different dialog in
the original document can open; an old drain cannot release that newer owner.
There is no guessed timeout or drain deadline. To reuse a node, keep it and its
owner document alive until drain completes. Removing a retired node or tearing
down its frame/window may abandon event scheduling; abandon that node rather than
reusing it. Disposal still restores attributes/cancels callbacks, and retired-node
drain does not block a different dialog. Consumers close/dispose before retiring
frames/windows; no deferred source operations or unbounded callback queue is kept.

The [browser/native fixture](../crates/ravet-surface/native/tests/modal-dialog.html)
covers actual native modality, outside-focus blocking, close/form/cancel order,
bounds, owner exclusion, duplicate/replacement drains, fake events, callback and
focus reentry, adoption and exact restoration in Chromium and hidden macOS WebKit.
Trusted Chromium Tab reaches native controls then the browser focus boundary;
ShiftTab returns to the dialog button and Escape closes once/restores its previous
outside control. The preview reports `document.hasFocus() === false` outside trusted
routing, and focus-restoration regressions explicitly dispatch a focus event after
native closure. Evidence does not certify native-window focus, assistive technology,
other backend/platform, browser event deadlines or global memory bounds. See
[ADR0196](decisions/0196-bounded-native-modal-dialog.md).
