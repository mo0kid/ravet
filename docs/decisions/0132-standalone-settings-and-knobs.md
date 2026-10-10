# 0132: Default standalone settings and rotary controls

## Decision

Put the standalone Cocoa window and title-bar Audio/MIDI settings entry point in
`ravet-webview`, outside portable audio/DSP. Enable the popup by default through
`StandaloneOptions`, with an explicit `io_settings: false` opt-out. Standalone
UI projects share its markup, snapshot/action contract and lifecycle; embedded
plug-in editors leave device routing to their host. Headless fixtures do not
create a window. Consumer callbacks own actual device services and validation.

Persist only Audio/MIDI selections through native macOS preferences, keyed by a
stable window title in a RAVET settings suite. Bound versioned JSON to 8 KiB and
restore exact available device IDs without starting audio. Hidden fixtures use
process-local preferences and full opt-out omits storage. Keep demo/file sources
in the application rather than the shared device selector. Expose a page Start
helper that reuses selections or opens settings if output is unconfigured.

The popup selects output/input devices, adjacent active channel pairs, supported
output sample rate and requested buffer size, and independently enabled MIDI
inputs. It does not open devices until Start. Device changes require Stop;
refresh is explicit. Buffer milliseconds are nominal, not measured latency.
The example mixes selected input pairs into stereo and duplicates stereo across
selected output pairs. Unselected outputs are cleared. The input stream uses its
default clock; clocked duplex conversion handles differing output rates.

Add a reusable rotary knob to local web controls. It shares slider gesture
ownership, supports linear/logarithmic travel, cancellation, navigation keys,
numeric commits and retryable ends. Consumers draw its arc/indicator from CSS
variables and supply labels. Rotary interaction sends physical values. A reusable
piano widget layers short black keys over full white keys, handles independent
pointer chords/glissando and releases notes on cancellation/blur/disposal.
Shared CSS palette tokens match portable SVG theme colors. Following review of
five alternatives, Amber was selected as the default on 9 October 2026: warm
charcoal/plum surfaces with amber emphasis and a warm paper light companion.
Disabled piano keys retain full opacity so the lower layer cannot show through.
The synth/MPE layouts must fit default and smaller example window sizes.

The teaching synth is oscillator → low-pass VCF → VCA, with one shared ADSR per
voice. Prepare 256 filter-coefficient entries on the control thread and pass
fixed patches through a bounded queue. Waveform switching preserves phase without
crossfading. ADSR edits latch at note-on; other synth controls affect held notes.
MIDI packets arrive at block boundaries; native timestamps are not sample-mapped.
At most 16 MIDI inputs, 256 packets per input and 64 polling attempts per block
bound the example. Queue loss clears voices to avoid stuck notes.

## Pointer adjustment default (10 October 2026)

Shared knobs and slider/faders leave paired numeric editing when a pointer
adjustment starts. Knobs do not force focus; native ranges keep their browser drag
behavior and release pointer focus on completion/cancellation. Shared theme CSS
suppresses pointer focus outlines until keyboard input. Cancelling control click
activation prevents a surrounding label from opening its numeric editor after a
drag. Direct numeric entry and keyboard navigation keep their existing behavior.
This default belongs in the framework controls so all consumers inherit it.

## Status and limits

The portable synth, routing and heap fixtures are implemented. The default
standalone window/popup and knob interaction are macOS WebKit prototypes with
hardware-free native UI fixtures. The existing effect plug-in remains a separate
host-owned integration; no subtractive plug-in or MIDI 2.0 synth/export is added.
MIDI 2.0 UI is a full-resolution codec inspector. Broader desktop/device/controller
validation, assistive technology, long sessions and deadlines remain independent.
See [examples](../examples.md), [native editor](../native-editor.md),
[web controls](../web-controls.md) and [work status](../WORK_STATUS.md).
