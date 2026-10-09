"""Task-oriented explanations checked against the framework and teaching processors."""
GUIDES = [
('building-a-processor', 'Building an audio processor', 'Processing & events', '''
# Building an audio processor

A processor owns the mutable state that changes while samples are rendered. It does not need to own a window, a device, a plug-in SDK or a file writer. Implement {{api:ravet_foundation::Processor}} and let a separate consumer supply blocks.

## Choose the audio layout and limits

{{api:ravet_foundation::ProcessConfig}} describes sample rate, channel layout, maximum block size and bounded event storage. The caller creates a valid configuration; the processor still decides whether it supports that configuration. The example delay accepts mono/stereo effects at 8–192 kHz, at most 1,024 frames and 128 input events. The subtractive example accepts mono/stereo instruments with no audio inputs.

`prepare` is the place to allocate delay history, scratch buffers and tables. Clear the active configuration first: a failed preparation must not leave an earlier configuration apparently active. Store the new configuration only after all required resources are prepared. The callback must not race preparation or deactivation.

## Implement the callback

Begin `process` by checking preparation and calling `config.validate_block(block)`. Reject invalid layout, frame counts and event storage before changing DSP state. Product-specific constraints, such as a delay's physical parameter range, need an additional complete event preflight.

On rejection, call `block.clear_outputs()` and return `ProcessStatus::Fault(error)`. This clears active audio and emitted events. Successful processing initializes every active output sample. Unknown valid event kinds may be ignored if the processor documents that policy.

Apply events for a frame before rendering that frame. Process same-frame events in their supplied order. A zero-frame block can carry offset-zero events: apply those without advancing an oscillator, smoother or envelope.

## Reset and deactivate

`reset` clears signal history while keeping the prepared configuration and requested controls. It has the same callback restrictions as processing. The example delay clears its rings and snaps smoothers to their current targets; the synth clears voices, envelopes, filters and oscillator phases.

`deactivate` stops prepared processing and may release resources outside the callback. The two teaching processors reset and clear their configuration. Their conservative `Tail::Infinite` declarations do not mean samples are guaranteed to remain audible indefinitely.

## Verify the contract

Use fixed expected samples, different block partitions and an invalid late event to check whole-block rejection. Measure both allocation and deallocation around repeated callbacks, reset and prepared control changes. A passing allocation test does not prove a real-time deadline or host compatibility.

Read [the delay walkthrough](/ravet/docs/tutorials/src/delay/) and [the synth walkthrough](/ravet/docs/tutorials/src/synth/) for complete implementations.
'''),
('event-timing', 'Sample-timed events and automation', 'Processing & events', '''
# Sample-timed events and automation

{{api:ravet_foundation::Event}} carries a frame offset and {{api:ravet_foundation::EventData}}. Offsets belong to the current block; they are not absolute timestamps or an instruction to wait for wall-clock time.

## Translate a clip schedule into callback offsets

The offline helper keeps an absolute schedule. For each block beginning at `start`, it copies events before the block's exclusive `end` into fixed storage and subtracts `start`. An event exactly at `end` stays pending for the next block at offset zero. Only the occupied event slice is borrowed by `ProcessBlock`.

This preserves ordering even when two events share a frame. It also makes a sample at frame 72,007 independent of whether the renderer partitions its clip into 64-, 128- or 256-frame blocks.

## Validate first, apply second

A valid event stream fits the prepared capacity and is in nondecreasing offset order. For a nonempty block, an offset equal to the frame count is outside the block. Zero-frame blocks accept offset-zero events without advancing sample state. Finite parameter values may still violate a particular processor's physical range.

The delay preflights every known parameter event before writing a ring or changing a target. If a later event is invalid, earlier events in that block are not committed. See {{api:ravet_foundation::ProcessConfig::validate_block}} and the [offline scheduling helper](/ravet/docs/tutorials/src/support/mod/).

## Automation is not the same as smoothing

An event takes effect at its exact offset. A smoother determines the subsequent sample trajectory. {{api:ravet_signal::LinearSmoother}} advances once per frame, not once per channel. A target change starts from the current effective value; an identical target does not restart a ramp.

The example delay uses 20 ms for time and 5 ms for feedback, wet mix and ping-pong routing. Bypass is immediate. Moving the read position changes pitch briefly; it is not a crossfade between two stationary delays.

## Native MIDI has a different timing boundary

The live example polls MIDI with a bounded budget and applies accepted short messages at the next processing block. It does not convert native arrival timestamps into sample offsets. The offline fixtures demonstrate sample timing; that does not establish sample-accurate live input timing.
'''),
('callback-handoff', 'Audio-thread ownership and control handoff', 'Processing & events', '''
# Audio-thread ownership and control handoff

The main thread can allocate, parse JSON, enumerate devices and prepare coefficients. The audio owner must process already-prepared data without allocation, deallocation, blocking or I/O. These are implementation obligations, not guarantees conferred by a Rust type alone.

## Prepare data before sending it

`SynthPatch::new` validates controls and constructs a fixed 256-entry filter table on the control thread. The callback receives a prepared patch, rather than computing logarithms, exponentials and filter coefficients for each sample. Patches contain fixed storage; dropping a command in the callback does not drop a heap-owned table.

The [live audio adapter](/ravet/docs/tutorials/src/ui/audio/) uses a 16-command SPSC queue. The UI owns the producer and the audio owner owns the consumer. `Command` carries prepared synth patches, delay values, a demo switch or a validated short MIDI message.

## Bound work and handle overflow

Each callback consumes at most 16 control commands and makes at most 64 native MIDI polling attempts across no more than 16 inputs. Each native MIDI input uses 256 packet slots. A full queue is a correctness condition: losing note-off messages can leave notes held indefinitely.

The example requests a panic/reset on control overflow. Detected native packet loss clears voice state and drains bounded pending packets. This avoids pretending every packet was delivered and gives the UI an explicit error it can present.

## Publish telemetry without sharing mutable DSP

{{api:ravet_foundation::SnapshotMailbox}} transfers coherent fixed-size word snapshots. The example publishes an 80-word record containing a frame counter, four peak/RMS readings, a drop counter and up to eight voice records. Float values are transferred using their bit representation; a UI snapshot reconstructs them with `f32::from_bits`.

A failed `try_read` leaves the previous displayed snapshot usable. A failed `try_publish` does not justify blocking the callback. Do not use this mechanism as a replacement for ordered event delivery: it communicates the latest coherent state, not every intermediate change.

## Stop before changing ownership

Device replacement, format changes and teardown belong outside running callbacks. The UI pauses and releases the stream, closes MIDI inputs, drops the producer and resets telemetry before creating another audio owner. [Native UI message flow](/ravet/docs/reference/native-ui-flow/) follows that sequence in detail.
'''),
('delay-signal-flow', 'Stereo delay: controls, timing and feedback', 'DSP', '''
# Stereo delay: controls, timing and feedback

The teaching delay wraps two prepared {{api:ravet_signal::DelayLine}} instances with four {{api:ravet_signal::LinearSmoother}} values. Its complete source is explained in [delay.rs](/ravet/docs/tutorials/src/delay/).

## Physical controls

| ID | Control | Range | Default |
| --- | --- | --- | --- |
| 1 | Delay time | 1–2,000 ms | 375 ms |
| 2 | Feedback | 0–0.95 | 0.45 |
| 3 | Wet mix | 0–1 | 0.35 |
| 4 | Dry bypass | Exactly 0 or 1 | 0 |
| 5 | Ping-pong routing | Exactly 0 or 1 | 0 |

`set_controls` validates the complete five-value patch before committing it. IDs map to array positions by subtracting one. Time, feedback, mix and routing have smoother slots; bypass does not.

## One rendered frame

Read the input first, duplicating mono into the internal stereo pair and replacing nonfinite samples with zero. Advance each smoother once. Convert time from milliseconds to fractional samples using the prepared sample rate, then read both delay lines before writing either line.

For channel `c`, write `dry[c] + feedback * ((1 - cross) * wet[c] + cross * wet[1 - c])`. At `cross = 0`, each channel feeds itself. At `cross = 1`, each channel feeds the opposite delayed output. Reading both lines before either write prevents channel order from changing the recurrence.

After writing, advance both rings. Non-bypassed output is `(1 - mix) * dry + mix * wet`. Bypass outputs dry samples while the internal recurrence continues. Removing bypass can therefore reveal a retained tail.

## Preparation and reset

Each ring holds two seconds at the configured rate plus interpolation headroom. Allocation occurs in preparation. Time glides for 20 ms; the other smoothed controls use 5 ms. `reset` clears history and snaps ramps without allocating.

## Limits and tests

There is no output limiter, tempo synchronization or plug-in export for this example. Finite input still needs headroom. The [delay tests](/ravet/docs/tutorials/tests/delay/) check impulse positions, cross-channel repeats, partition-independent automation, bypass-tail retention, late-event rejection and callback heap activity.
'''),
('synth-signal-flow', 'Subtractive synth: voices, envelope and filter', 'Instruments & MIDI', '''
# Subtractive synth: voices, envelope and filter

The example instrument owns eight fixed voice slots. Each slot contains {{api:ravet_signal::Oscillator}}, {{api:ravet_signal::Adsr}} and {{api:ravet_signal::StateVariableFilter}}. {{api:ravet_instrument::MpeReceiver}} decides voice and expression ownership; the processor decides how those values affect sound.

## Prepare the patch outside the callback

`SynthPatch::new` validates all nine physical controls and an 8–192 kHz sample rate. It converts ADSR milliseconds into seconds and creates 256 filter coefficient entries. Each entry represents increasing envelope/timbre excursion above the base cutoff, capped below 45% of sample rate.

The waveform index must be an integer from zero to three. Sine, polyBLEP saw and polyBLEP square are available alongside a triangle that can alias. There is no waveform crossfade.

## Respond to a note

MIDI and normalized note events feed the shared MPE voice receiver. Its changed-slot mask avoids reinitializing untouched voices. A newly started or stolen voice resets oscillator and filter history, latches the current ADSR configuration and triggers attack. A released voice triggers envelope release exactly once.

Pitch is the note key plus member and master bend. Convert semitones to frequency relative to A4 = 440 Hz and cap it below 49% of sample rate. Retuning does not require creating a new oscillator.

## Render a voice

Advance the envelope; combine member/master timbre and pressure with explicit clamps. Envelope excursion plus up to 24 semitones of timbre chooses an entry from the prepared filter table. Render oscillator → low-pass filter → envelope-controlled amplitude.

Velocity scales amplitude. Pressure can add up to 50%. Sum active slots and multiply by the requested level divided by eight. This fixed scaling is not a limiter or an overload guarantee at high resonance.

## Finish the release

When the envelope enters `Idle`, call the receiver's `finish` for that slot. Note-off alone does not free the voice: release audio still owns it. ADSR edits latch at the next note-on; level, waveform and filter changes also affect held notes.

Read [synth.rs](/ravet/docs/tutorials/src/synth/) and [the subtractive tests](/ravet/docs/tutorials/tests/subtractive/) for the code and checks. This example has no MIDI 2.0 synthesis or plug-in export.
'''),
('native-ui-flow', 'Native UI messages, device settings and snapshots', 'Editors & graphics', '''
# Native UI messages, device settings and snapshots

The examples combine a local HTML editor, a main-thread Rust bridge and an independently owned audio callback. [ui/mod.rs](/ravet/docs/tutorials/src/ui/mod/) owns devices and JSON; [ui/audio.rs](/ravet/docs/tutorials/src/ui/audio/) owns prepared rendering.

## Open the window without opening devices

The entry point selects Synth, Effect, MPE or MIDI 2.0 mode. {{api:ravet_webview::StandaloneWindow}} embeds the editor and its callbacks. The standard title-bar Audio/MIDI settings popup is enabled by default. Creating the window only enumerates settings; a Start action is required to open and play an output.

The MIDI 2.0 tab is an inspector. It creates and decodes packets but does not send them to an instrument or an external device.

## Validate a page message

The bridge refuses messages larger than 4,096 bytes before parsing JSON. `State::handle` dispatches by message type. Synth and delay controls validate a candidate whole patch before committing UI state. MIDI input validates the short-message bytes. The inspector checks group, channel, key and full-resolution numeric bounds before constructing a typed packet.

Do not trust an HTML `min`/`max` attribute as validation: the Rust boundary checks again. The page's native bridge reports unavailability explicitly rather than silently accepting edits.

## Start and stop audio

Starting stops the old owner, verifies the selected device's f32 channel/rate/buffer capabilities, validates selected channel pairs and opens at most 16 unique MIDI inputs. A selected audio input uses the clocked duplex adapter; output-only operation uses the output adapter.

The producer sends the prepared patch or delay controls before playback. Any setup or play failure triggers cleanup. Changing the example mode also stops audio. The page releases held on-screen notes when settings open, the mode changes or the window loses those interactions.

## Refresh the display

The callback publishes word snapshots. `State::snapshot` reads the latest complete record and constructs JSON for controls, meters, voice expression, packets and settings. `ravetUpdate` updates controls without sending those refreshed values back as edits.

The [editor walkthrough](/ravet/docs/tutorials/editor/index/) explains each HTML/JavaScript line, including keyboard ownership, pitch-bend encoding and the uniformly scaled 900×780 design canvas.
'''),
]
