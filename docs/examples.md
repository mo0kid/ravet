# Small examples and use cases

The four `ravet-examples` entry points open an interactive native window on
macOS with the default `live-ui` feature. Build with Rust and Xcode command-line
tools, using the [getting started](getting-started.md) prerequisites. The
portable processors and explicit offline fixtures also build without `live-ui`.

```sh
cargo run -p ravet-examples --bin midi-synth --locked
cargo run -p ravet-examples --bin effect --locked
cargo run -p ravet-examples --bin mpe --locked
cargo run -p ravet-examples --bin midi2 --locked
```

Each window includes tabs for all four examples. Click **Audio / MIDI…** in the
native title bar to open the standard settings popup. Choose an output, active
channel pairs, sample rate and buffer size, then press **Start audio**. The popup
closes after a successful start; reopen it to stop or change settings. Devices
are never opened merely by creating a window. Empty input/MIDI inventories hide
their controls. Buffer labels show samples and nominal milliseconds; “Device
default” leaves buffer negotiation to the backend. A requested fixed buffer is
not a measured latency or callback-size guarantee.

Available audio inputs and their channel pairs appear in the same popup. The
effect can process a selected input or quiet, rhythmic demo bursts. The synth generates its
own audio; optional captured input does not alter its sound. Multiple MIDI inputs
can be enabled together (at most 16 in these examples). No MIDI input is selected
initially; the on-screen keyboard remains available. Stop audio before changing
settings and refresh explicitly after device changes. Audio/MIDI selections are
remembered between launches; unavailable devices remain unselected. The main
Start/Stop button restarts with the current settings, opening the popup if no
output is configured. Restore does not start audio automatically.

## Subtractive MIDI synth

The [portable processor](../examples/tutorials/src/synth.rs) has eight voices,
sine/polyBLEP saw/polyBLEP square/triangle waveform selection, a resonant low-pass
VCF and a VCA. One ADSR per voice drives both amplitude and filter excursion.
The knobs support vertical dragging, Shift for finer pointer changes, arrow and
Home/End/Page keys, numeric entry and double-click reset. Cutoff uses logarithmic
knob travel. The shared [web controls](web-controls.md) provide gesture ownership
and snapshot refresh without feeding edits back to audio.

| Control | Range | Default |
|---|---|---|
| VCA level | 0..1 | 0.6 |
| VCF cutoff | 20..20,000 Hz, capped below Nyquist internally | 800 Hz |
| Resonance | Q 0.5..8 | 0.7 |
| Filter envelope amount | 0..72 semitones above base cutoff | 36 |
| Attack / decay / release | 0..5,000 ms | 10 / 150 / 300 ms |
| Sustain | 0..1 | 0.7 |

ADSR edits latch at the next note-on. Level, waveform, cutoff, resonance and
filter amount affect held notes. Waveform changes retain oscillator phase but
are not crossfaded. Filter coefficients are prepared on the UI thread into a
fixed 256-step table; audio selects an entry from envelope/timbre position.
Triangle can alias; there is no limiter or overload guarantee at high resonance.
MIDI velocity scales the VCA and the mix scales by the eight-voice capacity.

Click and hold piano keys or use `A W S E D F T G Y H U J K` when the page
background or piano widget has keyboard focus. The reusable two-octave keyboard
spans C3–C5, with shorter black keys layered above adjacent white keys. Independent
pointers play chords; dragging releases the previous key and starts the next. Focused piano keys also play with Space/Enter.
Release, cancellation, opening settings or leaving the window releases held
on-screen notes. All notes off clears queued notes at the next processing block.
MIDI sustain, bend and the existing two-zone voice policy remain available.

This app is distinct from the existing [Sine CLAP reference](sine-clap.md), which
has its own fixed sine processor and host level control. The new subtractive
processor does not yet have a plug-in export.

## Stereo delay effect

The live effect uses the [portable delay processor](../examples/tutorials/src/delay.rs),
built from RAVET's prepared fractional delay lines and sample-counted smoothers.
Three knobs control delay time, feedback and wet/dry mix, with optional ping-pong
cross-feedback and dry bypass. Demo mode plays tapered, staggered stereo tone
bursts every 0.9 seconds, making the repeats audible without an input device.
The **Demo loop** switch is in the effect panel. Turn it off to process the
input device selected in Audio/MIDI settings; that popup lists device inputs
and **No input**, without synthetic sources or files. With no input selected
and Demo loop off, the effect receives silence.

| Control | Range | Default |
|---|---|---|
| Delay time | 1..2,000 ms, logarithmic knob travel | 375 ms |
| Feedback | 0..0.95 | 0.45 |
| Wet/dry mix | 0..1 (dry..wet) | 0.35 |
| Ping-pong / bypass | Off or on | Both off |

Delay time glides over 20 ms, producing a brief pitch change during edits.
Feedback, mix and ping-pong routing ramp over 5 ms. Stereo mode repeats each
channel independently; ping-pong sends each delayed channel into the opposite
feedback branch. Bypass immediately returns dry input while internal repeats
continue. Reset clears the delay history and preserves requested controls.
There is no limiter, tempo sync, preset persistence or new delay plug-in export.
The portable processor accepts mono/stereo f32 at 8..192 kHz, up to 1,024 frames
and 128 input events. Two f64 delay lines allocate only during preparation;
processing, control changes and reset perform no heap operations. Nonfinite
input samples become zero; ordinary finite input still requires output headroom.

Input channel pairs selected in settings are averaged into stereo;
the stereo result is copied to each selected output pair. Unselected output
channels are silenced; an odd final mono channel receives the stereo average.
Input and output clocks use the existing clocked duplex adapter, including
conversion when their default input rate and selected output rate differ.

The existing Gain plug-in remains a separate, simpler plug-in integration:

```sh
python3 scripts/bundle-clap.py --plugin gain --profile release
cargo run -p ravet-clap-smoke --locked -- 'target/clap/release/RAVET Gain.clap'
```

See [getting started](getting-started.md) and [native editor](native-editor.md)
for its editor and recorded host limits. Plug-in I/O is configured by its host;
the standalone settings popup is not added to host-owned embedded editors.

## MPE performance

The MPE tab plays the subtractive synth. Its on-screen keyboard configures a
lower zone with master channel 0 and two member channels, 1 and 2 (zero-based).
Member bend defaults to ±48 semitones; master bend retains its receiver default.
Select a member and adjust its bend, pressure and timbre knobs while holding a
note. Pressure increases VCA amplitude by up to 50%; CC74 timbre adds up to 24
semitones of filter opening. A connected controller can negotiate zone/range
settings through MIDI RPN messages. The voice table exposes expression and ADSR.
The two-note on-screen limit is independent of the synth's eight-voice pool.

Native MIDI queues retain complete packets. This example consumes short MIDI
messages at the next audio block; it does not map native timestamps to sample
offsets. There are 256 packet slots per input and at most 64 polling attempts
per processing block shared across up to 16 inputs. Full MIDI/control queues
clear voices rather than leaving a dropped note-off stuck. No physical
controller conformance or audio deadline is established by offline fixtures.

## MIDI 2.0 packet inspector

The MIDI 2.0 tab provides editable group/channel/key, 16-bit velocity and 32-bit
expression fields. Encode/decode displays exact UMP words and recovered fields,
retaining up to 16 packets. It is a codec inspector: it opens no devices and does
not synthesize MIDI 2.0, negotiate MIDI-CI/profiles or export a MIDI 2.0 plug-in.
The settings popup explains this when opened from that tab. Native UMP delivery
is a separate [CoreMIDI UMP capability](core-midi-ump.md).

## Offline fixtures and checks

Rendering and scripted traces require explicit flags:

```sh
cargo run -p ravet-examples --bin midi-synth --locked -- --render target/examples/synth-fixture.wav
cargo run -p ravet-examples --bin effect --locked -- --render target/examples/effect-fixture.wav
cargo run -p ravet-examples --bin mpe --locked -- --offline
cargo run -p ravet-examples --bin midi2 --locked -- --offline
cargo test -p ravet-examples --no-default-features --locked
```

The [offline helper](../examples/tutorials/src/support/mod.rs) renders stereo
PCM16 at 48 kHz in 256-frame blocks with at most 32 scheduled events and refuses
file overwrite. Synth fixtures exercise note/pedal/release timing; effect
fixtures check decaying ping-pong repeats and exact bypass offsets. Portable
delay tests additionally check partition-independent automation, maximum delay,
transactional event rejection, tail retention during bypass and zero heap actions.
Scripted MPE prints
independent member/master expression; MIDI 2.0 checks full-resolution timed UMP
round trips. These fixtures do not open devices or windows.

On macOS, the hidden native UI fixture checks all tabs, title-bar popup activation,
explicit opt-out, controls, fake channel/rate/buffer selection, note release and
packet inspection. It writes screenshots and opens no actual devices:

```sh
cargo run -p ravet-examples --bin midi-synth --locked -- --ui-test target/examples-ui
cargo test -p ravet-examples --locked
```

The live [UI owner](../examples/tutorials/src/ui/mod.rs) and
[bounded audio adapter](../examples/tutorials/src/ui/audio.rs) keep JSON, native
ownership and coefficient preparation outside processing. A fixed 16-command
queue carries prepared patches; coherent word snapshots carry meters/voices.
The device adapter accepts f32 configurations with at most 64 channels, selected
rates 8..192 kHz and prepared sub-blocks of at most 512 frames. Live changes and
arrival-time MIDI are block-quantized. Native window/device integration remains
a macOS prototype; broader platforms, devices, long sessions and deadlines need
actual validation. See [work status](WORK_STATUS.md) for evidence.

The example window locks its content aspect ratio to 900:780. A single 900×780
design canvas scales uniformly in both directions, including text, controls,
panels and spacing. Resize events update the canvas immediately; the scale is
not capped. All four tabs are checked at 720×624, 900×780 and 1080×936, plus
wide/short and narrow/tall requests constrained to the same aspect ratio. The
fixtures verify actual browser dimensions, control dimensions/positions and
unclipped content. Voice details scroll inside their
bounded monitor panel. Shared semantic palette tokens cover controls, settings
and piano keys; Amber is the selected RAVET default, with charcoal/plum surfaces
and amber emphasis. Disabled piano keys remain opaque.
