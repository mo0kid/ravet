# AAX-only MidiSine reference

An authored empty Steinberg bootstrap satisfies the external wrapper's module
infrastructure with zero VST3 DSP classes. Owned public AAX registration selects
exact `org.ravet.midisine.mono/stereo` factories and initialized native processing;
no private SDK layouts or copied SDK implementation are used.

Local macOS build:

```sh
python3 scripts/bundle-aax.py --plugin midi --vst3-sdk /path/to/vst3 --aax-sdk /path/to/AAX
```

`--internal-precision 64` selects the shared sine kernel's genuine native f64 output
behind unchanged AAX f32 host audio. The raw short-MIDI stream is bounded and fully
preflighted; complete state persists only acknowledged Level. Bank/program requests
remain observable without implicit Level replacement. No bypass/editor/AudioSuite
or Pro Tools/PACE validation is claimed. See [the input/reference guide](../../docs/aax-short-midi.md)
for canonical F7, scope, diagnostics, ownership, local bundle tests and limits.
