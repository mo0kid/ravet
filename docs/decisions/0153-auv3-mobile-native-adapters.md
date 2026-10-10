# 0153: AUv3 and mobile native adapters

## Decision

AUv3 and mobile application backends reuse the optional native processor,
parameter and live-state entries. Portable processors retain no SDK, device or
UI dependencies. The native session owns one paired f32 processor/publication;
complete control edits and recall publish outside audio ownership. Rendering
explicitly applies publication before processing and conditionally reflects
afterward. SDK registration, stream ownership and UI remain native adapters.

AUv3 uses an `AUAudioUnit` subclass rather than the AUv2/VST3 SDK wrapper. The
public Gain extension embeds a programmatic CoreAudioKit parameter editor and
has a separate identity (`aufx/RvG3/Ravt`). Its containing app owns registration
and distribution. It never creates devices. Native parameter values remain
physical, and saved Gain state includes bypass. Persistent ramps continue across
render calls using prepared bounded storage; an active ramp processes individual
samples without expanding a whole ramp into allocated event storage.

Apple standalone effects use `AVAudioEngine` and app-owned `AVAudioSession`,
permissions, interruption/route handling and explicit MIDI input selection.
Android standalone effects use AAudio, foreground microphone permission and
audio focus, explicit devices and a bounded input MIDI queue. Error callbacks
publish status; stopped control owns stream teardown. The mobile fixtures expose
Audio/MIDI settings by default. Embedded AU editors expose parameter controls
and leave device routing with their host.

Consumers own their device/OS/route and AUv3 host validation matrix; there is no
fixed hardware or host allowlist. Build deployment choices are explicit options,
with adapter API minimums checked before compilation. Selected targets remain
plans until runtime checks establish evidence in the consumer project.

## Implementation status

The current native platform profile is f32, one main bus, at most 128 global
parameters and 64 channels per direction. The public effect supports mono/stereo.
AU input flattening reserves slots for active ramps, rejecting blocks with more
than `1024 - parameter_count` events. Short MIDI and framed UMP require an explicit
subclass profile; the public Gain effect has no MIDI input profile. Generated
event routing, auxiliary buses, factory programmes, SysEx, precision selection,
and a native instrument reference remain further work.

Apple mobile builds default to arm64 iOS/iPadOS 17 and arm64 simulator; the macOS
fixture defaults to macOS 12. Consumers may raise those deployment minimums.
Android builds default to arm64/API 26 and package with SDK/build-tools 36,
JDK 17 and NDK r29. Consumers choose minimum API, compile/target SDK and installed
build-tools; minimum API cannot go below 26, and compile SDK must be at least 36
and the target SDK. Rust/CMake and Android packaging share the selected minimums. Preferred device configuration may
negotiate differently; actual values are shown. Android duplex streams must
negotiate equal rates/channel counts; this adapter has no independent-clock
conversion. App backgrounding stops audio. Apple engine reconstruction retains complete state and reconnects an available
selected MIDI endpoint by its unique ID; Android failure recovery requires Start.

## Evidence and limits

MacOS arm64 and iOS 26 simulator in-process AU contracts exercise audio,
parameters, state, timed automation, persistent ramps and lifecycle. The iOS app
launches with visible native controls. Android native code cross-compiles and its
Java source compiles; a scripted AAudio double tests ownership, chunking, shortage
silence, error publication and bounded MIDI delivery. Build/signature checks
establish artifacts, rather than execution on an Android device.

App-extension discovery/IPC, third-party hosts, physical audio/MIDI routes,
interruptions/hotplug, Android runtime, accessibility interaction, signing and
store distribution need their own evidence. Offline, simulator and scripted
backend tests do not establish full mobile support or real-time deadlines.
