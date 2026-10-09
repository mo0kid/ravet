"""Beginner-facing introduction, checked against the gain processor and its adapters."""
INTRODUCTION = '''
# Introduction to RAVET

RAVET stands for **Rust Audio Visual Engineering Toolkit**. It is a framework for building audio effects, instruments, plug-ins and standalone audio applications in Rust.

An effect changes incoming sound; an instrument generates sound in response to notes or other controls. Either can run inside a digital audio workstation (DAW) as a plug-in, or in a standalone application connected to audio hardware.

RAVET lets you **write the sound processing once and connect it to different environments through adapters**. An adapter translates a DAW's or device's buffers and events into the data your processor expects. The processor can also be called directly by an offline renderer.

The sound-processing code therefore works with samples and events, without needing to manage a DAW's plug-in interface or open an audio device. **DSP** means *digital signal processing*: the maths that generates or changes sound. The chosen adapter must support the processor's inputs, outputs and events; support and validation vary by format and platform.

## Audio arrives in blocks

A **sample** is one channel's value at one instant. A **frame** contains all the channel samples for that instant. One stereo frame contains a left sample and a right sample.

A **block** is a sequence of frames processed in one call. At a sample rate of 48,000 Hz, there are 48,000 frames per second on each channel. A block of 256 frames contains:

- 256 samples on each channel, or 512 samples in total for stereo;
- `256 / 48,000` seconds of audio, approximately **5.3 milliseconds**.

The processor must finish in time for the next audio to be delivered. Missing that deadline can cause a gap or glitch. The block's duration is not the total playback latency, which also depends on device and host buffering.

## The callback and the sample loop

“Audio loop” can refer to two different levels of repetition.

**The audio callback** is a function the DAW or audio backend calls repeatedly to request the next block. RAVET's adapter turns each request into a call to the processor's `process` method.

**The sample loop** is the work done inside that call. A processor can visit the block's frames, apply any events scheduled for each frame, and calculate the output samples. Some algorithms work on a whole block instead; RAVET does not require every processor to use the same inner loop.

The {{api:ravet_gain::GainProcessor}} is a useful first example. This simplified sketch shows its frame-by-frame processing:

```rust
for frame in 0..frames {
    apply_events_for_this_frame();

    let gain = smoother.next_value();

    for channel in 0..channel_count {
        output[channel][frame] = input[channel][frame] * gain;
    }
}
```

The sketch leaves out setup and validation. `apply_events_for_this_frame()` represents the event-handling loop in the real source. In the full processor:

1. The complete block is validated before processing changes any state.
2. Events at a frame's offset are applied before that frame is rendered.
3. The smoother advances once per frame, so all channels use the same effective gain.
4. Each input sample is multiplied by that gain, or copied unchanged when bypass is active.
5. Input and output levels are measured for the meters.

For example, an input sample of `0.2` with an effective linear gain of `2.0` produces `0.4`. A smoother makes gain changes gradual to avoid an abrupt jump in the signal.

A delay and a synthesizer use the same processing entry point. Their calculations are different: a delay reads earlier samples from stored history, while a synth generates oscillator samples and shapes them with envelopes and filters.

## Where processing is called

The environment determines when another block is needed. A plug-in is called by its DAW; a standalone stream is called by its audio backend; an offline program decides how quickly to render its blocks.

| Environment | How the processor is called |
| --- | --- |
| Standalone audio | The audio backend invokes the callback registered by {{api:ravet_device::open_output}}. {{api:ravet_device::PreparedOutput::render}} builds a processing block, calls the processor and copies its output into the device buffer. Output-only streams suit instruments. Effects that need live input use an input/output adapter to supply that audio too. |
| DAW plug-in | The DAW calls the plug-in's format entry point. In the gain CLAP example, {{api:ravet_clap::EffectBridge}} translates the host's audio buffers, events and transport into RAVET's representation, then calls the same gain processor used by the offline example. |
| Offline rendering | Ordinary program code prepares a processor and calls `process` on its own buffers. The gain fixture runs without a device or DAW and checks its output against expected samples. |

Try the offline gain fixture from an authorised RAVET checkout:

```sh
cargo run -p ravet-gain --locked
```

It processes eight stereo frames, changes gain at frames zero and four, and checks the result. It does not play audio through a device. [Getting started](/ravet/docs/getting-started/) covers the requirements and how to build the CLAP version.

## The main parts of the framework

A Rust **crate** is a library or program package. RAVET separates its responsibilities across crates so a processor can use the parts it needs.

| Part | Responsibility |
| --- | --- |
| [ravet-foundation](/ravet/docs/api/ravet_foundation/) | Defines the processor interface, borrowed audio buffers, timed events and transport information such as playback position and tempo. |
| [ravet-signal](/ravet/docs/api/ravet_signal/) | Provides DSP building blocks: filters, oscillators, delay lines, smoothers, FFTs, convolution, reverb and meters. |
| [ravet-graph](/ravet/docs/api/ravet_graph/) | Connects processors, prepares their routing and runs them in the order required by their connections. |
| [Device adapters](/ravet/docs/audio-devices/) and [plug-in formats](/ravet/docs/plugin-hosting/) | Translate between portable processing and audio hardware or a host's plug-in interface. |
| [Editors and controls](/ravet/docs/native-editor/), [parameters](/ravet/docs/parameters/) and [state](/ravet/docs/observable-state/) | Provide windows, controls, parameter bindings, presets and communication between the interface and the processing owner. |

[Architecture](/ravet/docs/architecture/) explains these responsibilities in more detail. The [feature catalogue](/ravet/docs/FEATURES/) records the implementation and validation limits of each integration.

## Prepare once, process many blocks

The basic lifecycle of {{api:ravet_foundation::Processor}} is:

```text
create → prepare → process repeatedly → deactivate
```

**Create** constructs the processor's initial state. **Prepare** receives the sample rate, channel layout and maximum buffer/event capacities. The processor checks that it supports them and allocates any storage it needs before playback starts.

**Process** uses that prepared storage for each block. The same processor instance survives between calls: a delay keeps its previous samples, an oscillator keeps its phase, and a smoother keeps its current position in a ramp.

**Reset** clears signal history while retaining the prepared configuration and requested controls. **Deactivate** ends prepared processing. Preparation and deactivation happen after callbacks have stopped. Some consumers also use explicit `start_audio` and `stop_audio` session hooks; the [processor reference](/ravet/docs/api/ravet_foundation/trait.Processor.html) describes those separately.

## Controls and the audio thread

The **audio thread** owns the changing DSP state while processing runs. The UI must pass changes to that owner rather than modifying a delay ring or oscillator at the same time.

Bounded queues carry ordered commands and events. A queue has a fixed capacity, so its consumer can do a limited amount of work per callback. Snapshots share a coherent view of the latest state, such as meter readings, without requiring the UI to read the processor's mutable state directly.

An event can specify an exact frame offset within a block. Offsets start at zero. In a 256-frame block, an event at offset 100 takes effect before frame 100 is rendered. This allows a note or automation change to occur partway through the block.

During normal processing, the contract prohibits **memory allocation or deallocation, blocking, and file or network I/O**. Those operations can take an unpredictable amount of time and interrupt playback. Preparation, UI work and file writing happen outside the callback. Each processor implementation must uphold this rule; implementing the trait alone does not guarantee it.

Read [sample-timed events](/ravet/docs/reference/event-timing/) and [audio-thread ownership](/ravet/docs/reference/callback-handoff/) for the detailed rules.

## Follow an effect or instrument

Start with [Getting started](/ravet/docs/getting-started/) to run the gain fixture. For a complete effect or instrument, follow the [delay walkthrough](/ravet/docs/tutorials/src/delay/#L111) or the [subtractive synth walkthrough](/ravet/docs/tutorials/src/synth/#L252). Their numbered source listings show how controls, events and stored DSP state produce each block of sound.
'''
