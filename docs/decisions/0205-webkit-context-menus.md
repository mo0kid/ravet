# 0205: WebKit browser context menu suppression

Status: implemented in Cocoa/WebKit and Linux/WebKitGTK; macOS synthetic DOM
validation only, Linux runtime validation pending.

## Decision

Install a capturing, non-passive `contextmenu` listener at document start in all
frames for both WebKit backends. Cancel the browser's default menu without
stopping propagation or cancelling mouse/pointer events. The shared creation
path covers embedded editors, standalone windows, local files, inline pages,
child frames and subsequent document loads.

Browser menus interfere with consumer right-click reset and custom-menu actions.
Consumers still receive those events and can perform their own action; their
context-menu handlers observe `defaultPrevented`. This is a trusted-page default,
not a security boundary against consumer scripts. No native opt-out is exposed
in this change. Windows/WebView2 retains its existing policy.

## Evidence and limits

The [macOS probe](../../crates/ravet-surface/native/tests/context-menu.mm) runs a
[local fixture](../../crates/ravet-surface/native/tests/context-menu.html) through
file and inline creation and repeat loads. Synthetic DOM events check
document-start cancellation, child frames, pointer/keyboard-style context-menu
events, consumer right-click reset/custom-handler propagation and left clicks.
This does not certify actual OS menu presentation, host routing, assistive
technology, Linux runtime behavior or DAW compatibility.

See the [native editor guide](../native-editor.md) and implementations:
[Cocoa](../../crates/ravet-surface/native/RavetWebView.mm),
[WebKitGTK](../../crates/ravet-surface/native/RavetWebViewLinux.cpp).
