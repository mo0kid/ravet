# Interactive RAVET examples

Run from the repository root on macOS with Rust and Xcode command-line tools:

```sh
cargo run -p ravet-examples --bin midi-synth --locked
cargo run -p ravet-examples --bin effect --locked
cargo run -p ravet-examples --bin mpe --locked
cargo run -p ravet-examples --bin midi2 --locked
```

Use the title bar's **Audio / MIDI…** button to choose devices, channel pairs,
sample rate, buffer size and active MIDI inputs. Press **Start audio** there.
These Audio/MIDI selections are remembered between launches. The main Start/Stop
button restarts using the selected settings; restore itself does not start audio.
The synth has waveform selection, VCA/VCF knobs and a shared ADSR. The effect is
a stereo delay with time, feedback and mix knobs, ping-pong and bypass; demo
bursts repeat until stopped. Its Demo loop switch is in the effect panel; turn
it off to hear the selected device input. MPE also plays live; MIDI 2.0 is an interactive packet inspector without audio/device
output. The standard popup is enabled by default in RAVET standalone windows and
can be explicitly disabled. Plug-in devices remain host-owned.

Explicit `--render [new-path.wav]` (synth/effect) and `--offline` (MPE/MIDI 2.0)
retain hardware-free fixtures. WAV paths refuse overwrite. Portable tests build
with `--no-default-features`. `--ui-test <directory>` runs hidden WebKit fixtures
and writes screenshots without opening devices.

See the [guide](../../docs/examples.md) for use cases, plug-in builds, controls,
source map and current limits.
