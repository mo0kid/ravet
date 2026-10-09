# AAX gain reference

An external Steinberg AAX wrapper and external Avid SDK now build the public
Rust-backed mono/stereo gain reference as a local AAX bundle. The same VST3
processor/controller and Rust state/DSP are compiled into this artifact.
AudioSuite, MIDI, auxiliary buses and a custom editor are not advertised.

```sh
CARGO_BUILD_JOBS=2 python3 scripts/bundle-aax.py \
  --vst3-sdk /path/to/vst3sdk --aax-sdk /path/to/AAX
```

The script builds with two jobs, runs the underlying native DSP/controller
contracts, adds development notices, ad hoc signs the macOS artifact and loads
its binary to check required AAX exports and the SDK version query. No normal
Pro Tools directory is changed. The local bundle is under `target/aax/release`.
The installed SDK reports version word `0x20801345980` in the exercised build.
Only macOS arm64 has been built and loaded.

Loading this binary outside Pro Tools does not establish AAX host acceptance,
parameter/state/render integration or deadline behavior. Pro Tools developer
host access or an authorized production PACE signing configuration is needed
for that next validation. Installed wraptool alone is insufficient. The helper
never reads credentials, invokes PACE production signing or publishes artifacts.
Avid terms in the supplied SDK and the owner's agreement require review before
any release; the development notice is not a redistribution licence grant.

Native IDs `RvG1` and `RvG2`, manufacturer `Ravt`, product `RvGn` and effect IDs
`org.ravet.gain.mono`/`org.ravet.gain.stereo` identify the public reference.
These identifiers require collision review before release. See
[VST3](vst3.md) for the inherited processing bounds and
[decision 0023](decisions/0023-native-format-references.md).

The shared processor's native VST3 f64 update was followed by an AAX rebuild and
required-export/SDK-version probe pass. This preserves local binary feasibility;
it does not establish an AAX f64 path or production Pro Tools loading/signing.
