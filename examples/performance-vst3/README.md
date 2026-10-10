# Public VST3 performance instrument

The `RAVET Performance Sine` prototype connects the public native 16-voice
instrument to the externally supplied VST3 SDK. It declares no audio input,
mono/stereo output, one 16-channel performance input and native f32/f64 rendering.
The host supplies its generic parameter UI; no custom editor is declared.

Build from the repository root on the locally validated macOS arm64 toolchain:

```sh
# Complete build, public notices, local signing, contracts and SDK validation:
CARGO_BUILD_JOBS=2 python3 scripts/bundle-vst3.py \
  --sdk /absolute/path/to/external/vst3 --plugin performance
# Lower-level configuration remains available:
cargo build -p ravet-gain-native -p ravet-performance-native --release --locked
cmake -S examples/gain-vst3 -B target/performance-vst3 \
  -DCMAKE_BUILD_TYPE=Release \
  -DRAVET_VST3_SDK=/absolute/path/to/external/vst3 \
  -DRAVET_GAIN_LIBRARY="$PWD/target/release/libravet_gain_native.a" \
  -DRAVET_PERFORMANCE_LIBRARY="$PWD/target/release/libravet_performance_native.a"
cmake --build target/performance-vst3 --config Release \
  --target ravet_performance_vst3 ravet_vst3_performance_instrument validator --parallel 2
ctest --test-dir target/performance-vst3 -C Release \
  -R '^ravet_vst3_performance_instrument$' --output-on-failure
target/performance-vst3/bin/Release/validator \
  target/performance-vst3/VST3/Release/ravet_performance_vst3.vst3
```

The existing build entry point shares external SDK setup and requires the Gain
archive at configuration time. Building the explicit performance target links
only the performance archive. SDK sources remain outside RAVET. Output stays in
`target/`; these commands do not install into a system plug-in directory.

The profile-aware bundler keeps Gain as its default and isolates Performance at
`target/vst3/performance/{debug,release}`. Gain-only editor/program options reject
with Performance before building. No production signing identity is selected.

Level is the single global parameter (stable ID 1). Canonical complete state
contains level only; voice history is not serialized. Pending recall is an event
inside transactional rendering, so late rejected events preserve DSP progress and
do not consume the pending recall. Five audible standard note expressions are
advertised: volume, pan, tuning, vibrato and expression. Brightness/poly pressure
are accepted and stored without changing this reference's timbre.

Preparation supports 1..65536 maximum frames. Up to 1024 mapped global/performance
events share one callback budget; note cents may expand one raw note into two
events, and pending complete recall consumes one slot. Overflow rejects the whole
block with supported output silence. An unrelated event bus is filtered only
after validating its header/timing. Unknown selected expressions and unsupported
selected payloads reject. VST3 has no equivalent of native voice-completion
`NoteEnd`; this reference suppresses generated completion notifications while
retaining internal voice retirement. It declares no performance output bus.

The local SDK validator passes 47 tests; a disposable REAPER 7.82/macOS arm64
fixture also passes three 48 kHz synthetic MIDI/state/automation renders and
independent PCM checks. The host harness is `tools/reaper-validation/performance-instrument.lua`;
check its prepared resource with `python3 tools/reaper-validation/check-performance-render.py
/absolute/path/to/test-resource`. Hardware sends remain muted and transport
remains stopped. These offline results do not establish live device playback.

See [VST3 contracts](../../docs/vst3.md),
[decision 0166](../../docs/decisions/0166-vst3-performance-instrument.md) and
[host validation](../../docs/host-validation.md). Local SDK tests do not prove
other hosts/platforms, physical audio, deadline performance or accessibility.
