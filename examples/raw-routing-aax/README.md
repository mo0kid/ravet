# RawRouting AAX reference

This local prototype consumes the public native RawBusRouting and RawDelayRouting
processors through the reusable owned AAX adapter. Six registered identities cover
mono/stereo main routing, both detector/monitor auxiliary source orders, and
mono/stereo Delay8 with an explicitly prepared Delay16 candidate. Host audio remains
planar float32; native processing is explicitly selected as 32 or 64.

Build with externally supplied AAX and VST3 SDKs:

```sh
python3 scripts/bundle-aax.py --plugin raw-routing --vst3-sdk /path/to/vst3 --aax-sdk /path/to/AAX
python3 scripts/bundle-aax.py --plugin raw-routing --internal-precision 64 --vst3-sdk /path/to/vst3 --aax-sdk /path/to/AAX
```

The bundler requires normal and fast-math native and actual registered-module
contracts, includes applicable notices and verifies a local ad-hoc signature. It
does not install the result. There is no editor or stock-wrapper profile for this
reference. The authored empty VST3 bootstrap exposes zero DSP classes; only the
owned AAX registration/factories are supported, not the SDK stock VST3 creator.

See [the raw effect/auxiliary guide](../../docs/aax-raw-routing.md) for scheduling,
MIDI, source-order, state, precision, timing and validation limits. This is not a
production state-compatibility or Pro Tools qualification claim. SDKs remain
outside RAVET; Avid/PACE distribution authorization is separate.
