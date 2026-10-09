# LV2 gain reference

The public reference exports distinct mono and stereo LV2 descriptors with
f32 audio ports and block-start gain/bypass control ports. Its prepared DSP is
the same Rust gain ABI as [VST3](vst3.md). Headers stay in an external LV2 SDK.

```sh
CARGO_BUILD_JOBS=2 python3 scripts/bundle-lv2.py --sdk /path/to/lv2
```

Supply the LV2 source SDK containing `include/lv2/core/lv2.h` and `COPYING`.
Version 1.18.10 from the [official project](https://github.com/lv2/lv2) was
exercised. The script builds with two jobs, copies Turtle metadata and binary
into `target/lv2/ravet_gain.lv2`, adds notices and checks the actual packaged
binary. macOS receives an ad hoc binary signature. No global host installation
occurs. The Linux build branch is unvalidated; Windows is not implemented here.

Plugin URIs are `urn:ravet:gain:mono` and `urn:ravet:gain:stereo`. Mono ports are
input, output, gain, bypass; stereo ports are left/right inputs, left/right
outputs, gain, bypass. Physical gain spans 0..4 with unity at 1; bypass is stepped
0/1. Finite controls clamp; nonfinite controls reject with output silence.
All controls are required connections. Inputs may alias outputs; output ranges
must be distinct and host pointers cover the stated frame count. Processing is
bounded to 65,536 frames; excess blocks silence without DSP mutation. Panic
containment disables the instance until recreation. Ordinary rejection permits
later valid blocks. Activate resets history and preserves requested values.

The host persists control-port values and reapplies them at block boundaries.
No extra nonparameter state or state-extension payload is needed by this gain
reference. This fixture checks emulated control recall, not actual DAW session
recall. Atom ports, sample-timed automation, MIDI, custom UI, programs, f64 and
auxiliary buses remain pending. No hardRTCapable declaration is made.

Actual-binary contracts check both descriptors, invalid sample rate, activation,
wet/dry samples, control recall, zero-frame changes, malformed-control silence,
deactivation/reactivation, active-prefix tails and in-place aliases. Native
allocation, host discovery/metadata validation and DAW integration need separate
evidence. See [decision 0023](decisions/0023-native-format-references.md).
