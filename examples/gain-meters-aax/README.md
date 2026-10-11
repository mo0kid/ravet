# Gain AAX meter reference

Opt-in native32/64 Gain with six explicit host f32 peak/RMS taps, distinct
mono/stereo effect/product/native IDs and the unchanged canonical Gain schema,
complete state, transport, smoothing and bypass DSP. The authored AAX-only SDK
bootstrap exposes zero VST3 classes. External SDK implementation is not vendored.

See [prepared AAX meters](../../docs/aax-meters.md) for exact field/projection,
metadata lifetime, whole preflight, SPSC/stopped lifecycle and diagnostics.

```sh
CARGO_BUILD_JOBS=1 python3 scripts/bundle-aax.py --plugin gain-meters \
  --internal-precision 64 --vst3-sdk /path/to/vst3sdk --aax-sdk /path/to/AAX
```

The maintained generic composition contracts additionally build the public raw
sine and raw routing/Delay native exports. Target-only R4/W241 dependency
adoption must precede public packaging of this full pipeline. No stock/editor
profile, global install, Pro Tools, PACE signing, production distribution,
other-platform or real-time deadline qualification is implied.
