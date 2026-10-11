# Explicit native semantic companion

`ravet-webview::accessibility` provides checked copied descriptions for custom
editor content. On macOS12 or later, `WebView::set_accessibility` and the same
method on `StandaloneEditor` publish a native AppKit accessibility hierarchy.
This is an opt-in complete alternate content hierarchy: automatic WebKit content
traversal is replaced until `set_accessibility(None)` restores it. Cover the whole
editor content when opting in. Arbitrary HTML is not inspected or converted.
The standard title-bar Audio/MIDI popup remains outside this content hierarchy.

The framework owns real `NSAccessibilityElement` subclasses with public button,
slider and static-text protocols, copied role/label/value metadata, native screen
frames, navigation order and bounded hit-testing. Button press and discrete slider
increment/decrement intents go to `Callbacks::accessibility_action`, whose default
is refusal. Value/focus setters and every other action selector are unsupported.

## Model and limits

`AccessibilityFrame::new` checks finite top-left local logical rectangles. Origins,
extents and far edges are within +/-16384; width/height are positive. AppKit derives
screen frames from the actual attached view/window, including original flipped
coordinates. Consumers supply actual geometry; page CSS/transforms/device scale
are not guessed. The example obtains bounded rectangles from its own trusted page.

`AccessibilityNode::new` copies a nonzero u64 ID,1..1024-byte UTF-8 label, frame,
enabled/hidden availability and `AccessibilityKind`. Static text owns0..4096-byte
text; button has no value; slider owns finite minimum/value/maximum/increment,
with minimum <= value <= maximum and increment >0. Slider steps are consumer-owned;
the native companion sends discrete intent and never silently clamps host values.
Labels/text cannot contain NUL. Visible disabled nodes remain readable but refuse
actions. Hidden nodes are excluded from native child/navigation order.

`AccessibilitySnapshot::new` checks1..64 distinct nodes and at most65536 combined
panel-label/node-label/static-text UTF-8 bytes, then copies them. Its optional
focused ID must name an enabled/unhidden button or slider. It reports consumer
focus; it does not move OS/page focus or make a window key. The model is flat;
tree/table/editable-text/range selection and arbitrary native roles are excluded.
Construction and publication allocate on UI/worker threads, never on audio.
The declared bounds do not bound AppKit/WebKit/global layout or allocator work,
notification delivery or elapsed time; general process allocation failure is not
a recoverable result guarantee.

## Publication, intent and owner lifetime

Installation requires the Cocoa UI thread and an attached original native parent
and window. Invalid complete input preserves the previous hierarchy. Each accepted
full publication receives a new editor-local `AccessibilityRevision`; a revision
number is not an authorization token or an identity shared across editors. Clearing
at revision exhaustion is still permitted without creating a new revision. The
native owner rejects exhaustion before publication. Callbacks/notification
transitions reject reentrant installation with `Busy`; publish after callback
unwind in the consumer's UI loop. Standard layout/value/reported-focus notification
posts follow complete publication. A post does not prove delivery to any client.

Native actions check the original attached owner, revision, role, enabled/hidden
state and nonbusy transition. The original window and ancestor chain must be
visible; ancestor inspection is capped at32 and deeper owners are unavailable.
Slider endpoints send no intent. Callback refusal, caught Rust panic or recursive
borrow/action returns false. An accepted action means the consumer accepted intent;
it does not imply a host edit, new value or visual acknowledgement. Consumers own
their parameter begin/edit/end failure recovery. Publish the accepted model later
and acknowledge it independently through the existing WebKit snapshot transport.
Accepted consumer effects cannot be rolled back if a callback hides/closes or
destroys its editor; no later native presentation write revives retired ownership.

Each publication has new native element objects. A client retaining an old object
can still read its immutable role/label/value, but ownership-dependent parent/frame/
focus/action results become nil/zero/false. Reusing an ID in a later snapshot does
not reactivate an older object. Parent references are weak, avoiding a retained
container/child cycle. Native teardown retires callbacks before releasing the Rust
context; a retained context protects synchronous callback self-destruction.

Detach/reparent makes the original installation unavailable before new actions.
Ordinary hide/reveal is not terminal and may reveal the admitted hierarchy again.
A RAVET standalone closed owner is terminal; its borrowed editor rejects new
publication, while `None` remains original-owner cleanup. Non-RAVET host windows
follow their own visibility/attachment and explicit consumer clearing policy;
RAVET does not infer a monotonic host close lifecycle. Ownership-dependent direct
protocol calls off the Cocoa main thread return inert results without dispatching
or waiting. No implicit global focus/show, trust/settings/private API or host
process inspection is performed.

## Consumer and evidence

Run `cargo run -p ravet-examples --bin native_semantics`. The custom output control
shares one Rust model between its visual WebKit control and explicit native status,
slider and reset-button nodes. Normal page keyboard messages and native actions
use that same model; geometry and source bounds are checked before publication.
The normal title-bar settings popup is enabled. This reference has no physical
audio routing or parameter-host adapter; those are independent consumer concerns.

Run `python3 scripts/check-native-semantics.py` on macOS for the explicit owned
fixture (optional `--output` and `--runner`, no shell). It compiles native sources
and two sequential single-file Rust fixtures independently of Cargo dependencies.
The reproducible own-window fixture exercises actual PUBLIC AppKit protocol
getters/actions, whole preflight, stale objects, callback reentry/teardown, measured
native frames, normal WebKit message transport and the standard popup. Its injected
AppKit key is a fixture-generated event, not a human or screenreader gesture.
Native direct-protocol evidence is distinct from DOM assertions and from system
AX client discovery. Earlier remote-WebKit content AX was unavailable in the owned
inspection profile; this companion supplies an explicitly authored alternative,
not a repair or validation of that remote content. System AX observer/client
discovery, VoiceOver speech/navigation, actual assistive technologies, native
global focus and other platforms remain unvalidated.

Apple's [custom-control guidance](https://developer.apple.com/documentation/appkit/custom-controls),
[NSAccessibilityElement](https://developer.apple.com/documentation/appkit/nsaccessibilityelement-swift.class)
and [public protocol/notification guidance](https://developer.apple.com/library/archive/documentation/Accessibility/Conceptual/AccessibilityMacOSX/ImplementingAccessibilityforCustomControls.html)
define the native role, parent/children/frame and notification contracts used here.
