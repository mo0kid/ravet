# Owned standalone window closure

On macOS, `StandaloneWindow::state()` returns a copied
`StandaloneWindowState { closed, visible, key, miniaturized }`, or `None` if the
exact owned main-thread native window could not be read. It neither shows nor
activates, minimizes or changes focus. A hidden live window has `closed=false`
and `visible=false`; the existing `pump` visibility Boolean cannot distinguish
it from a closed window by itself.

`closed` is monotonic local close admission. The framework-owned NSWindow
subclass sets that ivar before superclass closure emits native notifications.
During a synchronous will-close notification, the other flags remain actual
Cocoa observations and can still reflect pre-close values. After the close
call returns, the native window is no longer visible. This does not promise a
particular global key-window or focus transition.

`request_close()` invokes this exact window's native `performClose:` action.
It returns true on actual admitted closure, false on native veto, unavailable
owner or synchronous operation reentry. Outside a busy resize/policy/close
operation, an already-closed request returns true without another notification.
The shared Rust operation guard covers wrapper calls; a native closing ivar
also refuses nested requests during externally invoked close notifications.
No public delegate, arbitrary notification callback, process quit, show/hide,
fullscreen or global focus API is introduced.

After terminal admission, new embedding, content measurement, resize, policy
mutation, native settings activation, evaluation and snapshot calls reject.
`content_size` returns None; copied state and the last admitted constraints remain
readable. Borrowed editor visibility becomes a no-op. Embedding rechecks after
native editor creation and attachment; a synchronous closure prevents returning
a usable new editor. Native effect guards stop subsequent owned writes after
foreign callbacks close the window. Already-admitted foreign effects cannot be
rolled back; a refused result during such closure is not transactional rollback.

Closure retains the native parent for an already-borrowed editor. It does not
destroy that editor, cancel its snapshot timer/application callbacks or stop an
audio engine. Drop the editor, then the window. Already-admitted JavaScript can
still finish; this is not a general WebKit cancellation API. Pending native
snapshot completion retains the native window object, checks closure before file
output and captures no raw framework host pointer. Window destruction calls
direct native close, bypassing veto for cleanup; the native subclass makes this
a no-op if closure was already admitted.

The default title-bar Audio/MIDI settings configuration and hidden process-local
preferences remain unchanged. After closure even a direct action on that window's
retained settings button is a no-op.

## Reproducible owned-window validation

```sh
python3 scripts/check-window-lifecycle.py
```

The explicit fixture creates only its own hidden windows, opens no devices and
uses synthetic local HTML plus the default popup. It compiles actual framework
Rust/native sources with warnings denied; `--output` selects its build/artifact
directory and optional `--runner` supplies argument-parsed executable wrapping
without a shell. macOS 12+, Rust and Xcode command-line tools are required.

Five owned hidden windows pass 15 groups: hidden-live versus closed state,
default no-device popup, synthetic will-close notification without terminal
retirement, native veto without closure, wrapper and native `performClose:`
admission before notification, nested close/resize/policy refusal and idempotence,
retired JS/popup/embed/size/policy/snapshot/view-visibility attempts without
effects, exact callback-context destruction and child-before-parent cleanup.
Additional destructor cases prove direct close bypasses veto and already-closed
destruction emits no second notification. Rejected snapshots preserve an owned
sentinel file; a missing native content view rejects embedding without panic,
leaked child or callback context. Private fixture inspection verifies the retained page did not
execute rejected JavaScript or reopen its popup.

Fixture notification tokens are exact-window scoped and removed before their
borrowed contexts are dropped. Destructor-only counters retain the native window
independently and never access the destroyed Rust/framework host. Initial lookup
examines at most eight own-process windows and a unique 1..256-byte title;
fixture-only retained-page traversal caps depth 32, 256 nodes, eight children per node and
4096-byte scripts. These caps bound fixture work, not arbitrary Cocoa/WebKit
layout cost or an event deadline.

These are own-window Cocoa action/notification and DOM/resource observations.
Native `performClose:` is a synthetic owned-window action, not a human titlebar
click. Hidden fixtures establish no foreground/key/minimization transition,
screen-reader/native AX, hardware, device, DAW or other-platform certification.
The public lifecycle API remains macOS-only.
