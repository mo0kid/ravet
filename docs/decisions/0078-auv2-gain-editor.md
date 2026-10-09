# 0078: Optional actual-component AUv2 Cocoa editor

## Context

The optional VST3 editor requires actual AU Cocoa protocol validation; compiling
an SDK wrapper alone does not establish controller/edit/geometry integration.

## Decision

Add explicit native-editor AU bundling that embeds the editor-enabled VST3 and
runs an actual component AUCocoaUIBase host probe. Reuse the external SDK's view
mapping without copying/modifying SDK source. Keep default DSP-only bundling.
Native test helpers find the owned WebView recursively through the AU view layer.
Hosts own AppKit/run loop, live unit/parent lifetime and view teardown.

Exercise notifying AUParameterSet for host changes, plus all-parameter listener
notification after bulk preset recall. Raw parameter setters do not notify this
UI path; stale listener caches otherwise suppress a later same-value change.
Exercise setFrame, which the SDK maps to IPlugView onSize. The inherited
setFrameSize path did not forward in this probe and remains unclaimed.

## Consequences and validation

The embedded page and Rust codecs remain shared; the SDK emits AU gesture/value
notifications and queues audio parameter changes. Existing host-bypass preservation
across presets remains unchanged. Meter delivery, generic AU registration, AUv3,
DAW/focus/accessibility and other architectures remain separate work.

Actual component checks pass three gain/bypass editor cycles verified in rendered
f32 audio, host/preset refresh and subsequent edits, resizing and teardown. The
first fixtures used non-notifying setters/preset caches and an unforwarded resize
path; correcting the host protocol fixed them. A hidden window snapshot was flat;
showing the owned disposable window produced readable native pixels. All views
are released before unit disposal and the probe closes its own window. Separate
component contracts and VST3 SDK validator regressions pass.
