# Reusable local web controls

`ravet-surface::embed_web_controls` embeds the reusable local JavaScript at one
`<!-- RAVET_CONTROLS -->` marker in a trusted consumer-authored page. Missing or
duplicate markers and output larger than 1 MiB reject. Preparation allocates on
the UI thread; the page CSP must permit its inline script. This is a document
assembly helper, with no remote content sanitization or implicit network bridge.

The script exposes `window.RavetControls` with seven components:

- `Slider(range, numberOrNull, {id, minimum, maximum, step}, post)` binds native
  range/optional number inputs to finite physical parameter values. Bounds must
  be finite/ordered; step is positive or the default `'any'`. Number edits clamp
  and commit. Pointer drags and held navigation keys share begin/value/end
  ownership; cancellation or blur ends a gesture. Each held navigation key is
  tracked: releasing a modifier or only one of several held keys preserves the
  gesture until the last navigation key is released.
- `Knob(element, numberOrNull, {id, minimum, maximum, step=0.01,
  value=minimum, logarithmic=false}, post)` supplies rotary interaction and an
  ARIA slider role. Drag upwards/downwards over 150 pixels for the full travel;
  Shift uses 1,500 pixels. Positive logarithmic bounds enable log travel. Arrow
  keys, Home/End and Page keys edit values; double-click restores the configured
  default. Number entry clamps/quantizes and commits. CSS variables `--angle`
  (-135..135 degrees) and `--sweep` (0..270 degrees) let the consumer draw the
  indicator and arc. The consumer provides labels, markup and theme. Pointer
  cancellation/lost capture, blur and document hiding end the gesture. End
  rejection retains ownership for retry. Knobs and sliders share one gesture
  owner; rejected values preserve the knob display.
- `PianoKeyboard(root, {firstNote=48, lastNote=72, onNoteOn, onNoteOff,
  computerKeys='awsedftgyhujk', computerStart=60})` constructs a piano with full
  white keys and shorter overlapping black keys. Shared keyboard styling keeps
  both key layers opaque when disabled. Inclusive note bounds are integers
  0..127 with white endpoints and at least two notes. Independent
  pointer IDs play chords; dragging releases the previous key before starting
  the next, including black-key hit testing. Pointer cancellation, blur, hiding,
  disabling and disposal release held notes. `onNoteOn(note, source)` can reject
  a note by returning false; `onNoteOff(source)` retains source identity.
  Space/Enter play focused keys; computer bindings are suppressed while other
  interactive controls have focus. `setEnabled(bool)`, `releaseAll()` and
  `setActiveNotes([notes])` support consumer lifecycle and external MIDI display.
  External note refresh paints without submitting edits and accepts at most 128
  integer notes. The consumer owns MIDI channel assignment and voice limits.
- `Toggle(checkbox, id, post)` commits physical zero/one values. `update` accepts
  a boolean host snapshot.
- `Choice(select, id, post)` binds 1..256 unique finite option values, preserving
  their original DOM spelling for refresh. `update` accepts a listed value.
- `ActionButton(button, action, post)` sends a consumer action string of 1..128
  JavaScript code units. Consumers define its behavior.

- `AccessibleMeter(meter, readingElement, {label, minimum=0, maximum=1, format})`
  binds a native meter and visible text element in the same document. `update`
  accepts finite nonnegative `peak`/`rms` and boolean `nonfinite`. Visual RMS
  clamps to the display bounds; accessible and visible text retain actual peak
  and RMS dB, including overload and invalid-audio indications. Labels contain
  1..256 JavaScript code units; optional formatters receive a frozen snapshot
  and must return 1..4096 code units of nonblank text. Invalid readings or
  formatter failures preserve the previous display. Text is assigned through
  `textContent`, without parsing markup; meters send no edits.

Parameter IDs are unsigned 32-bit integers. `post` explicitly receives
`{type: 'edit'|'begin'|'value'|'end', id, value}` or `{type:'action', action}`.
Returning false synchronously rejects submission. Begin rejection leaves a
slider idle; end rejection retains ownership for retry and prevents disposal.
One slider or knob owns a gesture per document; starting another ends the previous one
first. Native asynchronous errors, schema validation, host notification, queue
capacity/reserved end slots and sample timing belong to the consumer transport.
Use [parameter attachments](parameter-attachments.md) for Rust schema mappings.
DOM sliders use physical linear values; rotary knobs optionally use logarithmic
travel while still sending physical values.

For example, after embedding the script:

```js
const slider = new RavetControls.Slider(
  document.querySelector('#gain'), document.querySelector('#gain-number'),
  {id: 1, minimum: 0, maximum: 4, step: 0.001},
  message => window.webkit.messageHandlers.ravet.postMessage(message)
);
slider.update(1); // host refresh sends no edits
```

Refresh leaves focused controls and active gestures untouched. Labels, tab order,
keyboard semantics and focus rendering use native HTML elements and consumer
markup/CSS. `dispose` removes listeners; sliders and knobs first end any active gesture,
returning false if that end was rejected. Retry before replacing controls or
closing the bridge. Native editor owners still close host gestures on teardown.

The gain CLAP page uses the shared slider/number, toggle and accessible meter components. Its
native WebKit probe exercises three complete lifecycle cycles, live gain edits,
host automation, bypass, meters and resize. Additional in-page fixtures check
all five shared components, pointer cancellation, held-key gesture grouping,
begin/end rejection, disposal and choices with noncanonical numeric spelling.
Browser keyboard checks also confirm the page tab order and begin/value/end
submission from an actual navigation key. Browser automation left the document
unfocused, so computed focus rendering is not visual-focus evidence. These are
programmatic WebKit and browser checks. Broader platform runtime, screen-reader
audits, native drawing/layout/font/asset/theme APIs and a complete widget toolkit
remain pending. See [decision 0034](decisions/0034-reusable-web-controls.md) and
[decision 0071](decisions/0071-meter-text-and-held-keys.md).

The [interactive examples](examples.md) use the shared rotary knobs. Their native
fixture checks logarithmic drag, numeric clamping, navigation keys, cancellation,
refresh suppression and retryable/rejected gesture ownership. This programmatic
WebKit evidence is not an assistive-technology audit.

`WEB_THEME_STYLE` supplies `--ravet-background`, `--ravet-panel`,
`--ravet-raised`, `--ravet-border`, `--ravet-foreground`, `--ravet-muted`,
`--ravet-accent`, `--ravet-accent-text` and piano-key colors. The selected Amber
default uses charcoal/plum surfaces and matches `drawing::Theme::DARK`;
`Theme::LIGHT` is a warm paper companion. Consumers override semantic variables
for their own themes. The
standard standalone popup uses the same tokens. Shared styles disable text
selection on UI labels and other body text, including the settings popup.
Inputs, textareas, editable content and preformatted/code output remain
selectable for editing and copying. Consumer CSS can override this policy.

Native piano fixtures check stopped-key opacity, black/white geometry, multi-pointer chords,
glissando ordering, cancellation, focused-key release and nonfeedback external
note highlighting. Synth/MPE fixtures also reject page overflow at default and
smaller window sizes. These checks do not replace assistive-technology testing.
