# 0286: Keyboard focus for web-editor text fields in hosts that keep key status

Status: implemented in the Cocoa/WebKit backend; macOS AppKit fixture evidence.
In Pro Tools 26.4 (owner, 5 November 2026) characters reached the field but
Return, Enter and Escape did not, and clicking away did not commit; the
amendment below addresses both and awaits a Pro Tools re-check.

## Context

In Pro Tools 26.4 (macOS arm64) the delay's typed-value field could not be
typed into: Pro Tools kept key input for its own shortcuts, while mouse editing
worked. The same editor accepts typing in Ableton Live (VST3) and Logic (AUv2).
AAX SDK 2.8 offers no keyboard-focus call: `AAX_IViewContainer` has view type,
pointer, modifier query, size requests and parameter mouse-event forwarding
only, and `AAX_IEffectGUI`/`AAX_CEffectGUI` have no key handling.

## Decision

Handle it in the shared Cocoa web view for every format, not in the AAX adapter
or the plug-in table:

- A document-start script reports to native code, through a separate
  `ravetKeyboard` message handler, whether the focused element accepts typed
  text (text-like `<input>`, `<textarea>`, `contenteditable`; not buttons,
  checkboxes, ranges and the like). It reports changes only, including a field
  removed while focused.
- When a text field gains focus and the editor's window is not key, the web
  view asks for it (`makeKeyWindow`) and becomes first responder. When the
  field loses focus, key status goes back to the window that had it, but only
  if the editor took it and still has it.
- If the window still is not key (the host refuses key status), a local AppKit
  event monitor forwards key-down and key-up events without the Command key to
  the web view while the field has focus. Command-key shortcuts stay with the
  host. A mouse press in another window, or the application resigning active,
  ends this and blurs the field.

Nothing changes in hosts whose editor window is already key when a field is
focused (Logic, Live): no key-window change, no monitor. The editor protocol,
the injected editor client and `RavetPlugin.h` are unchanged.

## Evidence and limits

[`keyboard-focus.mm`](../../crates/ravet-surface/native/tests/keyboard-focus.mm)
models three hosts with real AppKit windows: a panel that becomes key only if
needed, a window that refuses key status, and an ordinary key window. It
checks typing, Command keys, restoring key status after blur and after removal
of a focused field, and the click-elsewhere release. The fixture fails against
the previous backend. It does not establish how Pro Tools routes key events:
if a host consumes key events before AppKit's local monitors run, typed input
still cannot reach the editor. Forwarded typing in a refusing window carries no
Command-key editing shortcuts (copy, paste, select all). Windows and Linux
backends are unchanged.

See the [native editor guide](../native-editor.md#keyboard-focus) and
[AAX](../aax.md).

## Amendment (5 November 2026)

Pro Tools takes Return, Enter and Escape in its own event handling even while
the editor window is key, and WebKit keeps a field focused when its window
stops being key, so the page never sees a blur. While a field has focus, the
web view now always installs its local key monitor and sends every key without
the Command modifier straight to the web view (the monitor runs before the
application dispatches the event), and it ends editing, blurring the field, when
the editor window resigns key status, as well as on a click in another window or
the application resigning active. Command shortcuts still reach the host. A new
fixture group (a window whose `sendEvent:` swallows Return, Enter and Escape)
fails on the previous code and passes with this change.
