# ADR 0189: Local control ownership belongs to the owner document

Status: implemented within the local DOM contract

## Decision

Store parameter gesture ownership in a WeakMap keyed by the control's original
owner document. Validate connected controls, their live owner window and any
paired numeric input's same-document membership before setting attributes or
registering listeners. Keep each slider/knob's pointerup/cancel/blur and visibility
listeners on that original owner window/document. Use that document's active
selection to suppress host refresh while editing.

Numeric commits, toggles, choices and actions finish only their document's owner.
Rejected end notifications retain ownership and listeners for an explicit accepted
retry, including rejected disposal. Natural focus blur still finishes gestures;
per-document ownership does not assert simultaneous keyboard focus across frames.

## Consequences and evidence

Same-origin embedded documents work without binding parent global listeners or
cross-document forced gesture termination. Consumers must finish/dispose controls
before retiring their owner document or frame, including retrying rejected ends.
There is no cross-origin access, automatic stylesheet/frame retirement, shadow-root
focus engine or native-window/assistive-technology certification. Isolated actual
Chromium and hidden WebKit fixtures check independent owners, child focus refresh,
child-window lifecycle events, rejected end/disposal, foreign paired input rejection
and missing live-window preflight. See [web controls](../web-controls.md).
