# NoteRouting LV2 reference

This explicit local profile consumes the authoritative Rust NoteRouting native
processor. It routes timed channel/key notes through one output Atom Sequence,
scales On velocity by persisted Level, retains Off velocity and emits silent
mono/stereo audio. Default Gain/Performance profiles are unchanged.

```sh
python3 scripts/bundle-lv2.py --sdk /path/to/lv2 --plugin note-routing
```

The generated directory is `target/lv2/ravet_note-routing.lv2`. Output must be an
8-byte aligned live Chunk allocation of at least24592 total bytes, body capacity
24584, reset by the host before every run. The reference accepts<=1024 frames and
combined controls/notes and requires full1024 generated capacity even for an empty
prefix. State is complete Level only; transient history is not persisted.

See [the generated-output guide](../../docs/lv2-output.md) for callback ownership,
status/refusal/safe-empty rules, validation and missing host/platform features.
Local maintained CMake12, bundled mandatory6, Python6 validation is target-only;
the original package explicitly reused recorded archives. A separate root replay
now rebuilds all three native references and passes twelve native profiles,
maintained NoteRouting packaging with actual Cargo, six mandatory validators and
strict ad-hoc signature verification. Public workspace acceptance remains separate. There is no LV2 DAW,
SysEx, UMP/MIDI2, f64 LV2 audio, extra event ports, auxiliary routing or editor claim.
