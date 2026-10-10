# AUv3 and mobile native adapters

RAVET has reusable native AUv3, iOS/iPadOS standalone and Android standalone
**prototypes**, with public Gain fixtures. These adapters reuse the
[native processor/parameter/live-state entries](native-processing.md). They do
not establish full mobile or third-party host support: the actual evidence and
remaining capabilities are listed below and in [work status](WORK_STATUS.md).

## Platform profiles

| Adapter | Build profile | Current behavior |
|---|---|---|
| AUv3 | macOS 12+, iOS/iPadOS 17+, Apple arm64/simulator | Prepared noninterleaved f32 main bus, global physical parameters, timed changes and continuing ramps, complete live state, host bypass, cached latency/tail, transport and opt-in short MIDI/framed UMP input |
| Apple standalone | iOS/iPadOS 17+, arm64/simulator | AVAudioEngine microphone effect, AVAudioSession configuration, interruption/route/reset rebuild, retained state, explicit CoreMIDI input |
| Android standalone | Android API 26+, arm64-v8a | AAudio microphone/output streams, actual configuration diagnostics, bounded immediate MIDI input, foreground permission/audio focus and stopped stream retirement |

The native session accepts up to 128 global parameters and one main bus with
0..64 inputs/1..64 outputs. Public Gain supports matched mono/stereo only and has
no MIDI input profile; its standalone MIDI selector becomes usable with a native
processor subclass that explicitly accepts MIDI. Processing/reset use prepared
storage; allocation, locks, file access and stream setup belong to control.
Normal native callback timing/allocation guarantees require actual measurement.

## Consumer choices

Consumers choose their own iPhone/iPad and Android models, OS versions, audio/MIDI
routes and AUv3 hosts. RAVET imposes no named device or host allowlist. Standalone
users select available devices and preferred audio settings through the apps;
embedded AUv3 routing belongs to the selected host.

Keep a validation matrix in the consumer project with device model, OS version,
AUv3 host/version (where applicable), audio/MIDI route, requested and negotiated
settings, checks performed and results. Exercise playback/capture, state recall,
automation, route changes, interruption/recovery and foreground lifecycle on the
chosen targets; AUv3 also needs extension discovery and host loading. A chosen
target is a test plan until those checks run. Consumers can release for their own
validated targets without requiring a project-wide list of phone models or hosts.

The build script accepts consumer deployment choices within the adapter's API
minimums. Defaults remain those in the table above; architectures remain arm64.
SDK selection and minimum OS settings do not establish device compatibility.

```sh
python3 scripts/build-mobile.py ios --deployment-target 18.0
python3 scripts/build-mobile.py android --ndk /path/to/android-ndk-r29 \
  --android-sdk /path/to/android-sdk --java-home /path/to/jdk17 \
  --android-min-api 28 --android-target-sdk 35 --android-compile-sdk 36 \
  --android-build-tools 36.0.0
```

Apple minimums are macOS 12 and iOS/iPadOS 17; the selected deployment target is
passed to both Rust and CMake. Android minimum API must be at least 26, target SDK
at least the minimum, and compile SDK at least 36 and the target SDK. The Java
fixture requires compile SDK 36 even when its deployment minimum is lower.
Android minimum API is shared by the NDK compiler, CMake, DEX and APK manifest;
compile/target SDK and build-tools select the installed SDK and manifest target.
Missing SDK/compiler versions fail explicitly; no tools are downloaded. Rebuilding
with different choices replaces that platform's existing fixture artifacts.

## Build the public Apple fixtures

Use Xcode and CMake 3.25+. Install the Rust targets explicitly:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
python3 scripts/build-mobile.py macos
python3 scripts/build-mobile.py ios-sim
python3 scripts/build-mobile.py ios
```

The macOS artifact is `target/auv3/macos/RavetGainContainer.app`, a headless
containing-app fixture with `Contents/PlugIns/RavetGainExtension.appex`.
The simulator/device artifacts are `target/auv3/ios-sim/RavetGainMobile.app`
and `target/auv3/ios/RavetGainMobile.app`, each embedding the extension under
`PlugIns/`. The AU identity is `aufx/RvG3/Ravt`, distinct from the
[AUv2 component](audio-unit.md). The extension principal class inherits the
programmatic CoreAudioKit parameter editor. Native system sliders use the
schema's linear/log/discrete mappings; iOS controls notify host touch/release
gestures. MacOS sliders currently send value notifications only.

Builds do not install/register extensions globally, launch applications or
provision devices. macOS/simulator artifacts use local ad hoc signatures and
include the RAVET and libm notices. Android APKs also retain the supplied NDK
notices. This is a scoped fixture packaging check; release dependency auditing
remains separate. The device artifact is unsigned: a consumer must
configure its own bundle IDs, developer team, app/extension provisioning profiles
and production entitlements through its Apple distribution project. The included
macOS sandbox entitlement is packaging configuration, not sandbox runtime evidence.

An explicit simulator fixture can include the in-process contract runner:

```sh
python3 scripts/build-mobile.py ios-sim --embed-contracts
# Use an available simulator UUID; boot it before installing.
xcrun simctl install SIMULATOR_UUID target/auv3/ios-sim/RavetGainMobile.app
SIMCTL_CHILD_RAVET_RUN_AUV3_CONTRACTS=1 xcrun simctl launch --console \
  --terminate-running-process SIMULATOR_UUID org.ravet.gain
```

This tests the subclass in an iOS process without starting devices; it does not
test an extension IPC host. Default builds omit this runner. MacOS builds run
their in-process contracts automatically.

## Build the public Android fixture

Use external NDK r29, CMake 3.25+ and the arm64 Rust target. SDKs remain outside
source control and the build script downloads nothing:

```sh
rustup target add aarch64-linux-android
python3 scripts/build-mobile.py android --ndk /path/to/android-ndk-r29
# Also package and verify an installable development APK:
python3 scripts/build-mobile.py android --ndk /path/to/android-ndk-r29 \
  --android-sdk /path/to/android-sdk --java-home /path/to/jdk17
```

APK packaging defaults to platform 36 and build-tools 36.0.0 in the usual SDK layout.
The result is `target/mobile/android/ravet-gain-debug.apk`; native-only builds
produce `target/mobile/android/libravet_gain_mobile.so`. The build checks Java
warnings, generates DEX/resources, aligns uncompressed native libraries for
16 KiB pages and verifies a local development signature. The development keystore
stays under ignored `target/mobile/`; this is not a store/distribution signing
configuration. An optional Gradle 8.13/AGP 8.13.0 project is also provided; pass
`-PravetGainLibrary=/absolute/path/to/arm64/libravet_gain_native.a` and configure
the external Android SDK/NDK. Optional Gradle properties `ravetMinApi`,
`ravetTargetSdk` and `ravetCompileSdk` select the same deployment choices and obey
the same minimums. The direct script is the locally exercised path.

## Standalone Audio/MIDI settings and lifecycle

The public apps expose Audio/MIDI settings by default, alongside gain, bypass,
Start and Stop. Embedded AU editors expose parameter controls and use host
routing. The Apple app's route picker uses the system route; its settings offer
preferred rates/buffers and currently available MIDI sources. Its status shows
the negotiated rate, buffer and input channels. Engine reconstruction preserves
complete parameter state and attempts MIDI reconnection by unique endpoint ID.
The Android app lists available input/output devices, preferred rate/buffer and
MIDI output ports (sources for the app), and shows actual negotiated values plus
input shortages and native errors. A disconnection/error stops on the control
poll; Start retries explicitly. Both fixtures stop on app backgrounding.

Android output callbacks chunk into at most 4096 frames. Input reads have a zero
timeout; short capture prefixes retain their samples and missing frames become
silence with a counter. Input/output must negotiate equal rates and channel
counts. Independent device clock drift is not corrected in this adapter.
The 1024-slot native MIDI queue rejects overflow, serializes producer ownership
outside the audio callback and delivers accepted messages at the next chunk's
frame zero. Android MIDI stream parsing preserves running status and interleaved
realtime bytes; timestamps, SysEx and MIDI 2.0 device input are not mapped here.

## AU render/state contract

AU parameter values are physical schema values: Gain is 0..4 with unity at 1.
Saved state is the complete shared native state, including bypass. Its canonical
record remains authoritative over cached superclass AU parameter values; malformed
recall preserves the previous publication. Control edits and newer complete
restores may coalesce. One-attempt Busy/Stale publication outcomes are bounded;
callers using `NativeSession` can check control return values. The AU main-queue
timer refreshes cached parameter values/observers at 30 Hz. Headless hosts without
a serviced main queue call `refreshParameterValues` on control when reading cached
values after direct internal rendering.

Allocation validates noninterleaved f32 bus/rate/frame bounds before preparation.
Maximum frames are 65536, default 4096. Parameter/MIDI lists validate completely
before input pull or DSP; at most 1024 links and `1024 - parameter_count`
flattened events are accepted, reserving slots for active parameter ramps.
Past/immediate events clamp to frame zero; future/out-of-order events reject.
Ramps retain remaining duration across blocks and cancel on newer complete
state recall or reset. If repeated concurrent control publication prevents
observing the accepted restore's values, a newly started ramp uses its last
observed base value; the DSP restore itself remains coherent. Native DSP smoothing
remains the processor's own behavior.
UMP words are framed using portable packet sizes; all words in each AU event list
use its enclosing render-event sample offset. Packet host timestamps are not
mapped independently. Unsupported event types/cables and generated events reject.
Faults clear supported output prefixes and retain backing tails. Cached render
blocks require the AudioUnit and caller-owned buffers to remain alive; callbacks
must stop before resource teardown.

## Evidence and outstanding support

Native AU schema/audio/state/bypass/timed changes/ramp/lifecycle, transport,
short MIDI/UMP rejection and programmatic editor construction are exercised on
macOS arm64 and in an iOS simulator process. Apple device and simulator artifacts
cross-build; the iPhone simulator app's native UI launches. Android native code
cross-compiles; Java and the debug APK package/signature/alignment checks pass.
A scripted AAudio double verifies chunking, live state, bounded MIDI overflow,
short input silence, open/configuration/capture failures and stream retirement:

```sh
cargo build -p ravet-gain-native --release --locked
cmake -S tools/mobile-validation -B target/mobile/contracts \
  -DRAVET_GAIN_LIBRARY="$PWD/target/release/libravet_gain_native.a"
cmake --build target/mobile/contracts --parallel 2
ctest --test-dir target/mobile/contracts --output-on-failure
```

Full support still requires extension discovery and IPC/host validation, physical
iPhone/iPad and Android audio/MIDI/route/interruption tests, Android runtime
execution, production signing/distribution and callback measurements. Auxiliary
buses, generated event output, factory presets, SysEx, f64 selection, native
instrument examples, persistent standalone session storage, broad touch/keyboard/
assistive-tech interaction and background audio remain unimplemented profiles.
See [decision 0153](decisions/0153-auv3-mobile-native-adapters.md). Platform build
recipes and configured CI are not executed device/DAW support claims.
