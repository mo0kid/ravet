# Reusable local web controls

`ravet-surface::embed_web_controls` embeds the reusable local JavaScript at one
`<!-- RAVET_CONTROLS -->` marker in a trusted consumer-authored page. Missing or
duplicate markers and output larger than 1 MiB reject. Preparation allocates on
the UI thread; the page CSP must permit its inline script. This is a document
assembly helper, with no remote content sanitization or implicit network bridge.

The script exposes `window.RavetControls` with seventeen components and `TextEditorError`:

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
  owner; rejected values preserve the knob display. `setEnabled(boolean)` supplies
  coordinated disabled input/Tab state and exact owned attribute restoration,
  retaining refused ends for retry. See [local control semantics](control-semantics.md)
  for its callback retirement and host/UI acceptance boundaries.
- `PianoKeyboard(root, {firstNote=48, lastNote=72, onNoteOn, onNoteOff,
  computerKeys='awsedftgyhujk', computerStart=60})` constructs a piano with full
  white keys and shorter overlapping black keys. Shared keyboard styling keeps
  both key layers opaque when disabled. Inclusive note bounds are integers
  0..127 with white endpoints and at least two notes. Independent
  pointer IDs play chords; dragging releases the previous key before starting
  the next, including black-key hit testing. Pointer cancellation, blur, hiding,
  disabling and disposal release held notes. `onNoteOn(note, source)` can reject
  a note by returning false; `onNoteOff(source)` retains source identity.
  One piano key participates in Tab order. Left/Right select adjacent chromatic
  keys without wrapping; Home/End select the first/last key. Space/Enter play the
  focused key, retaining one source while either play key is held. Moving focus
  releases that keyboard source once. A document mutation observer releases held
  notes when their keys become disabled, hidden, inert or detached, including
  removal of the piano; `refresh()` provides an explicit immediate check. Mutation
  observation runs at the browser's next mutation checkpoint, and does not observe
  stylesheet replacement/media-query changes; call `refresh()` for those changes.
  Node creation, hit testing and keyboard/blur/visibility listeners use the root's
  own document/window, including same-origin iframe documents. Missing live windows
  reject before markup mutation. The embedding consumer owns iframe lifecycle and
  calls `releaseAll()`/`dispose()` before hiding or retiring a frame itself.
  Dispose disconnects the observer before releasing notes. Space/Enter play focused keys; computer bindings are suppressed while other
  interactive controls have focus. `setEnabled(bool)`, `releaseAll()` and
  `setActiveNotes([notes])` support consumer lifecycle and external MIDI display.
  External note refresh paints without submitting edits and accepts at most 128
  integer notes. The consumer owns MIDI channel assignment and voice limits.
- `RovingFocus(root, members, {orientation='horizontal', wrap=true, rtl=false,
  index=0, onFocus=null})` gives a labelled composite one Tab stop. Supply 1..256
  distinct non-nested element members in the same document and inside the root.
  Left/Right, Up/Down (according to orientation), Home and End move focus without
  activation or edits. RTL explicitly reverses horizontal arrows; `both` uses
  both axes as one ordered list. Modified/default-prevented keys and Tab retain
  browser/consumer behavior. Disabled, hidden, inert, ARIA-disabled, detached and
  non-visible members are skipped. `refresh()` rechecks eligibility and returns
  eligible indices without stealing focus; consumers call it after changing
  membership visibility or disabled state. `focus(index)` accepts an eligible
  member and reports whether the browser accepted focus. Optional `onFocus(node)`
  receives changed selected members after explicit or native focus; it is a
  consumer UI callback, not activation. Dispose restores each original tabindex
  attribute and removes listeners. The consumer supplies the composite role,
  labels and focus style; do not bind text-editing controls whose arrow keys are
  needed for editing. There is no modal trap, automatic layout direction, dynamic
  member discovery or screen-reader certification.
- `SplitPane(root, first, separator, second, options)` supplies actual bounded
  two-pane grid layout, a labelled separator, pixel minima, TooSmall suspension,
  keyboard/pointer ownership and exact cleanup. See [local split-pane layout](split-pane.md).
- `VirtualList(root, items, settings)` copies bounded fixed text models with
  reusable viewport rows, active/pressed identity pins, guarded native pointer
  selection and bounded single/multiple model selection. See
  [fixed-model virtual text list](virtual-list.md).
- `TableView(table, headers, rows, settings)` supplies bounded fixed readonly
  cell navigation, explicit native column-header associations and separate row
  selection with retained unavailable models and exact cleanup. See
  [fixed readonly table grid](table-view.md).
- `TreeView(root, entries, settings)` supplies bounded fixed hierarchical
  expansion/navigation with separate virtual active focus, retained collapsed
  selection, native toggle focus admission and exact cleanup. See
  [fixed hierarchical tree selection](tree-view.md).
- `ListBox(root, options, settings)` separates bounded single/multiple selection
  from one root Tab stop and virtual active focus, with retained unavailable model
  state, keyboard ranges and exact cleanup. See [fixed flat-list selection](listbox.md).
- `ModalDialog(dialog, {label, initialFocus, onClose})` composes native browser
  modality, bounded completion and guarded close-drain ownership. See
  [local native modal dialogs](modal-dialogs.md).
- `TabbedPanels(root, entries, options)` composes bounded labelled native tabs and
  panels with manual or automatic activation, local lifecycle and exact attribute
  restoration. See [local tabbed panels](tabbed-panels.md).
- `PopupMenu(trigger, root, entries, {label, onAction})` supplies one-level
  bounded menu semantics, roving keyboard focus, exact-once native button actions
  and dismissal/attribute restoration. See [local popup menus](popup-menus.md).
- `TextEditor(textarea, options)` owns bounded committed UTF-8 source,
  grapheme-safe directional selection, local revision tokens and bounded undo/redo.
  See [browser text editing](browser-text-editing.md) for its complete API and
  composition/consumer-bridge limits. It has its own `onEdit` notification contract.
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
  `textContent`, without parsing markup; meters send no edits. Explicit labels
  remove competing name references and restore them on disposal; live-pair checks
  reject formatter retirement or stale recursive updates. See [local control
  semantics](control-semantics.md).

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

Knob and slider/fader pointer gestures leave paired numeric editing before
adjustment. They do not select the number or activate a surrounding label's
numeric input when released. Knobs do not force keyboard focus on pointer down;
native range inputs retain browser dragging and release pointer focus on
pointer up/cancel. The shared theme suppresses pointer focus outlines through
`data-ravet-pointer-focus`; keyboard input removes that marker and restores
consumer focus styling. Direct clicks/Tab into numeric inputs still allow normal
selection and committed number entry. Successful disposal restores any original
marker attribute. This policy applies to every shared knob and slider, including
embedded editors and standalone consumers.

Parameter snapshot refresh leaves focused controls and active gestures untouched. Labels, tab order,
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
audits and further widget families remain pending. See the [capability catalogue](FEATURES.md)
for drawing, layout, text, asset and theme capabilities and their backend limits. See [decision 0034](decisions/0034-reusable-web-controls.md) and
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

The reusable focus fixture runs 38 contract checks in both the collaborative
Chromium browser and hidden macOS WebKit: bounded navigation, RTL/vertical
selection, skipped members, original attribute restoration, piano note release
and overlapping play keys. Actual browser Tab/ArrowRight/Enter events also reach
one composite member, leave the composite on the next Tab, and produce one
piano note-on/off source. The browser reports `document.hasFocus() === false`, so
these are keyboard/DOM contracts, with no visual focus-ring or screen-reader
claim. Build the hidden fixture from the native editor project:

```sh
cmake -S crates/ravet-surface/native -B target/native-editor -DRAVET_WEB_EDITOR_TESTS=ON
cmake --build target/native-editor --target ravet-web-focus -j 2
ctest --test-dir target/native-editor -R '^ravet-web-focus$' --output-on-failure
```

The native build assembles the same trusted local scripts/styles inline, matching
consumer embedding and keeping WebKit's local read access restricted. No device
or host window is opened. See [decision 0169](decisions/0169-bounded-web-focus.md).

## Bounded text editing

`RavetControls.TextEditor` wraps one consumer-labelled textarea with UTF-8 source
limits, grapheme-safe selections, local revision ownership, bounded undo/redo and
composition guards. See [browser text editing](browser-text-editing.md) for its
API, native-input fallback and explicit consumer acknowledgement contract.
Browser text editing is independent of Rust document tokens and shaped drawing.

## Document ownership and embedded documents

Parameter controls and action buttons require connected elements in one live
owner document. A slider/knob's optional number input belongs to that same document;
invalid foreign inputs or missing owner windows reject before setting min/max,
ARIA or styling attributes. Same-origin iframe controls use their own document's
active element and window/document event listeners. Focused child controls reject
snapshot refresh rather than letting a parent-document activeElement check overwrite
editing. Slider and knob child-window pointerup/cancel/blur and child-document
visibility signals finish their owned gesture; parent-window signals do not finish
an unrelated child's gesture.

Gesture ownership uses a WeakMap keyed by the owner document. Slider, knob, toggle,
choice, numeric commits and action buttons share the same document's owner; ending
or beginning an edit in a different document does not force its owner to finish.
A rejected end keeps the original owner, listeners and held state for retry, and
blocks another control in that document from beginning or committing. Accepted
retry releases ownership once. Real focus movement still causes native blur and
may end the focused control's gesture.

Consumers keep the original document/window alive, finish/dispose controls before
removing or hiding an iframe, and retry rejected ends before retirement. This does
not observe arbitrary frame/stylesheet lifecycle or imply cross-origin access,
shadow-root focus traversal, native-window focus, screenreader or backend
certification. Isolated Chromium and hidden WebKit iframe fixtures validate the
specified owner-document DOM contracts.
