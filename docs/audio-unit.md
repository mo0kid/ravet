# AudioUnit v2 gain reference

The macOS AUv2 component uses the external Steinberg AU wrapper and Apple
AudioUnitSDK, embedding the same Rust-backed VST3 gain reference. SDKs remain
outside this repository. This is an exercised reference adapter with an optional
Cocoa editor. The separate [AUv3/mobile adapter](mobile.md) uses the reusable native ABI rather than this SDK wrapper.

The separate public [AUv2 performance instrument](performance-au.md) uses
`--plugin performance` with bounded MusicDevice note input and complete Level
state; Gain remains the default profile and retains its host-bypass policy.
Reusable [native AU schema/control/state helpers](native-au.md) supply its exact
parameter preflight and CONTROL-owned CF text/state validation.

## Build and validate

```sh
CARGO_BUILD_JOBS=2 python3 scripts/bundle-au.py \
  --vst3-sdk /path/to/vst3sdk --audio-unit-sdk /path/to/AudioUnitSDK
# Optional embedded editor and actual component Cocoa-view checks:
CARGO_BUILD_JOBS=2 python3 scripts/bundle-au.py \
  --vst3-sdk /path/to/vst3sdk --audio-unit-sdk /path/to/AudioUnitSDK --native-editor
```

The script builds the external Apple static library in Release for the current
macOS architecture with deployment target 12.0, then builds and ad hoc signs the
component under `target/vst3/release/AU`. It includes SDK and RAVET licence notices.
No normal plug-in directory is modified. The component contains `plugin.vst3`
as a resource; both binaries must be distributed together. Only macOS arm64
with Xcode 27 and the installed SDKs has been exercised.

The validation executable loads the actual component, registers its factory
inside that process, instantiates it through AudioComponent APIs, and checks
stereo 48 kHz noninterleaved f32 rendering, gain smoothing, dry bypass, state
recall and untouched buffer tails. This requires no audio device or global
component registration. VST3 contracts and its official validator also run.
This evidence does not establish DAW discovery, auval, other architectures,
sandbox execution, callback deadlines or native allocation behavior.

## Parameter and state semantics

The component identity is effect `aufx`, subtype `RvGn`, manufacturer `Ravt`.
Mono and stereo configurations are advertised. Gain ID 1 uses normalized 0..1
AU API values: unity is 0.25, twice amplitude is 0.5; the SDK supplies physical
value display conversion. Rendering uses the shared prepared f32 gain DSP and
its 5 ms smoothing. The underlying [VST3 reference](vst3.md) defines its bounds.

The host bypass property controls dry output. AUv2 preset recall deliberately
preserves the current host bypass: the official wrapper restores processor
state, then queues the current bypass value over the stored value. This differs
from the complete gain/bypass VST3 and CLAP state contracts. Tests explicitly
assert restored gain and preserved host bypass. Consumers needing independent
preset bypass must define a separate automatable parameter and contract.

The optional Cocoa editor maps the embedded VST3 IPlugView through the external
SDK wrapper, as described below. Hosts may provide their generic parameter UI.
Instrument/MIDI ports, f64,
auxiliary buses and commercial signing remain pending for this AUv2 path. See
[decision 0022](decisions/0022-auv2-reference.md).

After the shared processor gained native VST3 f64 support, the AUv2 component
was rebuilt and its actual-bundle f32 parameter/audio/bypass/state/tail contracts
passed again. This validates the existing AU path; the VST3 sample-size declaration
does not establish AU f64 negotiation or rendering.

## Optional Cocoa editor

`--native-editor` embeds the Rust-owned gain/bypass/status WebKit page in the
nested VST3 and validates the component's actual `kAudioUnitProperty_CocoaUI`
class through `AUCocoaUIBase`. Default bundling remains DSP-only. The external
SDK owns AU-to-VST3 view/controller and AU parameter gesture notifications; no SDK
source is copied or modified. The shared [VST3 editor](vst3.md) owns its control
lifecycle and Rust UI codecs, including four input/output meters. The SDK's
prepared output-parameter transfer drains on its main-thread timer and updates
AU parameter storage/listeners and the embedded controller. This provides the
same twelve normalized peak/RMS/diagnostic triples (IDs 10..21) described in
[VST3 metering](vst3.md); metadata marks them readable without writable flags.
The wrapper may expose their AU parameter metadata regardless of the VST3 hidden
flag. Levels saturate at four for display; audio/native f64 meter precision is
unchanged. Host bypass can skip processor rendering and retain the previous
meter display. Empty/fault/stopped render paths also retain previous readings.
Timer/run-loop delivery is required, with per-parameter rather than complete-block
UI atomicity. Broader widget/platform/DAW validation remains separate.

Hosts supply the AppKit main thread/run loop and retain the AudioUnit while its
views live. Use `AUParameterSet` for notifying external UI changes; a raw
`AudioUnitSetParameter` alone does not notify this listener path. After bulk
preset recall, notify `kAUParameterListener_AnyParameter` through
`AUParameterListenerNotify` so listener caches receive current values, including
values needed by subsequent edits. Rendering still uses ordinary AU parameter
queues. Host bypass preservation across preset recall remains unchanged.

The exercised resize path is Cocoa `setFrame:`, which the SDK maps to IPlugView
`onSize`; inherited `setFrameSize:` did not reach that mapping in the probe and
is not claimed. Shared view limits are 360×300 through 1200×900 logical pixels.
Consumers own their host geometry integration and view teardown before disposing
the AudioUnit. The component creates no device or host window automatically.

On macOS 27.0.1 arm64/Xcode 27, an actual loaded component passes three custom
Cocoa view cycles with gain/bypass edits verified in f32 rendered audio,
notifying external host refresh, complete preset recall/refresh, subsequent edits
and `setFrame:` resizing. Output transfer reaches all four displayed RMS meters;
metadata/normalized values, gain-dependent outputs and injected nonfinite
diagnostics/recovery pass against the actual component. A shown disposable host window produces a readable
native screenshot; a hidden AU host window had returned a flat snapshot despite
working DOM checks. The probe closes its own window afterward. Earlier failures
from non-notifying setters/preset caches and `setFrameSize:` are retained in work
status. Separate component audio/state/bypass/tail and VST3 validator contracts
also pass. This does not establish DAW discovery, auval, focus/assistive-tech,
sandbox or other-architecture support. See
[decision 0078](decisions/0078-auv2-gain-editor.md).

## Optional factory programs

Add `--factory-programs` to enable Unity/Silence/Bypass factory presets from the
embedded VST3 catalogue. It combines with `--native-editor`; both options default
to disabled. A RAVET-owned AU subclass corrects count-minus-one selector mapping,
queues complete program controls and synchronizes the AU host bypass property.
External SDK sources remain unchanged; only their factory symbol is renamed
in this target. The local factory uses ordinary AUBase effect dispatch, with an
explicit ClassInfoFromDocument restore path; instrument/music/ProcessBufferLists
extensions are not advertised.

Factory preset indices are 0..2. Parameter ID 3 accepts finite normalized 0..1
at block start only, snapping nearest with ties upwards; nonzero sample offsets
are rejected. Explicit factory recall updates both gain and host bypass, whereas
ordinary saved-state/document recall retains the SDK's host-bypass preservation.
Negative custom-preset numbers retain AUBase metadata semantics and do not select
a factory sound. Control state remains the persisted record, rather than a
stable selected program identity. The custom page still has gain/bypass/status.

Actual component registration and three disposable lifecycles exercise names,
repeated/reversed selection, selector endpoints/ties, custom metadata, invalid
selection preservation, rendered controls/bypass and document restore. Default
build, audio/state/tail and optional editor regressions run separately. This is
macOS arm64 in-process reference evidence; DAW preset menus, auval, other
architectures, instruments and sample-offset AU program automation remain
unvalidated. See [decision 0085](decisions/0085-auv2-factory-programs.md).

See [decision 0100](decisions/0100-auv2-meter-transfer.md) for the scoped AU
meter transfer evidence. It does not establish real-time timer delivery or DAW
meter forwarding beyond this owned synthetic host.
